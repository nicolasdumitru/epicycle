//! Local storage: SQLite through sqlx, with migrations and the change history.
//!
//! Queries are checked against the schema at compile time. The query metadata is
//! committed in `.sqlx/`, so builds don't need a database; after changing a query
//! or a migration, run `python3 scripts/sqlx_prepare.py`.

use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;

use epicycle_core::money::Currency;
use epicycle_core::recurrence::{DayOfMonth, Month, Rule, Schedule};
use epicycle_core::subscription::{Dated, Plan, Status, Subscription, SubscriptionId};
use jiff::Timestamp;
use jiff::civil::Date;
use serde::Serialize;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};
use thiserror::Error;
use uuid::Uuid;

/// Version of the JSON format of revision snapshots, i.e. of the serialized
/// [`Subscription`]. Bump it, and upgrade old snapshots when reading them,
/// whenever that format changes.
const SNAPSHOT_VERSION: i64 = 1;

#[derive(Debug, Error)]
pub enum DbError {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("subscription {} not found", .0.0)]
    NotFound(SubscriptionId),
    #[error("stored data is invalid: {0}")]
    Corrupt(String),
}

fn corrupt(what: impl std::fmt::Display) -> DbError {
    DbError::Corrupt(what.to_string())
}

/// Why a revision was recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Cause {
    Created,
    Edited,
    Deleted,
}

impl Cause {
    fn as_str(self) -> &'static str {
        match self {
            Cause::Created => "created",
            Cause::Edited => "edited",
            Cause::Deleted => "deleted",
        }
    }
}

impl FromStr for Cause {
    type Err = DbError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "created" => Ok(Cause::Created),
            "edited" => Ok(Cause::Edited),
            "deleted" => Ok(Cause::Deleted),
            other => Err(corrupt(format_args!("unknown revision cause {other:?}"))),
        }
    }
}

/// One entry of a subscription's change history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Revision {
    pub id: Uuid,
    pub parent: Option<Uuid>,
    pub device: Uuid,
    pub created_at: Timestamp,
    pub cause: Cause,
    /// The subscription after the change; `None` if it was deleted.
    pub snapshot: Option<Subscription>,
}

pub struct Db {
    pool: SqlitePool,
    device_id: Uuid,
}

impl Db {
    /// Opens (creating if needed) the database at `path` and applies any pending
    /// migrations.
    pub async fn open(path: &Path) -> Result<Db, DbError> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new().connect_with(options).await?;
        Db::init(pool).await
    }

    /// A fresh database that lives only as long as the returned value.
    pub async fn open_in_memory() -> Result<Db, DbError> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
        // Each connection to ":memory:" is a separate database, so keep exactly one.
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .connect_with(options)
            .await?;
        Db::init(pool).await
    }

    async fn init(pool: SqlitePool) -> Result<Db, DbError> {
        sqlx::migrate!().run(&pool).await?;
        // First run on this install: pick a device ID. Later runs keep it.
        let new_device_id = Uuid::now_v7().to_string();
        sqlx::query!(
            "INSERT INTO app_state (id, device_id, reporting_currency) VALUES (1, ?, 'EUR')
             ON CONFLICT (id) DO NOTHING",
            new_device_id
        )
        .execute(&pool)
        .await?;
        let state = sqlx::query!("SELECT device_id FROM app_state WHERE id = 1")
            .fetch_one(&pool)
            .await?;
        let device_id = Uuid::parse_str(&state.device_id).map_err(corrupt)?;
        Ok(Db { pool, device_id })
    }

    pub async fn close(self) {
        self.pool.close().await;
    }

    /// All current subscriptions, ordered by name.
    pub async fn subscriptions(&self) -> Result<Vec<Subscription>, DbError> {
        let rows = sqlx::query!(
            "SELECT id, name, currency FROM subscriptions ORDER BY name COLLATE NOCASE, id"
        )
        .fetch_all(&self.pool)
        .await?;
        let plan_rows = sqlx::query!(
            "SELECT subscription_id, effective_from, unit, every, month, day, anchor, amount
             FROM plans ORDER BY subscription_id, effective_from"
        )
        .fetch_all(&self.pool)
        .await?;
        let status_rows = sqlx::query!(
            "SELECT subscription_id, effective_from, status
             FROM status_changes ORDER BY subscription_id, effective_from"
        )
        .fetch_all(&self.pool)
        .await?;

        let mut plans: HashMap<String, Vec<Dated<Plan>>> = HashMap::new();
        for row in plan_rows {
            let rule = rule_from_columns(&row.unit, row.every, row.month, row.day)?;
            let schedule = Schedule::new(rule, parse_date(&row.anchor)?).map_err(corrupt)?;
            plans.entry(row.subscription_id).or_default().push(Dated {
                from: parse_date(&row.effective_from)?,
                value: Plan {
                    schedule,
                    amount: row.amount,
                },
            });
        }
        let mut statuses: HashMap<String, Vec<Dated<Status>>> = HashMap::new();
        for row in status_rows {
            statuses
                .entry(row.subscription_id)
                .or_default()
                .push(Dated {
                    from: parse_date(&row.effective_from)?,
                    value: status_from_str(&row.status)?,
                });
        }

        rows.into_iter()
            .map(|row| {
                Subscription::from_parts(
                    SubscriptionId(Uuid::parse_str(&row.id).map_err(corrupt)?),
                    row.name,
                    Currency::from_str(&row.currency).map_err(corrupt)?,
                    plans.remove(&row.id).unwrap_or_default(),
                    statuses.remove(&row.id).unwrap_or_default(),
                )
                .map_err(corrupt)
            })
            .collect()
    }

    /// Saves the subscription as its new current state and records a revision.
    /// Returns the revision's ID, or `None` if nothing changed.
    pub async fn save(&self, subscription: &Subscription) -> Result<Option<Uuid>, DbError> {
        let id = subscription.id().0.to_string();
        let snapshot = serde_json::to_string(subscription).map_err(corrupt)?;
        let mut tx = self.pool.begin().await?;

        let parent = latest_revision(&mut tx, &id).await?;
        if let Some(parent) = &parent
            && parent.snapshot.as_deref() == Some(snapshot.as_str())
        {
            return Ok(None);
        }
        // A subscription that exists only as a deletion is being recreated.
        let cause = match &parent {
            Some(p) if p.snapshot.is_some() => Cause::Edited,
            _ => Cause::Created,
        };
        let revision_id = self
            .insert_revision(&mut tx, &id, parent.map(|p| p.id), cause, Some(&snapshot))
            .await?;

        let currency = subscription.currency().code();
        let name = subscription.name();
        sqlx::query!(
            "INSERT INTO subscriptions (id, name, currency, head_revision_id) VALUES (?, ?, ?, ?)
             ON CONFLICT (id) DO UPDATE SET
                 name = excluded.name,
                 currency = excluded.currency,
                 head_revision_id = excluded.head_revision_id",
            id,
            name,
            currency,
            revision_id
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query!("DELETE FROM plans WHERE subscription_id = ?", id)
            .execute(&mut *tx)
            .await?;
        for plan in subscription.plans() {
            let (unit, every, month, day) = rule_columns(plan.value.schedule.rule());
            let from = plan.from.to_string();
            let anchor = plan.value.schedule.anchor().to_string();
            sqlx::query!(
                "INSERT INTO plans
                     (subscription_id, effective_from, unit, every, month, day, anchor, amount)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                id,
                from,
                unit,
                every,
                month,
                day,
                anchor,
                plan.value.amount
            )
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query!("DELETE FROM status_changes WHERE subscription_id = ?", id)
            .execute(&mut *tx)
            .await?;
        for change in subscription.status_changes() {
            let from = change.from.to_string();
            let status = status_as_str(change.value);
            sqlx::query!(
                "INSERT INTO status_changes (subscription_id, effective_from, status)
                 VALUES (?, ?, ?)",
                id,
                from,
                status
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(Some(Uuid::parse_str(&revision_id).map_err(corrupt)?))
    }

    /// Deletes a subscription. Its history is kept, ending in a deletion revision.
    pub async fn delete(&self, id: SubscriptionId) -> Result<Uuid, DbError> {
        let key = id.0.to_string();
        let mut tx = self.pool.begin().await?;
        let exists = sqlx::query!("SELECT id FROM subscriptions WHERE id = ?", key)
            .fetch_optional(&mut *tx)
            .await?
            .is_some();
        if !exists {
            return Err(DbError::NotFound(id));
        }
        let parent = latest_revision(&mut tx, &key).await?.map(|p| p.id);
        let revision_id = self
            .insert_revision(&mut tx, &key, parent, Cause::Deleted, None)
            .await?;
        // Plans and status changes go with it (ON DELETE CASCADE).
        sqlx::query!("DELETE FROM subscriptions WHERE id = ?", key)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Uuid::parse_str(&revision_id).map_err(corrupt)
    }

    /// A subscription's full history, oldest first.
    pub async fn history(&self, id: SubscriptionId) -> Result<Vec<Revision>, DbError> {
        let key = id.0.to_string();
        let rows = sqlx::query!(
            "SELECT id, parent_id, device_id, created_at, cause, snapshot_version, snapshot
             FROM revisions WHERE subscription_id = ? ORDER BY id",
            key
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                if row.snapshot_version != SNAPSHOT_VERSION {
                    return Err(corrupt(format_args!(
                        "unsupported snapshot version {}",
                        row.snapshot_version
                    )));
                }
                Ok(Revision {
                    id: Uuid::parse_str(&row.id).map_err(corrupt)?,
                    parent: row
                        .parent_id
                        .as_deref()
                        .map(Uuid::parse_str)
                        .transpose()
                        .map_err(corrupt)?,
                    device: Uuid::parse_str(&row.device_id).map_err(corrupt)?,
                    created_at: row.created_at.parse().map_err(corrupt)?,
                    cause: row.cause.parse()?,
                    snapshot: row
                        .snapshot
                        .as_deref()
                        .map(serde_json::from_str)
                        .transpose()
                        .map_err(corrupt)?,
                })
            })
            .collect()
    }

    async fn insert_revision(
        &self,
        tx: &mut sqlx::SqliteConnection,
        subscription_id: &str,
        parent: Option<String>,
        cause: Cause,
        snapshot: Option<&str>,
    ) -> Result<String, DbError> {
        // UUIDv7 IDs sort by creation time, which orders the history.
        let id = Uuid::now_v7().to_string();
        let device = self.device_id.to_string();
        let created_at = Timestamp::now().to_string();
        let cause = cause.as_str();
        sqlx::query!(
            "INSERT INTO revisions
                 (id, subscription_id, parent_id, device_id, created_at, cause,
                  snapshot_version, snapshot)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            id,
            subscription_id,
            parent,
            device,
            created_at,
            cause,
            SNAPSHOT_VERSION,
            snapshot
        )
        .execute(&mut *tx)
        .await?;
        Ok(id)
    }
}

struct LatestRevision {
    id: String,
    snapshot: Option<String>,
}

async fn latest_revision(
    tx: &mut sqlx::SqliteConnection,
    subscription_id: &str,
) -> Result<Option<LatestRevision>, DbError> {
    let row = sqlx::query!(
        "SELECT id, snapshot FROM revisions WHERE subscription_id = ? ORDER BY id DESC LIMIT 1",
        subscription_id
    )
    .fetch_optional(&mut *tx)
    .await?;
    Ok(row.map(|r| LatestRevision {
        id: r.id,
        snapshot: r.snapshot,
    }))
}

fn parse_date(s: &str) -> Result<Date, DbError> {
    s.parse().map_err(corrupt)
}

/// `(unit, every, month, day)` columns for a rule.
fn rule_columns(rule: Rule) -> (&'static str, i64, Option<i64>, Option<i64>) {
    match rule {
        Rule::Days { every } => ("days", every.get().into(), None, None),
        Rule::Weeks { every } => ("weeks", every.get().into(), None, None),
        Rule::Months { every, day } => ("months", every.get().into(), None, Some(day.get().into())),
        Rule::Years { every, month, day } => (
            "years",
            every.get().into(),
            Some(month.get().into()),
            Some(day.get().into()),
        ),
    }
}

fn rule_from_columns(
    unit: &str,
    every: i64,
    month: Option<i64>,
    day: Option<i64>,
) -> Result<Rule, DbError> {
    let every = u16::try_from(every)
        .ok()
        .and_then(std::num::NonZeroU16::new)
        .ok_or_else(|| corrupt(format_args!("invalid interval {every}")))?;
    let small = |value: Option<i64>, what: &str| -> Result<u8, DbError> {
        value
            .and_then(|v| u8::try_from(v).ok())
            .ok_or_else(|| corrupt(format_args!("missing or invalid {what} for {unit} rule")))
    };
    match unit {
        "days" => Ok(Rule::Days { every }),
        "weeks" => Ok(Rule::Weeks { every }),
        "months" => Ok(Rule::Months {
            every,
            day: DayOfMonth::new(small(day, "day")?).map_err(corrupt)?,
        }),
        "years" => Ok(Rule::Years {
            every,
            month: Month::new(small(month, "month")?).map_err(corrupt)?,
            day: DayOfMonth::new(small(day, "day")?).map_err(corrupt)?,
        }),
        other => Err(corrupt(format_args!("unknown rule unit {other:?}"))),
    }
}

fn status_as_str(status: Status) -> &'static str {
    match status {
        Status::Active => "active",
        Status::Paused => "paused",
        Status::Cancelled => "cancelled",
    }
}

fn status_from_str(s: &str) -> Result<Status, DbError> {
    match s {
        "active" => Ok(Status::Active),
        "paused" => Ok(Status::Paused),
        "cancelled" => Ok(Status::Cancelled),
        other => Err(corrupt(format_args!("unknown status {other:?}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;
    use std::num::NonZeroU16;

    fn netflix() -> Subscription {
        let rule = Rule::Months {
            every: NonZeroU16::MIN,
            day: DayOfMonth::LAST,
        };
        let plan = Plan {
            schedule: Schedule::new(rule, date(2026, 1, 31)).unwrap(),
            amount: 1299,
        };
        Subscription::new(
            SubscriptionId::generate(),
            "Netflix",
            Currency::EUR,
            date(2026, 1, 31),
            plan,
        )
        .unwrap()
    }

    fn yearly_plan(anchor: Date, amount: i64) -> Plan {
        let rule = Rule::Years {
            every: NonZeroU16::new(2).unwrap(),
            month: Month::new(anchor.month() as u8).unwrap(),
            day: DayOfMonth::new(anchor.day() as u8).unwrap(),
        };
        Plan {
            schedule: Schedule::new(rule, anchor).unwrap(),
            amount,
        }
    }

    #[tokio::test]
    async fn saves_and_loads_every_part_of_a_subscription() {
        let db = Db::open_in_memory().await.unwrap();
        let s = netflix()
            .with_plan(date(2026, 6, 1), yearly_plan(date(2026, 6, 1), 9900))
            .unwrap()
            .with_plan(
                date(2027, 1, 1),
                Plan {
                    schedule: Schedule::new(
                        Rule::Weeks {
                            every: NonZeroU16::MIN,
                        },
                        date(2027, 1, 1),
                    )
                    .unwrap(),
                    amount: 250,
                },
            )
            .unwrap()
            .with_status(date(2026, 3, 1), Status::Paused)
            .unwrap()
            .with_status(date(2026, 4, 1), Status::Active)
            .unwrap();
        db.save(&s).await.unwrap();
        assert_eq!(db.subscriptions().await.unwrap(), [s]);
    }

    #[tokio::test]
    async fn every_change_appends_a_revision_linked_to_its_parent() {
        let db = Db::open_in_memory().await.unwrap();
        let s = netflix();
        let first = db.save(&s).await.unwrap().unwrap();
        let renamed = s.renamed("Netflix Premium").unwrap();
        let second = db.save(&renamed).await.unwrap().unwrap();
        let deleted = db.delete(s.id()).await.unwrap();

        let history = db.history(s.id()).await.unwrap();
        let summary: Vec<_> = history.iter().map(|r| (r.id, r.parent, r.cause)).collect();
        assert_eq!(
            summary,
            [
                (first, None, Cause::Created),
                (second, Some(first), Cause::Edited),
                (deleted, Some(second), Cause::Deleted),
            ]
        );
        assert_eq!(history[0].snapshot.as_ref(), Some(&s));
        assert_eq!(history[1].snapshot.as_ref(), Some(&renamed));
        assert_eq!(history[2].snapshot, None);
        assert!(db.subscriptions().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn saving_an_unchanged_subscription_records_nothing() {
        let db = Db::open_in_memory().await.unwrap();
        let s = netflix();
        assert!(db.save(&s).await.unwrap().is_some());
        assert_eq!(db.save(&s).await.unwrap(), None);
        assert_eq!(db.history(s.id()).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn recreating_a_deleted_subscription_continues_its_history() {
        let db = Db::open_in_memory().await.unwrap();
        let s = netflix();
        db.save(&s).await.unwrap();
        let deleted = db.delete(s.id()).await.unwrap();
        let restored = db.save(&s).await.unwrap().unwrap();
        let history = db.history(s.id()).await.unwrap();
        assert_eq!(history.last().unwrap().id, restored);
        assert_eq!(history.last().unwrap().parent, Some(deleted));
        assert_eq!(history.last().unwrap().cause, Cause::Created);
        assert_eq!(db.subscriptions().await.unwrap(), [s]);
    }

    #[tokio::test]
    async fn deleting_a_missing_subscription_is_an_error() {
        let db = Db::open_in_memory().await.unwrap();
        let id = SubscriptionId::generate();
        assert!(matches!(db.delete(id).await, Err(DbError::NotFound(missing)) if missing == id));
    }

    #[tokio::test]
    async fn data_and_device_id_survive_reopening() {
        let dir = std::env::temp_dir().join(format!("epicycle-test-{}", Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("epicycle.sqlite3");
        let s = netflix();

        let db = Db::open(&path).await.unwrap();
        let device = db.device_id;
        db.save(&s).await.unwrap();
        db.close().await;

        let db = Db::open(&path).await.unwrap();
        assert_eq!(db.subscriptions().await.unwrap(), std::slice::from_ref(&s));
        assert_eq!(db.device_id, device);
        assert_eq!(db.history(s.id()).await.unwrap()[0].device, device);
        db.close().await;
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn the_schema_rejects_invalid_rows() {
        let db = Db::open_in_memory().await.unwrap();
        let s = netflix();
        db.save(&s).await.unwrap();
        let id = s.id().0.to_string();
        // A monthly rule needs a day; a negative amount is never valid.
        let missing_day = sqlx::query(
            "INSERT INTO plans VALUES (?, '2027-01-01', 'months', 1, NULL, NULL, '2027-01-01', 100)",
        )
        .bind(&id)
        .execute(&db.pool)
        .await;
        assert!(missing_day.is_err());
        let negative = sqlx::query(
            "INSERT INTO plans VALUES (?, '2027-01-01', 'days', 1, NULL, NULL, '2027-01-01', -5)",
        )
        .bind(&id)
        .execute(&db.pool)
        .await;
        assert!(negative.is_err());
    }
}
