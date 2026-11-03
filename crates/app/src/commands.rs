//! Tauri commands: the boundary between the UI and the Rust side.
//!
//! Commands take and return the plain data-transfer types defined here rather than
//! domain types, so the domain crate stays free of UI concerns. TypeScript
//! versions of these types are generated into `ui/src/lib/bindings/` by ts-rs
//! whenever `cargo test` runs.

use std::num::NonZeroU16;
use std::path::Path;

use epicycle_core::backup::Backup;
use epicycle_core::commitment::{commitment_on, total_commitment_on};
use epicycle_core::history::{self, ImportPlan, Side};
use epicycle_core::money::{Currency, Fractional, Money};
use epicycle_core::recurrence::{DayOfMonth, Month, Rule, Schedule};
use epicycle_core::subscription::{Plan, Status, Subscription, SubscriptionId};
use jiff::civil::Date;
use serde::{Deserialize, Serialize, Serializer};
use tauri::State;
use ts_rs::TS;
use uuid::Uuid;

use crate::db::Db;

/// An error to show to the user. It reaches the UI as its message.
#[derive(Debug)]
pub struct CommandError(String);

impl<E: std::fmt::Display> From<E> for CommandError {
    fn from(error: E) -> Self {
        CommandError(error.to_string())
    }
}

impl Serialize for CommandError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

/// An amount rounded to the currency's minor unit, as a decimal string in major
/// units (`"12.99"`), so no precision is lost on the way to the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct MoneyView {
    pub amount: String,
    pub currency: String,
}

impl From<Money> for MoneyView {
    fn from(money: Money) -> Self {
        MoneyView {
            amount: money.to_major().to_string(),
            currency: money.currency.code().to_owned(),
        }
    }
}

fn rounded(amount: Fractional) -> Result<MoneyView, CommandError> {
    Ok(amount.round()?.into())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Days,
    Weeks,
    Months,
    Years,
}

/// How a subscription bills, as shown in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "unit", rename_all = "snake_case")]
pub enum Billing {
    Days { every: u16 },
    Weeks { every: u16 },
    Months { every: u16, day: u8 },
    Years { every: u16, month: u8, day: u8 },
}

impl From<Rule> for Billing {
    fn from(rule: Rule) -> Self {
        match rule {
            Rule::Days { every } => Billing::Days { every: every.get() },
            Rule::Weeks { every } => Billing::Weeks { every: every.get() },
            Rule::Months { every, day } => Billing::Months {
                every: every.get(),
                day: day.get(),
            },
            Rule::Years { every, month, day } => Billing::Years {
                every: every.get(),
                month: month.get(),
                day: day.get(),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum StatusView {
    Active,
    Paused,
    Cancelled,
    /// Starts after today.
    NotStarted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionView {
    pub id: String,
    pub name: String,
    pub status: StatusView,
    /// Price per payment under the current plan (or the first plan, if it hasn't
    /// started yet).
    pub price: MoneyView,
    pub billing: Billing,
    /// Monthly and annual equivalents; absent unless active today.
    pub monthly: Option<MoneyView>,
    pub annual: Option<MoneyView>,
    /// ISO date of the next payment, if one is still due.
    pub next_payment: Option<String>,
    pub start: String,
}

/// Everything the main screen shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Overview {
    pub today: String,
    pub subscriptions: Vec<SubscriptionView>,
    /// Current recurring commitment, one entry per currency.
    pub monthly_total: Vec<MoneyView>,
    pub annual_total: Vec<MoneyView>,
}

/// The "add subscription" form.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NewSubscription {
    pub name: String,
    /// Price per payment as typed, in major units (`"12.99"` or `"12,99"`).
    pub amount: String,
    pub currency: String,
    pub every: u16,
    pub unit: Unit,
    /// ISO date of the next (or first) payment. Monthly and yearly billing repeat
    /// on its day of the month.
    pub next_payment: String,
    /// ISO date the subscription started, if known. Defaults to the earlier of
    /// today and the next payment.
    pub since: Option<String>,
}

/// How to import a backup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ImportMode {
    /// Add what's new in the backup and keep this device's changes.
    Merge,
    /// Make this device match the backup.
    Replace,
}

impl From<ImportMode> for history::ImportMode {
    fn from(mode: ImportMode) -> Self {
        match mode {
            ImportMode::Merge => history::ImportMode::Merge,
            ImportMode::Replace => history::ImportMode::Replace,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum KeptSide {
    Device,
    Backup,
}

/// A subscription that both this device and the backup changed separately.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ConflictView {
    pub name: String,
    pub kept: KeptSide,
    /// The kept version is a deletion.
    pub kept_deleted: bool,
}

/// What an import does (or did), in numbers of subscriptions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImportSummary {
    pub added: u32,
    pub updated: u32,
    pub removed: u32,
    pub conflicts: Vec<ConflictView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Imported {
    pub summary: ImportSummary,
    pub overview: Overview,
}

pub fn import_summary(plan: &ImportPlan) -> ImportSummary {
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    ImportSummary {
        added: count(plan.added()),
        updated: count(plan.updated()),
        removed: count(plan.removed()),
        conflicts: plan
            .conflicts()
            .map(|c| {
                let (kept, other) = match c.kept {
                    Side::Local => (&c.local, &c.incoming),
                    Side::Incoming => (&c.incoming, &c.local),
                };
                ConflictView {
                    // A conflict has at least one side that isn't a deletion.
                    name: kept
                        .as_ref()
                        .or(other.as_ref())
                        .map(|s| s.name().to_owned())
                        .unwrap_or_default(),
                    kept: match c.kept {
                        Side::Local => KeptSide::Device,
                        Side::Incoming => KeptSide::Backup,
                    },
                    kept_deleted: kept.is_none(),
                }
            })
            .collect(),
    }
}

fn parse_date(text: &str, field: &str) -> Result<Date, CommandError> {
    text.parse()
        .map_err(|_| CommandError(format!("{field} must be a date like 2026-01-31")))
}

/// Builds a subscription from the form, validating every field.
pub fn new_subscription(form: &NewSubscription, today: Date) -> Result<Subscription, CommandError> {
    let currency: Currency = form.currency.trim().to_uppercase().parse()?;
    let amount = Money::parse_major(&form.amount, currency)?;
    let next = parse_date(&form.next_payment, "Next payment")?;
    let every = NonZeroU16::new(form.every)
        .ok_or_else(|| CommandError("Billing interval must be at least 1".into()))?;
    let day = DayOfMonth::new(next.day() as u8)?;
    let rule = match form.unit {
        Unit::Days => Rule::Days { every },
        Unit::Weeks => Rule::Weeks { every },
        Unit::Months => Rule::Months { every, day },
        Unit::Years => Rule::Years {
            every,
            month: Month::new(next.month() as u8)?,
            day,
        },
    };
    let start = match form.since.as_deref().map(str::trim) {
        Some(since) if !since.is_empty() => parse_date(since, "Start date")?,
        _ => today.min(next),
    };
    let plan = Plan {
        schedule: Schedule::new(rule, next)?,
        amount: amount.minor,
    };
    Ok(Subscription::new(
        SubscriptionId::generate(),
        &form.name,
        currency,
        start,
        plan,
    )?)
}

fn subscription_view(s: &Subscription, today: Date) -> Result<SubscriptionView, CommandError> {
    let plan = s.plan_on(today).unwrap_or(&s.plans()[0].value);
    let status = match s.status_on(today) {
        None => StatusView::NotStarted,
        Some(Status::Active) => StatusView::Active,
        Some(Status::Paused) => StatusView::Paused,
        Some(Status::Cancelled) => StatusView::Cancelled,
    };
    let commitment = commitment_on(s, today).transpose()?;
    Ok(SubscriptionView {
        id: s.id().0.to_string(),
        name: s.name().to_owned(),
        status,
        price: Money::new(plan.amount, s.currency()).into(),
        billing: plan.schedule.rule().into(),
        monthly: commitment.map(|c| rounded(c.monthly)).transpose()?,
        annual: commitment.map(|c| rounded(c.annual)).transpose()?,
        next_payment: s.next_payment(today).map(|p| p.date.to_string()),
        start: s.start().to_string(),
    })
}

pub fn overview_of(subscriptions: &[Subscription], today: Date) -> Result<Overview, CommandError> {
    let totals = total_commitment_on(subscriptions, today)?;
    Ok(Overview {
        today: today.to_string(),
        subscriptions: subscriptions
            .iter()
            .map(|s| subscription_view(s, today))
            .collect::<Result<_, _>>()?,
        monthly_total: totals
            .monthly
            .iter()
            .map(rounded)
            .collect::<Result<_, _>>()?,
        annual_total: totals
            .annual
            .iter()
            .map(rounded)
            .collect::<Result<_, _>>()?,
    })
}

/// Today in the device's time zone.
fn today() -> Date {
    jiff::Zoned::now().date()
}

#[tauri::command]
pub async fn overview(db: State<'_, Db>) -> Result<Overview, CommandError> {
    overview_of(&db.subscriptions().await?, today())
}

#[tauri::command]
pub async fn add_subscription(
    db: State<'_, Db>,
    form: NewSubscription,
) -> Result<Overview, CommandError> {
    let today = today();
    db.save(&new_subscription(&form, today)?).await?;
    overview_of(&db.subscriptions().await?, today)
}

#[tauri::command]
pub async fn delete_subscription(db: State<'_, Db>, id: String) -> Result<Overview, CommandError> {
    let id = SubscriptionId(Uuid::parse_str(&id)?);
    db.delete(id).await?;
    overview_of(&db.subscriptions().await?, today())
}

#[tauri::command]
pub async fn export_backup(db: State<'_, Db>, path: String) -> Result<(), CommandError> {
    let json = db.export().await?.to_json();
    write_replacing(Path::new(&path), json.as_bytes())
        .map_err(|e| CommandError(format!("Couldn't save the backup: {e}")))
}

/// Writes a temporary file next to `path` and then renames it, so a failed
/// write never leaves a half-written file in place of an older backup.
fn write_replacing(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let mut partial = path.as_os_str().to_owned();
    partial.push(".partial");
    std::fs::write(&partial, contents)?;
    std::fs::rename(&partial, path)
}

fn read_backup(path: &str) -> Result<Backup, CommandError> {
    let json = std::fs::read_to_string(path)
        .map_err(|e| CommandError(format!("Couldn't read the file: {e}")))?;
    Ok(Backup::from_json(&json)?)
}

/// What importing the backup at `path` would do, without changing anything.
#[tauri::command]
pub async fn preview_import(
    db: State<'_, Db>,
    path: String,
    mode: ImportMode,
) -> Result<ImportSummary, CommandError> {
    let plan = db.preview_import(&read_backup(&path)?, mode.into()).await?;
    Ok(import_summary(&plan))
}

#[tauri::command]
pub async fn import_backup(
    db: State<'_, Db>,
    path: String,
    mode: ImportMode,
) -> Result<Imported, CommandError> {
    let plan = db.import(&read_backup(&path)?, mode.into()).await?;
    Ok(Imported {
        summary: import_summary(&plan),
        overview: overview_of(&db.subscriptions().await?, today())?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn form(amount: &str, every: u16, unit: Unit, next: &str) -> NewSubscription {
        NewSubscription {
            name: "Music".into(),
            amount: amount.into(),
            currency: "eur".into(),
            every,
            unit,
            next_payment: next.into(),
            since: None,
        }
    }

    #[test]
    fn monthly_billing_repeats_on_the_next_payments_day() {
        let today = date(2026, 10, 27);
        let s = new_subscription(&form("9,99", 1, Unit::Months, "2026-10-31"), today).unwrap();
        assert_eq!(s.currency(), Currency::EUR);
        assert_eq!(s.start(), today);
        let view = subscription_view(&s, today).unwrap();
        assert_eq!(
            view.price,
            MoneyView {
                amount: "9.99".into(),
                currency: "EUR".into()
            }
        );
        assert_eq!(view.billing, Billing::Months { every: 1, day: 31 });
        assert_eq!(view.next_payment.as_deref(), Some("2026-10-31"));
        assert_eq!(view.monthly.unwrap().amount, "9.99");
        assert_eq!(view.annual.unwrap().amount, "119.88");
        assert_eq!(
            s.next_payment(date(2026, 11, 1)).unwrap().date,
            date(2026, 11, 30)
        );
    }

    #[test]
    fn yearly_billing_shows_its_monthly_equivalent() {
        let today = date(2026, 10, 27);
        let s = new_subscription(&form("120", 1, Unit::Years, "2027-03-15"), today).unwrap();
        let view = subscription_view(&s, today).unwrap();
        assert_eq!(
            view.billing,
            Billing::Years {
                every: 1,
                month: 3,
                day: 15
            }
        );
        assert_eq!(view.monthly.unwrap().amount, "10.00");
        assert_eq!(view.next_payment.as_deref(), Some("2027-03-15"));
    }

    #[test]
    fn an_explicit_start_date_is_kept() {
        let mut f = form("5", 2, Unit::Weeks, "2026-10-30");
        f.since = Some("2024-01-05".into());
        let s = new_subscription(&f, date(2026, 10, 27)).unwrap();
        assert_eq!(s.start(), date(2024, 1, 5));
    }

    #[test]
    fn a_future_start_is_not_counted_yet() {
        let mut f = form("5", 1, Unit::Months, "2026-12-01");
        f.since = Some("2026-12-01".into());
        let today = date(2026, 10, 27);
        let s = new_subscription(&f, today).unwrap();
        let view = subscription_view(&s, today).unwrap();
        assert_eq!(view.status, StatusView::NotStarted);
        assert_eq!(view.monthly, None);
        assert_eq!(view.next_payment.as_deref(), Some("2026-12-01"));
    }

    #[test]
    fn invalid_forms_are_rejected_with_a_message() {
        let today = date(2026, 10, 27);
        let error = |f: NewSubscription| new_subscription(&f, today).unwrap_err().0;
        assert!(error(form("9.999", 1, Unit::Months, "2026-10-31")).contains("decimal places"));
        assert!(error(form("abc", 1, Unit::Months, "2026-10-31")).contains("not an amount"));
        assert!(error(form("9", 0, Unit::Months, "2026-10-31")).contains("at least 1"));
        assert!(error(form("9", 1, Unit::Months, "31/10/2026")).contains("Next payment"));
        let mut f = form("9", 1, Unit::Months, "2026-10-31");
        f.currency = "XYZ".into();
        assert!(error(f).contains("unknown currency"));
        let mut f = form("9", 1, Unit::Months, "2026-10-31");
        f.name = " ".into();
        assert!(error(f).contains("name"));
    }

    #[test]
    fn overview_totals_commitment_per_currency() {
        let today = date(2026, 10, 27);
        let mut usd = form("5", 1, Unit::Months, "2026-11-01");
        usd.currency = "USD".into();
        let subs = [
            new_subscription(&form("10", 1, Unit::Months, "2026-11-01"), today).unwrap(),
            new_subscription(&form("120", 1, Unit::Years, "2027-01-01"), today).unwrap(),
            new_subscription(&usd, today).unwrap(),
        ];
        let overview = overview_of(&subs, today).unwrap();
        assert_eq!(
            overview.monthly_total,
            [
                MoneyView {
                    amount: "20.00".into(),
                    currency: "EUR".into()
                },
                MoneyView {
                    amount: "5.00".into(),
                    currency: "USD".into()
                },
            ]
        );
        assert_eq!(overview.annual_total[0].amount, "240.00");
        assert_eq!(overview.today, "2026-10-27");
    }

    #[tokio::test]
    async fn import_summary_names_conflicts_and_the_kept_side() {
        let today = date(2026, 10, 27);
        let laptop = Db::open_in_memory().await.unwrap();
        let phone = Db::open_in_memory().await.unwrap();
        let s = new_subscription(&form("10", 1, Unit::Months, "2026-11-01"), today).unwrap();
        laptop.save(&s).await.unwrap();
        let shared = laptop.export().await.unwrap();
        phone
            .import(&shared, history::ImportMode::Merge)
            .await
            .unwrap();
        // The laptop deletes it, then the phone renames it.
        laptop.delete(s.id()).await.unwrap();
        phone.save(&s.renamed("Renamed").unwrap()).await.unwrap();

        let plan = laptop
            .preview_import(&phone.export().await.unwrap(), history::ImportMode::Merge)
            .await
            .unwrap();
        assert_eq!(
            import_summary(&plan),
            ImportSummary {
                added: 1,
                updated: 0,
                removed: 0,
                conflicts: vec![ConflictView {
                    name: "Renamed".into(),
                    kept: KeptSide::Backup,
                    kept_deleted: false,
                }],
            }
        );
    }

    #[test]
    fn saving_a_backup_replaces_the_file_whole() {
        let dir = std::env::temp_dir().join(format!("epicycle-test-{}", Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("backup.json");
        std::fs::write(&path, "older and longer contents").unwrap();
        write_replacing(&path, b"new").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
