//! Local storage: SQLite through sqlx, with migrations and the change history.
//!
//! Queries are checked against the schema at compile time. The query metadata is
//! committed in `.sqlx/`, so builds don't need a database; after changing a query
//! or a migration, run `just sqlx-prepare`.

use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;

use epicycle_core::backup::{Backup, Settings};
use epicycle_core::history::{
    Cause, History, HistoryError, ImportMode, ImportPlan, NewHead, Revision, RevisionId,
    plan_import,
};
use epicycle_core::money::Currency;
use epicycle_core::recurrence::{DayOfMonth, Month, Rule, Schedule};
use epicycle_core::subscription::{Dated, Plan, Status, Subscription, SubscriptionId};
use jiff::Timestamp;
use jiff::civil::Date;
use sqlx::SqliteConnection;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};
use thiserror::Error;
use uuid::Uuid;

/// Version of the JSON format of revision snapshots, i.e. of the serialized
/// [`Subscription`]. Bump it, and upgrade old snapshots when reading them,
/// whenever that format changes.
///
/// - 1: the first format.
/// - 2: adds `notes`, left out when empty. Version 1 snapshots read as version
///   2 snapshots without notes, so they need no upgrade.
const SNAPSHOT_VERSION: i64 = 2;

#[derive(Debug, Error)]
pub enum DbError {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("subscription {0} not found")]
    NotFound(SubscriptionId),
    #[error(transparent)]
    History(#[from] HistoryError),
    #[error("stored data is invalid: {0}")]
    Corrupt(String),
}

fn corrupt(what: impl std::fmt::Display) -> DbError {
    DbError::Corrupt(what.to_string())
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
            "SELECT id, name, currency, notes FROM subscriptions ORDER BY name COLLATE NOCASE, id"
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
                .map(|s| s.with_notes(&row.notes))
                .map_err(corrupt)
            })
            .collect()
    }

    /// Saves the subscription as its new current state and records a revision.
    /// Returns the revision's ID, or `None` if nothing changed.
    pub async fn save(&self, subscription: &Subscription) -> Result<Option<RevisionId>, DbError> {
        let snapshot = serde_json::to_string(subscription).map_err(corrupt)?;
        let mut tx = self.pool.begin().await?;

        let head = head_revision(&mut tx, subscription.id()).await?;
        if let Some(head) = &head
            && head.snapshot.as_deref() == Some(snapshot.as_str())
        {
            return Ok(None);
        }
        // A subscription that exists only as a deletion is being recreated.
        let cause = match &head {
            Some(h) if h.snapshot.is_some() => Cause::Edited,
            _ => Cause::Created,
        };
        let revision = self.new_revision(
            subscription.id(),
            head.map(|h| h.id).into_iter().collect(),
            cause,
            Some(subscription.clone()),
        );
        insert_revision(&mut tx, &revision).await?;
        write_current(&mut tx, subscription, revision.id).await?;
        tx.commit().await?;
        Ok(Some(revision.id))
    }

    /// Deletes a subscription. Its history is kept, ending in a deletion revision.
    pub async fn delete(&self, id: SubscriptionId) -> Result<RevisionId, DbError> {
        let key = id.0.to_string();
        let mut tx = self.pool.begin().await?;
        let exists = sqlx::query!("SELECT id FROM subscriptions WHERE id = ?", key)
            .fetch_optional(&mut *tx)
            .await?
            .is_some();
        if !exists {
            return Err(DbError::NotFound(id));
        }
        let head = head_revision(&mut tx, id).await?;
        let revision = self.new_revision(
            id,
            head.map(|h| h.id).into_iter().collect(),
            Cause::Deleted,
            None,
        );
        insert_revision(&mut tx, &revision).await?;
        remove_current(&mut tx, id).await?;
        tx.commit().await?;
        Ok(revision.id)
    }

    /// A subscription's full history, oldest first.
    pub async fn history(&self, id: SubscriptionId) -> Result<Vec<Revision>, DbError> {
        let key = id.0.to_string();
        let rows = sqlx::query_as!(
            RevisionRow,
            "SELECT id, subscription_id, parent_id, merge_parent_id, device_id, created_at,
                    cause, snapshot_version, snapshot
             FROM revisions WHERE subscription_id = ? ORDER BY id",
            key
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Revision::try_from).collect()
    }

    /// Everything a backup holds: the whole history and the settings.
    pub async fn export(&self) -> Result<Backup, DbError> {
        let mut conn = self.pool.acquire().await?;
        Ok(Backup {
            exported_at: Timestamp::now(),
            device_id: self.device_id,
            settings: settings(&mut conn).await?,
            history: load_history(&mut conn).await?,
        })
    }

    /// What importing `backup` would do, without changing anything.
    pub async fn preview_import(
        &self,
        backup: &Backup,
        mode: ImportMode,
    ) -> Result<ImportPlan, DbError> {
        let mut conn = self.pool.acquire().await?;
        let local = load_history(&mut conn).await?;
        Ok(plan_import(&local, &backup.history, mode)?)
    }

    /// Imports a backup in one transaction: either all of it applies or, on any
    /// error, nothing does. Returns what was done.
    pub async fn import(&self, backup: &Backup, mode: ImportMode) -> Result<ImportPlan, DbError> {
        let mut tx = self.pool.begin().await?;
        let local = load_history(&mut tx).await?;
        let plan = plan_import(&local, &backup.history, mode)?;

        // Parents first, so every parent exists when its child is inserted.
        for revision in &plan.new_revisions {
            insert_revision(&mut tx, revision).await?;
        }
        for change in &plan.changes {
            let head = match &change.head {
                NewHead::Existing(id) => *id,
                NewHead::Record { parents, cause } => {
                    let revision = self.new_revision(
                        change.subscription_id,
                        parents.clone(),
                        *cause,
                        change.after.clone(),
                    );
                    insert_revision(&mut tx, &revision).await?;
                    revision.id
                }
            };
            match &change.after {
                Some(subscription) => write_current(&mut tx, subscription, head).await?,
                None => remove_current(&mut tx, change.subscription_id).await?,
            }
        }
        if mode == ImportMode::Replace {
            let currency = backup.settings.reporting_currency.code();
            sqlx::query!(
                "UPDATE app_state SET reporting_currency = ? WHERE id = 1",
                currency
            )
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(plan)
    }

    /// A revision recorded now, on this device.
    fn new_revision(
        &self,
        subscription_id: SubscriptionId,
        parents: Vec<RevisionId>,
        cause: Cause,
        subscription: Option<Subscription>,
    ) -> Revision {
        Revision {
            // UUIDv7 IDs sort by creation time, which orders the history.
            id: RevisionId(Uuid::now_v7()),
            subscription_id,
            parents,
            device_id: self.device_id,
            created_at: Timestamp::now(),
            cause,
            subscription,
        }
    }
}

/// A row of `revisions`, as read by `query_as!`.
struct RevisionRow {
    id: String,
    subscription_id: String,
    parent_id: Option<String>,
    merge_parent_id: Option<String>,
    device_id: String,
    created_at: String,
    cause: String,
    snapshot_version: i64,
    snapshot: Option<String>,
}

impl TryFrom<RevisionRow> for Revision {
    type Error = DbError;

    fn try_from(row: RevisionRow) -> Result<Self, Self::Error> {
        if !(1..=SNAPSHOT_VERSION).contains(&row.snapshot_version) {
            return Err(corrupt(format_args!(
                "unsupported snapshot version {}",
                row.snapshot_version
            )));
        }
        let uuid = |s: &str| Uuid::parse_str(s).map_err(corrupt);
        Ok(Revision {
            id: RevisionId(uuid(&row.id)?),
            subscription_id: SubscriptionId(uuid(&row.subscription_id)?),
            parents: [row.parent_id, row.merge_parent_id]
                .into_iter()
                .flatten()
                .map(|p| uuid(&p).map(RevisionId))
                .collect::<Result<_, _>>()?,
            device_id: uuid(&row.device_id)?,
            created_at: row.created_at.parse().map_err(corrupt)?,
            cause: row.cause.parse().map_err(corrupt)?,
            subscription: row
                .snapshot
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .map_err(corrupt)?,
        })
    }
}

async fn load_history(conn: &mut SqliteConnection) -> Result<History, DbError> {
    let rows = sqlx::query_as!(
        RevisionRow,
        "SELECT id, subscription_id, parent_id, merge_parent_id, device_id, created_at,
                cause, snapshot_version, snapshot
         FROM revisions"
    )
    .fetch_all(&mut *conn)
    .await?;
    let revisions = rows
        .into_iter()
        .map(Revision::try_from)
        .collect::<Result<Vec<_>, _>>()?;
    History::new(revisions).map_err(corrupt)
}

async fn settings(conn: &mut SqliteConnection) -> Result<Settings, DbError> {
    let row = sqlx::query!("SELECT reporting_currency FROM app_state WHERE id = 1")
        .fetch_one(&mut *conn)
        .await?;
    Ok(Settings {
        reporting_currency: row.reporting_currency.parse().map_err(corrupt)?,
    })
}

async fn insert_revision(conn: &mut SqliteConnection, revision: &Revision) -> Result<(), DbError> {
    let id = revision.id.0.to_string();
    let subscription_id = revision.subscription_id.0.to_string();
    let parent = revision.parents.first().map(|p| p.0.to_string());
    let merge_parent = revision.parents.get(1).map(|p| p.0.to_string());
    let device = revision.device_id.to_string();
    let created_at = revision.created_at.to_string();
    let cause = revision.cause.as_str();
    let snapshot = revision
        .subscription
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(corrupt)?;
    sqlx::query!(
        "INSERT INTO revisions
             (id, subscription_id, parent_id, merge_parent_id, device_id, created_at, cause,
              snapshot_version, snapshot)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        id,
        subscription_id,
        parent,
        merge_parent,
        device,
        created_at,
        cause,
        SNAPSHOT_VERSION,
        snapshot
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Makes `subscription` the current state, with `head` as its latest revision.
async fn write_current(
    conn: &mut SqliteConnection,
    subscription: &Subscription,
    head: RevisionId,
) -> Result<(), DbError> {
    let id = subscription.id().0.to_string();
    let head = head.0.to_string();
    let currency = subscription.currency().code();
    let name = subscription.name();
    let notes = subscription.notes();
    sqlx::query!(
        "INSERT INTO subscriptions (id, name, currency, notes, head_revision_id)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT (id) DO UPDATE SET
             name = excluded.name,
             currency = excluded.currency,
             notes = excluded.notes,
             head_revision_id = excluded.head_revision_id",
        id,
        name,
        currency,
        notes,
        head
    )
    .execute(&mut *conn)
    .await?;

    sqlx::query!("DELETE FROM plans WHERE subscription_id = ?", id)
        .execute(&mut *conn)
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
        .execute(&mut *conn)
        .await?;
    }

    sqlx::query!("DELETE FROM status_changes WHERE subscription_id = ?", id)
        .execute(&mut *conn)
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
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Removes a subscription from the current state; its history stays.
async fn remove_current(conn: &mut SqliteConnection, id: SubscriptionId) -> Result<(), DbError> {
    let key = id.0.to_string();
    // Plans and status changes go with it (ON DELETE CASCADE).
    sqlx::query!("DELETE FROM subscriptions WHERE id = ?", key)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

struct Head {
    id: RevisionId,
    snapshot: Option<String>,
}

/// A subscription's latest revision: the one no other revision replaces.
async fn head_revision(
    conn: &mut SqliteConnection,
    id: SubscriptionId,
) -> Result<Option<Head>, DbError> {
    let key = id.0.to_string();
    let row = sqlx::query!(
        r#"SELECT r.id AS "id!", r.snapshot FROM revisions AS r
           WHERE r.subscription_id = ?
             AND NOT EXISTS (
                 SELECT 1 FROM revisions AS c
                 WHERE c.parent_id = r.id OR c.merge_parent_id = r.id
             )"#,
        key
    )
    .fetch_optional(&mut *conn)
    .await?;
    row.map(|r| {
        Ok(Head {
            id: RevisionId(Uuid::parse_str(&r.id).map_err(corrupt)?),
            snapshot: r.snapshot,
        })
    })
    .transpose()
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
    use epicycle_core::history::Side;
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

    fn named(name: &str) -> Subscription {
        netflix().renamed(name).unwrap()
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
            .unwrap()
            .with_notes("Shared with the family.\nCancel before the price rise.");
        db.save(&s).await.unwrap();
        assert_eq!(db.subscriptions().await.unwrap(), std::slice::from_ref(&s));
        let history = db.history(s.id()).await.unwrap();
        assert_eq!(history.last().unwrap().subscription.as_ref(), Some(&s));
    }

    #[tokio::test]
    async fn reads_version_1_snapshots() {
        let db = Db::open_in_memory().await.unwrap();
        let s = netflix();
        let revision = db.save(&s).await.unwrap().unwrap().0.to_string();
        // Revisions saved before notes existed have snapshot version 1. Without
        // notes, the JSON is the same.
        sqlx::query("UPDATE revisions SET snapshot_version = 1 WHERE id = ?")
            .bind(&revision)
            .execute(&db.pool)
            .await
            .unwrap();
        let history = db.history(s.id()).await.unwrap();
        assert_eq!(history[0].subscription.as_ref(), Some(&s));
        // And saving it unchanged still records nothing.
        assert_eq!(db.save(&s).await.unwrap(), None);
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
        let summary: Vec<_> = history
            .iter()
            .map(|r| (r.id, r.parents.clone(), r.cause))
            .collect();
        assert_eq!(
            summary,
            [
                (first, vec![], Cause::Created),
                (second, vec![first], Cause::Edited),
                (deleted, vec![second], Cause::Deleted),
            ]
        );
        assert_eq!(history[0].subscription.as_ref(), Some(&s));
        assert_eq!(history[1].subscription.as_ref(), Some(&renamed));
        assert_eq!(history[2].subscription, None);
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
        assert_eq!(history.last().unwrap().parents, [deleted]);
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
        assert_eq!(db.history(s.id()).await.unwrap()[0].device_id, device);
        db.close().await;
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn the_schema_rejects_invalid_rows() {
        let db = Db::open_in_memory().await.unwrap();
        let s = netflix();
        let first = db.save(&s).await.unwrap().unwrap().0.to_string();
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
        // A second parent needs a first one, and a different one.
        for (parent, merge_parent) in [(None, Some(&first)), (Some(&first), Some(&first))] {
            let bad_merge = sqlx::query(
                "INSERT INTO revisions (id, subscription_id, parent_id, merge_parent_id,
                     device_id, created_at, cause, snapshot_version, snapshot)
                 VALUES (?, ?, ?, ?, 'd', '2026-01-01T00:00:00Z', 'deleted', 1, NULL)",
            )
            .bind(Uuid::now_v7().to_string())
            .bind(&id)
            .bind(parent)
            .bind(merge_parent)
            .execute(&db.pool)
            .await;
            assert!(bad_merge.is_err());
        }
    }

    /// Exports `db` to JSON and reads it back, as a file would be.
    async fn backup_of(db: &Db) -> Backup {
        Backup::from_json(&db.export().await.unwrap().to_json()).unwrap()
    }

    #[tokio::test]
    async fn backups_round_trip_without_loss_or_duplicates() {
        let original = Db::open_in_memory().await.unwrap();
        let edited = netflix();
        original.save(&edited).await.unwrap();
        let edited = edited
            .with_plan(date(2026, 6, 1), yearly_plan(date(2026, 6, 1), 9900))
            .unwrap()
            .with_status(date(2026, 9, 1), Status::Paused)
            .unwrap();
        original.save(&edited).await.unwrap();
        let deleted = named("Deleted");
        original.save(&deleted).await.unwrap();
        original.delete(deleted.id()).await.unwrap();
        let recreated = named("Recreated");
        original.save(&recreated).await.unwrap();
        original.delete(recreated.id()).await.unwrap();
        original.save(&recreated).await.unwrap();

        let exported = original.export().await.unwrap();
        let copy = Db::open_in_memory().await.unwrap();
        let plan = copy
            .import(&backup_of(&original).await, ImportMode::Merge)
            .await
            .unwrap();
        assert_eq!(plan.added(), 2);
        assert_eq!(plan.conflicts().count(), 0);

        // Same subscriptions, and exporting again gives the same backup.
        assert_eq!(
            copy.subscriptions().await.unwrap(),
            original.subscriptions().await.unwrap()
        );
        let mut reexported = copy.export().await.unwrap();
        reexported.exported_at = exported.exported_at;
        reexported.device_id = exported.device_id;
        assert_eq!(reexported.to_json(), exported.to_json());

        // Importing the same backup again, in either mode, changes nothing.
        for mode in [ImportMode::Merge, ImportMode::Replace] {
            let again = copy
                .import(&backup_of(&original).await, mode)
                .await
                .unwrap();
            assert_eq!(again, ImportPlan::default());
            assert_eq!(
                copy.export().await.unwrap().history,
                original.export().await.unwrap().history
            );
        }
    }

    #[tokio::test]
    async fn devices_exchanging_backups_converge() {
        let laptop = Db::open_in_memory().await.unwrap();
        let phone = Db::open_in_memory().await.unwrap();
        let s = netflix();
        laptop.save(&s).await.unwrap();
        phone
            .import(&backup_of(&laptop).await, ImportMode::Merge)
            .await
            .unwrap();

        // Both change it separately; the laptop's change is the later one.
        phone.save(&s.renamed("Phone").unwrap()).await.unwrap();
        laptop.save(&s.renamed("Laptop").unwrap()).await.unwrap();

        let plan = laptop
            .import(&backup_of(&phone).await, ImportMode::Merge)
            .await
            .unwrap();
        let conflicts: Vec<_> = plan.conflicts().collect();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].kept, Side::Local);
        assert_eq!(laptop.subscriptions().await.unwrap()[0].name(), "Laptop");
        // The phone's version stays in the history, under a revision with two parents.
        let history = laptop.history(s.id()).await.unwrap();
        assert_eq!(history.len(), 4);
        assert!(
            history
                .iter()
                .any(|r| r.subscription.as_ref().is_some_and(|s| s.name() == "Phone"))
        );
        assert_eq!(history.last().unwrap().parents.len(), 2);
        assert_eq!(history.last().unwrap().cause, Cause::Imported);

        // The phone sees the laptop already settled it: no conflict, same history.
        let plan = phone
            .import(&backup_of(&laptop).await, ImportMode::Merge)
            .await
            .unwrap();
        assert_eq!(plan.conflicts().count(), 0);
        assert_eq!(plan.updated(), 1);
        assert_eq!(phone.subscriptions().await.unwrap()[0].name(), "Laptop");
        assert_eq!(
            phone.export().await.unwrap().history,
            laptop.export().await.unwrap().history
        );

        // And the phone's own later edits continue from the shared history.
        phone
            .save(&s.renamed("Phone again").unwrap())
            .await
            .unwrap();
        let plan = laptop
            .import(&backup_of(&phone).await, ImportMode::Merge)
            .await
            .unwrap();
        assert_eq!(plan.conflicts().count(), 0);
        assert_eq!(
            laptop.subscriptions().await.unwrap()[0].name(),
            "Phone again"
        );
    }

    #[tokio::test]
    async fn an_old_backup_does_not_bring_back_deleted_subscriptions() {
        let db = Db::open_in_memory().await.unwrap();
        let s = netflix();
        db.save(&s).await.unwrap();
        let old = backup_of(&db).await;
        db.delete(s.id()).await.unwrap();
        let plan = db.import(&old, ImportMode::Merge).await.unwrap();
        assert_eq!(plan, ImportPlan::default());
        assert!(db.subscriptions().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn replace_matches_the_backup_and_keeps_what_it_replaced() {
        let other = Db::open_in_memory().await.unwrap();
        let theirs = named("Theirs");
        other.save(&theirs).await.unwrap();
        sqlx::query("UPDATE app_state SET reporting_currency = 'USD'")
            .execute(&other.pool)
            .await
            .unwrap();

        let db = Db::open_in_memory().await.unwrap();
        let mine = named("Mine");
        db.save(&mine).await.unwrap();

        let backup = backup_of(&other).await;
        let preview = db
            .preview_import(&backup, ImportMode::Replace)
            .await
            .unwrap();
        assert_eq!((preview.added(), preview.removed()), (1, 1));
        // Previewing changes nothing.
        assert_eq!(
            db.subscriptions().await.unwrap(),
            std::slice::from_ref(&mine)
        );

        let plan = db.import(&backup, ImportMode::Replace).await.unwrap();
        assert_eq!(plan, preview);
        assert_eq!(db.subscriptions().await.unwrap(), [theirs]);
        let replaced = db.history(mine.id()).await.unwrap();
        assert_eq!(replaced[0].subscription.as_ref(), Some(&mine));
        assert_eq!(replaced.last().unwrap().cause, Cause::Deleted);
        let mut conn = db.pool.acquire().await.unwrap();
        assert_eq!(
            settings(&mut conn).await.unwrap().reporting_currency.code(),
            "USD"
        );
    }

    #[tokio::test]
    async fn a_failed_import_changes_nothing() {
        let db = Db::open_in_memory().await.unwrap();
        let s = netflix();
        db.save(&s).await.unwrap();
        let before = db.export().await.unwrap();

        // A backup claiming a revision this device has, but with other contents.
        let mut json: serde_json::Value = serde_json::from_str(&before.to_json()).unwrap();
        json["revisions"][0]["subscription"]["name"] = "Tampered".into();
        let tampered = Backup::from_json(&json.to_string()).unwrap();
        for mode in [ImportMode::Merge, ImportMode::Replace] {
            assert!(matches!(
                db.import(&tampered, mode).await,
                Err(DbError::History(HistoryError::Mismatch(_)))
            ));
        }
        assert_eq!(db.export().await.unwrap().history, before.history);
        assert_eq!(db.subscriptions().await.unwrap(), [s]);
    }
}
