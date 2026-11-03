//! Tauri commands: the boundary between the UI and the Rust side.
//!
//! Commands take and return the plain data-transfer types defined here rather than
//! domain types, so the domain crate stays free of UI concerns. TypeScript
//! versions of these types are generated into `ui/src/lib/bindings/` by ts-rs
//! whenever `cargo test` runs.

use std::num::NonZeroU16;
use std::path::Path;

use epicycle_core::backup::Backup;
use epicycle_core::backup::RateChanges;
use epicycle_core::commitment::{commitment_on, total_commitment_on};
use epicycle_core::history::{self, HistoryReplacement, ImportPlan, Side};
use epicycle_core::money::{Currency, Fractional, Money};
use epicycle_core::recurrence::{DayOfMonth, Month, Rule, Schedule};
use epicycle_core::subscription::{Plan, Status, Subscription, SubscriptionId};
use jiff::civil::Date;
use serde::{Deserialize, Serialize, Serializer};
use tauri::State;
use ts_rs::TS;
use uuid::Uuid;

use crate::db::Db;
use crate::history_view::{DeletedView, RevisionView, history_view};
use epicycle_core::fx::{self, Freshness, Rate, RateSource, RateTable};
use epicycle_core::history::{History, RevisionId};
use rust_decimal::Decimal;

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
    pub notes: String,
    /// The next plan change after today, e.g. the end of a free trial.
    pub next_change: Option<PlanView>,
}

/// A plan in a subscription's history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PlanView {
    /// ISO date it takes effect.
    pub from: String,
    pub price: MoneyView,
    pub billing: Billing,
    /// ISO date of one payment, which fixes the schedule's phase.
    pub anchor: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StatusChangeView {
    pub from: String,
    pub status: StatusChoice,
}

/// Everything the subscription screen shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SubscriptionDetail {
    pub today: String,
    pub subscription: SubscriptionView,
    /// Oldest first.
    pub plans: Vec<PlanView>,
    /// Oldest first. Before the first change the subscription is active.
    pub status_changes: Vec<StatusChangeView>,
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

/// A currency the user can choose for a new subscription.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct CurrencyOption {
    /// ISO 4217 code, e.g. `"RON"`.
    pub code: String,
    /// English name, e.g. `"Romanian leu"`.
    pub name: String,
}

/// How to import a backup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ImportMode {
    /// Add what's new in the backup and keep this device's changes.
    Merge,
    /// Make this device match the backup, keeping what it replaces in the
    /// history.
    Replace,
    /// Make this device's history exactly the backup's, permanently deleting
    /// changes the backup doesn't have.
    ReplaceHistory,
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
    /// Changes in this device's history that would be permanently deleted;
    /// only replacing the history deletes any.
    pub lost_revisions: u32,
    /// Exchange rates entered by hand that are added or changed.
    pub rates_changed: u32,
    /// Exchange rates entered by hand on this device that are deleted; only
    /// replacing the history deletes any.
    pub rates_removed: u32,
}

/// Adds what happens to manual exchange rates to an import summary.
pub fn with_rates(summary: ImportSummary, rates: &RateChanges) -> ImportSummary {
    ImportSummary {
        rates_changed: count(rates.changed),
        rates_removed: count(rates.removed),
        ..summary
    }
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

pub fn replacement_summary(plan: &HistoryReplacement) -> ImportSummary {
    ImportSummary {
        added: count(plan.added),
        updated: count(plan.updated),
        removed: count(plan.removed),
        conflicts: Vec::new(),
        lost_revisions: count(plan.lost_revisions),
        rates_changed: 0,
        rates_removed: 0,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Imported {
    pub summary: ImportSummary,
    pub overview: Overview,
}

pub fn import_summary(plan: &ImportPlan) -> ImportSummary {
    ImportSummary {
        lost_revisions: 0,
        rates_changed: 0,
        rates_removed: 0,
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
    let currency = currency_in_use(&form.currency)?;
    let next = parse_date(&form.next_payment, "Next payment")?;
    let plan = plan_of(&form.amount, currency, form.every, form.unit, next)?;
    let start = match form.since.as_deref().map(str::trim) {
        Some(since) if !since.is_empty() => parse_date(since, "Start date")?,
        _ => today.min(next),
    };
    let s = Subscription::new(
        SubscriptionId::generate(),
        &form.name,
        currency,
        start,
        plan,
    )?;
    // The next payment is the next one: nothing is due between now and then
    // (a free trial, or time already paid for). Earlier payments, back to the
    // start, are estimates of what was paid.
    let from = today.max(start);
    Ok(if next > from {
        s.with_free_period(from, next)?
    } else {
        s
    })
}

/// A plan billing `amount` (as typed) every `every` units, with a payment on
/// `next`. Monthly and yearly billing repeat on `next`'s day.
fn plan_of(
    amount: &str,
    currency: Currency,
    every: u16,
    unit: Unit,
    next: Date,
) -> Result<Plan, CommandError> {
    let amount = Money::parse_major(amount, currency)?;
    let every = NonZeroU16::new(every)
        .ok_or_else(|| CommandError("Billing interval must be at least 1".into()))?;
    let day = DayOfMonth::new(next.day() as u8)?;
    let rule = match unit {
        Unit::Days => Rule::Days { every },
        Unit::Weeks => Rule::Weeks { every },
        Unit::Months => Rule::Months { every, day },
        Unit::Years => Rule::Years {
            every,
            month: Month::new(next.month() as u8)?,
            day,
        },
    };
    Ok(Plan {
        schedule: Schedule::new(rule, next)?,
        amount: amount.minor,
    })
}

/// A status the user can set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum StatusChoice {
    Active,
    Paused,
    Cancelled,
}

impl From<StatusChoice> for Status {
    fn from(choice: StatusChoice) -> Self {
        match choice {
            StatusChoice::Active => Status::Active,
            StatusChoice::Paused => Status::Paused,
            StatusChoice::Cancelled => Status::Cancelled,
        }
    }
}

impl From<Status> for StatusChoice {
    fn from(status: Status) -> Self {
        match status {
            Status::Active => StatusChoice::Active,
            Status::Paused => StatusChoice::Paused,
            Status::Cancelled => StatusChoice::Cancelled,
        }
    }
}

/// One change to an existing subscription.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum Edit {
    /// Name and notes, which have no effective date.
    Details { name: String, notes: String },
    /// A new price or billing schedule from a date on. A plan on the same date
    /// as an existing one replaces it, which is how mistakes are corrected.
    Plan {
        from: String,
        amount: String,
        every: u16,
        unit: Unit,
        /// The first payment under the new plan; monthly and yearly billing
        /// repeat on its day.
        next_payment: String,
    },
    /// Pause, cancel or resume from a date on.
    Status { from: String, status: StatusChoice },
}

/// Applies an edit, validating it. The result is a new value; nothing is saved.
pub fn apply_edit(s: &Subscription, edit: &Edit) -> Result<Subscription, CommandError> {
    match edit {
        Edit::Details { name, notes } => Ok(s.renamed(name)?.with_notes(notes)),
        Edit::Plan {
            from,
            amount,
            every,
            unit,
            next_payment,
        } => {
            let from = parse_date(from, "Effective date")?;
            let next = parse_date(next_payment, "First payment")?;
            if next < from {
                return Err(CommandError(
                    "The first payment can't be before the change takes effect".into(),
                ));
            }
            let changed = s.with_plan(from, plan_of(amount, s.currency(), *every, *unit, next)?)?;
            // Nothing is paid between the change and its first payment.
            Ok(if next > from {
                changed.with_free_period(from, next)?
            } else {
                changed
            })
        }
        Edit::Status { from, status } => {
            let from = parse_date(from, "Effective date")?;
            Ok(s.with_status(from, (*status).into())?)
        }
    }
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
        notes: s.notes().to_owned(),
        next_change: s
            .plans()
            .iter()
            .find(|p| p.from > today)
            .map(|p| plan_view(s.currency(), p)),
    })
}

fn plan_view(currency: Currency, p: &epicycle_core::subscription::Dated<Plan>) -> PlanView {
    PlanView {
        from: p.from.to_string(),
        price: Money::new(p.value.amount, currency).into(),
        billing: p.value.schedule.rule().into(),
        anchor: p.value.schedule.anchor().to_string(),
    }
}

pub fn detail_of(s: &Subscription, today: Date) -> Result<SubscriptionDetail, CommandError> {
    Ok(SubscriptionDetail {
        today: today.to_string(),
        subscription: subscription_view(s, today)?,
        plans: s
            .plans()
            .iter()
            .map(|p| plan_view(s.currency(), p))
            .collect(),
        status_changes: s
            .status_changes()
            .iter()
            .map(|c| StatusChangeView {
                from: c.from.to_string(),
                status: c.value.into(),
            })
            .collect(),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum RateSourceView {
    Ecb,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum FreshnessView {
    Fresh,
    Stale,
    Manual,
}

/// An exchange rate: units of `currency` per 1 EUR on `date`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RateView {
    pub currency: String,
    pub date: String,
    /// Exact decimal, e.g. `"5.3405"`.
    pub per_eur: String,
    pub source: RateSourceView,
    pub freshness: FreshnessView,
}

fn rate_view(rate: &Rate, today: Date) -> RateView {
    RateView {
        currency: rate.currency().code().to_owned(),
        date: rate.date().to_string(),
        per_eur: rate.per_eur().normalize().to_string(),
        source: match rate.source() {
            RateSource::Ecb { .. } => RateSourceView::Ecb,
            RateSource::Manual { .. } => RateSourceView::Manual,
        },
        freshness: match rate.freshness(today) {
            Freshness::Fresh => FreshnessView::Fresh,
            Freshness::Stale => FreshnessView::Stale,
            Freshness::Manual => FreshnessView::Manual,
        },
    }
}

/// The current recurring commitment in one currency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConvertedTotal {
    pub currency: String,
    /// The reporting currency, used unless another is chosen.
    pub default_currency: String,
    /// Absent when a rate is missing.
    pub monthly: Option<MoneyView>,
    pub annual: Option<MoneyView>,
    /// The rates the totals rely on, so their age can be shown.
    pub rates: Vec<RateView>,
    /// Currencies with no rate at all; the totals can't be computed until one
    /// is entered.
    pub missing: Vec<String>,
}

/// Converts the current commitment to `to` with the rates for `today`.
pub fn converted_total_of(
    subscriptions: &[Subscription],
    rates: &RateTable,
    to: Currency,
    default: Currency,
    today: Date,
) -> Result<ConvertedTotal, CommandError> {
    let totals = total_commitment_on(subscriptions, today)?;
    let mut currencies: Vec<Currency> = totals.monthly.iter().map(|f| f.currency).collect();
    currencies.push(to);
    currencies.sort();
    currencies.dedup();
    let missing: Vec<String> = currencies
        .iter()
        .filter(|&&c| c != Currency::EUR && rates.rate_for(c, today).is_none())
        .map(|c| c.code().to_owned())
        .collect();
    let mut view = ConvertedTotal {
        currency: to.code().to_owned(),
        default_currency: default.code().to_owned(),
        monthly: None,
        annual: None,
        rates: Vec::new(),
        missing,
    };
    if !view.missing.is_empty() {
        return Ok(view);
    }
    let monthly = fx::convert_total(rates, &totals.monthly, to, today)?;
    let annual = fx::convert_total(rates, &totals.annual, to, today)?;
    view.monthly = Some(rounded(monthly.amount)?);
    view.annual = Some(rounded(annual.amount)?);
    // Both totals use the same rates; list each once.
    view.rates = monthly.rates.iter().map(|r| rate_view(r, today)).collect();
    Ok(view)
}

/// Every currency's rate for today (the newest on or before it), and every
/// manual rate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RatesOverview {
    pub today: String,
    pub reporting_currency: String,
    /// One per currency, sorted by code.
    pub current: Vec<RateView>,
    /// Newest first.
    pub manual: Vec<RateView>,
}

pub fn rates_overview(rates: &RateTable, reporting: Currency, today: Date) -> RatesOverview {
    let mut currencies: Vec<Currency> = rates.iter().map(Rate::currency).collect();
    currencies.dedup();
    let mut manual: Vec<RateView> = rates
        .iter()
        .filter(|r| matches!(r.source(), RateSource::Manual { .. }))
        .map(|r| rate_view(r, today))
        .collect();
    manual.sort_by(|a, b| b.date.cmp(&a.date).then(a.currency.cmp(&b.currency)));
    RatesOverview {
        today: today.to_string(),
        reporting_currency: reporting.code().to_owned(),
        current: currencies
            .into_iter()
            .filter_map(|c| rates.rate_for(c, today))
            .map(|r| rate_view(r, today))
            .collect(),
        manual,
    }
}

/// A currency the user typed, which must be one in use.
fn currency_in_use(code: &str) -> Result<Currency, CommandError> {
    let currency: Currency = code.trim().to_uppercase().parse()?;
    if !currency.in_use() {
        return Err(CommandError(format!(
            "{currency} is no longer used for payments; choose another currency"
        )));
    }
    Ok(currency)
}

/// A rate typed by the user as "1 EUR = `per_eur` `currency`".
pub fn manual_rate(
    currency: &str,
    date: &str,
    per_eur: &str,
    now: jiff::Timestamp,
) -> Result<Rate, CommandError> {
    let currency = currency_in_use(currency)?;
    let date = parse_date(date, "Date")?;
    let per_eur: Decimal = per_eur
        .trim()
        .replace(',', ".")
        .parse()
        .map_err(|_| CommandError(format!("{per_eur:?} is not a number")))?;
    Rate::new(
        currency,
        date,
        per_eur,
        RateSource::Manual { entered_at: now },
    )
    .map_err(|e| match e {
        fx::FxError::BaseCurrency => CommandError("EUR is the base: 1 EUR = 1 EUR".into()),
        other => other.into(),
    })
}

/// Today in the device's time zone.
pub fn today() -> Date {
    jiff::Zoned::now().date()
}

#[tauri::command]
pub async fn overview(db: State<'_, Db>) -> Result<Overview, CommandError> {
    overview_of(&db.subscriptions().await?, today())
}

/// The current commitment converted to `currency`, or to the reporting
/// currency if none is given.
#[tauri::command]
pub async fn converted_total(
    db: State<'_, Db>,
    currency: Option<String>,
) -> Result<ConvertedTotal, CommandError> {
    let default = db.reporting_currency().await?;
    let to = match currency.as_deref() {
        Some(code) => currency_in_use(code)?,
        None => default,
    };
    converted_total_of(
        &db.subscriptions().await?,
        &db.rates().await?,
        to,
        default,
        today(),
    )
}

/// Fetches new ECB rates if the cached ones are out of date. Returns how many
/// were stored; fails quietly-worded when offline.
#[tauri::command]
pub async fn refresh_rates(db: State<'_, Db>) -> Result<u32, CommandError> {
    let stored = crate::rates::refresh(&db, today()).await?;
    Ok(u32::try_from(stored).unwrap_or(u32::MAX))
}

#[tauri::command]
pub async fn exchange_rates(db: State<'_, Db>) -> Result<RatesOverview, CommandError> {
    Ok(rates_overview(
        &db.rates().await?,
        db.reporting_currency().await?,
        today(),
    ))
}

#[tauri::command]
pub async fn add_manual_rate(
    db: State<'_, Db>,
    currency: String,
    date: String,
    per_eur: String,
) -> Result<RatesOverview, CommandError> {
    let rate = manual_rate(&currency, &date, &per_eur, jiff::Timestamp::now())?;
    db.save_rates(&[rate]).await?;
    exchange_rates(db).await
}

#[tauri::command]
pub async fn delete_manual_rate(
    db: State<'_, Db>,
    currency: String,
    date: String,
) -> Result<RatesOverview, CommandError> {
    db.delete_manual_rate(currency.parse()?, parse_date(&date, "Date")?)
        .await?;
    exchange_rates(db).await
}

/// Favourite currencies, in the user's order.
#[tauri::command]
pub async fn favorite_currencies(db: State<'_, Db>) -> Result<Vec<String>, CommandError> {
    Ok(db
        .favorites()
        .await?
        .iter()
        .map(|c| c.code().to_owned())
        .collect())
}

#[tauri::command]
pub async fn set_favorite_currencies(
    db: State<'_, Db>,
    currencies: Vec<String>,
) -> Result<Vec<String>, CommandError> {
    let favorites = currencies
        .iter()
        .map(|c| currency_in_use(c))
        .collect::<Result<Vec<_>, _>>()?;
    db.set_favorites(&favorites).await?;
    favorite_currencies(db).await
}

#[tauri::command]
pub async fn set_reporting_currency(
    db: State<'_, Db>,
    currency: String,
) -> Result<(), CommandError> {
    db.set_reporting_currency(currency_in_use(&currency)?)
        .await?;
    Ok(())
}

/// The currencies offered for new subscriptions, sorted by code.
#[tauri::command]
pub fn currencies() -> Vec<CurrencyOption> {
    Currency::selectable()
        .into_iter()
        .map(|c| CurrencyOption {
            code: c.code().to_owned(),
            name: c.name().to_owned(),
        })
        .collect()
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

async fn find(db: &Db, id: &str) -> Result<Subscription, CommandError> {
    let id = SubscriptionId(Uuid::parse_str(id)?);
    db.subscriptions()
        .await?
        .into_iter()
        .find(|s| s.id() == id)
        .ok_or_else(|| CommandError("This subscription no longer exists".into()))
}

#[tauri::command]
pub async fn subscription(
    db: State<'_, Db>,
    id: String,
) -> Result<SubscriptionDetail, CommandError> {
    detail_of(&find(&db, &id).await?, today())
}

#[tauri::command]
pub async fn edit_subscription(
    db: State<'_, Db>,
    id: String,
    edit: Edit,
) -> Result<SubscriptionDetail, CommandError> {
    let edited = apply_edit(&find(&db, &id).await?, &edit)?;
    db.save(&edited).await?;
    detail_of(&edited, today())
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
    let backup = read_backup(&path)?;
    Ok(match mode {
        ImportMode::Merge | ImportMode::Replace => {
            let (plan, rates) = db.preview_import(&backup, history_mode(mode)).await?;
            with_rates(import_summary(&plan), &rates)
        }
        ImportMode::ReplaceHistory => {
            let (plan, rates) = db.preview_history_replacement(&backup).await?;
            with_rates(replacement_summary(&plan), &rates)
        }
    })
}

/// The history merge behind Merge and Replace.
fn history_mode(mode: ImportMode) -> history::ImportMode {
    match mode {
        ImportMode::Merge => history::ImportMode::Merge,
        ImportMode::Replace | ImportMode::ReplaceHistory => history::ImportMode::Replace,
    }
}

#[tauri::command]
pub async fn import_backup(
    db: State<'_, Db>,
    path: String,
    mode: ImportMode,
) -> Result<Imported, CommandError> {
    let backup = read_backup(&path)?;
    let summary = match mode {
        ImportMode::Merge | ImportMode::Replace => {
            let (plan, rates) = db.import(&backup, history_mode(mode)).await?;
            with_rates(import_summary(&plan), &rates)
        }
        ImportMode::ReplaceHistory => {
            let (plan, rates) = db.replace_history(&backup).await?;
            with_rates(replacement_summary(&plan), &rates)
        }
    };
    Ok(Imported {
        summary,
        overview: overview_of(&db.subscriptions().await?, today())?,
    })
}

fn parse_id<T>(text: &str, wrap: impl Fn(Uuid) -> T) -> Result<T, CommandError> {
    Ok(wrap(Uuid::parse_str(text)?))
}

/// A subscription's change history, newest first. Works for deleted
/// subscriptions too.
#[tauri::command]
pub async fn subscription_history(
    db: State<'_, Db>,
    id: String,
) -> Result<Vec<RevisionView>, CommandError> {
    let id = parse_id(&id, SubscriptionId)?;
    let history = History::new(db.history(id).await?)?;
    Ok(history_view(&history, db.device_id()))
}

/// Makes an earlier version of a subscription current again.
#[tauri::command]
pub async fn restore_revision(
    db: State<'_, Db>,
    id: String,
    revision: String,
) -> Result<SubscriptionDetail, CommandError> {
    let restored = db
        .restore(
            parse_id(&id, SubscriptionId)?,
            parse_id(&revision, RevisionId)?,
        )
        .await?;
    detail_of(&restored, today())
}

/// Deleted subscriptions, most recently deleted first.
#[tauri::command]
pub async fn deleted_subscriptions(db: State<'_, Db>) -> Result<Vec<DeletedView>, CommandError> {
    let mut views = Vec::new();
    for (deletion, version) in db.deleted().await? {
        let Some(s) = &version.subscription else {
            continue;
        };
        let plan = s.plans().last().expect("a subscription has a plan").value;
        views.push(DeletedView {
            id: s.id().to_string(),
            name: s.name().to_owned(),
            price: Money::new(plan.amount, s.currency()).into(),
            billing: plan.schedule.rule().into(),
            deleted_at: deletion.created_at.to_string(),
            revision_id: version.id.to_string(),
        });
    }
    Ok(views)
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

    fn music() -> Subscription {
        new_subscription(
            &form("10", 1, Unit::Months, "2026-10-15"),
            date(2026, 10, 1),
        )
        .unwrap()
    }

    #[test]
    fn edits_name_and_notes() {
        let s = apply_edit(
            &music(),
            &Edit::Details {
                name: " Music Family ".into(),
                notes: "Shared with Ana\n".into(),
            },
        )
        .unwrap();
        assert_eq!(s.name(), "Music Family");
        assert_eq!(s.notes(), "Shared with Ana");
        let blank = Edit::Details {
            name: "".into(),
            notes: "".into(),
        };
        assert!(apply_edit(&s, &blank).unwrap_err().0.contains("name"));
    }

    #[test]
    fn a_price_change_applies_from_its_date() {
        let edit = Edit::Plan {
            from: "2026-12-01".into(),
            amount: "12.50".into(),
            every: 1,
            unit: Unit::Months,
            next_payment: "2026-12-15".into(),
        };
        let s = apply_edit(&music(), &edit).unwrap();
        let detail = detail_of(&s, date(2026, 12, 20)).unwrap();
        assert_eq!(detail.plans.len(), 2);
        assert_eq!(detail.plans[1].from, "2026-12-01");
        assert_eq!(detail.subscription.price.amount, "12.50");
        // Before the change, the old price still applies.
        let before = detail_of(&s, date(2026, 11, 20)).unwrap();
        assert_eq!(before.subscription.price.amount, "10.00");
    }

    #[test]
    fn moving_the_next_payment_months_ahead_skips_the_months_between() {
        // Reported: the next payment moved to 2027-03-12 still showed one on
        // the 12th of this month.
        let edit = Edit::Plan {
            from: "2026-11-03".into(),
            amount: "20".into(),
            every: 1,
            unit: Unit::Months,
            next_payment: "2027-03-12".into(),
        };
        let s = apply_edit(&music(), &edit).unwrap();
        let view = subscription_view(&s, date(2026, 11, 3)).unwrap();
        assert_eq!(view.next_payment.as_deref(), Some("2027-03-12"));
    }

    #[test]
    fn a_free_trial_is_free_until_the_first_payment() {
        let today = date(2026, 11, 3);
        let s = new_subscription(&form("20", 1, Unit::Months, "2026-12-03"), today).unwrap();
        let view = subscription_view(&s, today).unwrap();
        assert_eq!(view.next_payment.as_deref(), Some("2026-12-03"));
        assert_eq!(view.price.amount, "0.00");
        assert_eq!(view.monthly.unwrap().amount, "0.00");
        let next = view.next_change.unwrap();
        assert_eq!(
            (next.from.as_str(), next.price.amount.as_str()),
            ("2026-12-03", "20.00")
        );
        // A next payment that is simply the next one adds no free period.
        let plain = new_subscription(&form("20", 1, Unit::Months, "2026-11-12"), today).unwrap();
        assert_eq!(plain.plans().len(), 1);
        assert_eq!(subscription_view(&plain, today).unwrap().next_change, None);
    }

    #[test]
    fn a_new_plan_cannot_pay_before_it_starts() {
        let edit = Edit::Plan {
            from: "2026-12-01".into(),
            amount: "12".into(),
            every: 1,
            unit: Unit::Months,
            next_payment: "2026-11-30".into(),
        };
        assert!(
            apply_edit(&music(), &edit)
                .unwrap_err()
                .0
                .contains("can't be before")
        );
    }

    #[test]
    fn pausing_and_resuming() {
        let paused = apply_edit(
            &music(),
            &Edit::Status {
                from: "2026-11-01".into(),
                status: StatusChoice::Paused,
            },
        )
        .unwrap();
        let view = subscription_view(&paused, date(2026, 11, 20)).unwrap();
        assert_eq!(view.status, StatusView::Paused);
        assert_eq!(view.next_payment, None);

        let resumed = apply_edit(
            &paused,
            &Edit::Status {
                from: "2027-01-01".into(),
                status: StatusChoice::Active,
            },
        )
        .unwrap();
        let view = subscription_view(&resumed, date(2026, 11, 20)).unwrap();
        assert_eq!(view.next_payment.as_deref(), Some("2027-01-15"));
        assert_eq!(
            detail_of(&resumed, date(2026, 11, 20))
                .unwrap()
                .status_changes
                .len(),
            2
        );
    }

    #[test]
    fn a_status_change_before_the_start_is_rejected() {
        let edit = Edit::Status {
            from: "2020-01-01".into(),
            status: StatusChoice::Cancelled,
        };
        assert!(apply_edit(&music(), &edit).is_err());
    }

    fn ecb(currency: &str, day: i8, per_eur: &str) -> Rate {
        Rate::new(
            currency.parse().unwrap(),
            date(2026, 11, day),
            per_eur.parse().unwrap(),
            RateSource::Ecb {
                fetched_at: jiff::Timestamp::UNIX_EPOCH,
            },
        )
        .unwrap()
    }

    fn priced(name: &str, amount: &str, currency: &str) -> Subscription {
        let mut f = form(amount, 1, Unit::Months, "2026-11-15");
        f.name = name.into();
        f.currency = currency.into();
        new_subscription(&f, date(2026, 11, 1)).unwrap()
    }

    #[test]
    fn converts_the_commitment_to_one_currency() {
        let subs = [priced("A", "10", "EUR"), priced("B", "53.40", "RON")];
        let rates = RateTable::new([ecb("RON", 2, "5.34"), ecb("USD", 2, "1.12")]);
        let eur = Currency::EUR;
        let today = date(2026, 11, 3);

        let total = converted_total_of(&subs, &rates, eur, eur, today).unwrap();
        assert_eq!(total.monthly.unwrap().amount, "20.00");
        assert_eq!(total.annual.unwrap().amount, "240.00");
        assert_eq!(total.rates.len(), 1);
        assert_eq!(total.rates[0].freshness, FreshnessView::Fresh);

        // Any other currency, through EUR: 20 EUR = 22.40 USD.
        let usd = "USD".parse().unwrap();
        let total = converted_total_of(&subs, &rates, usd, eur, today).unwrap();
        assert_eq!(total.monthly.unwrap().amount, "22.40");
        assert_eq!(total.default_currency, "EUR");
    }

    #[test]
    fn names_missing_rates_instead_of_guessing() {
        let subs = [priced("A", "10", "GBP")];
        let rates = RateTable::new([ecb("USD", 2, "1.12")]);
        let eur = Currency::EUR;
        let chf = "CHF".parse().unwrap();
        let total = converted_total_of(&subs, &rates, chf, eur, date(2026, 11, 3)).unwrap();
        assert_eq!(total.monthly, None);
        assert_eq!(total.missing, ["CHF", "GBP"]);
    }

    #[test]
    fn marks_old_ecb_rates_as_stale() {
        let subs = [priced("A", "10", "USD")];
        let rates = RateTable::new([ecb("USD", 2, "1.12")]);
        let eur = Currency::EUR;
        let total = converted_total_of(&subs, &rates, eur, eur, date(2026, 11, 20)).unwrap();
        assert_eq!(total.rates[0].freshness, FreshnessView::Stale);
    }

    #[test]
    fn reads_manual_rates_as_typed() {
        let now = jiff::Timestamp::UNIX_EPOCH;
        let rate = manual_rate("ron", "2026-11-03", "5,1", now).unwrap();
        assert_eq!(rate.per_eur(), Decimal::new(51, 1));
        assert!(manual_rate("RON", "2026-11-03", "0", now).is_err());
        assert!(
            manual_rate("RON", "2026-11-03", "abc", now)
                .unwrap_err()
                .0
                .contains("not a number")
        );
        assert!(
            manual_rate("EUR", "2026-11-03", "1", now)
                .unwrap_err()
                .0
                .contains("base")
        );

        // A manual rate wins over the ECB's for the same day, and is listed.
        let table = RateTable::new([ecb("RON", 3, "5.34"), rate]);
        let overview = rates_overview(&table, Currency::EUR, date(2026, 11, 3));
        assert_eq!(overview.current[0].source, RateSourceView::Manual);
        assert_eq!(overview.current[0].per_eur, "5.1");
        assert_eq!(overview.manual.len(), 1);
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
        f.currency = "HRK".into();
        assert!(error(f).contains("no longer used"));
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
            import_summary(&plan.0),
            ImportSummary {
                added: 1,
                updated: 0,
                removed: 0,
                conflicts: vec![ConflictView {
                    name: "Renamed".into(),
                    kept: KeptSide::Backup,
                    kept_deleted: false,
                }],
                lost_revisions: 0,
                rates_changed: 0,
                rates_removed: 0,
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
