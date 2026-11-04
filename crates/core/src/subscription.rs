//! Subscriptions: what is billed, when, and how that changed over time.
//!
//! A subscription has a history of [`Plan`]s (billing schedule and amount), each
//! in effect from a date, and a history of [`Status`] changes. Keeping both as
//! history rather than a current value is what lets past spending be estimated
//! with the prices and schedules that applied at the time.

use jiff::civil::Date;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::money::{Currency, Money};
use crate::recurrence::{Due, Schedule};

/// Stable identifier that survives export and import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SubscriptionId(pub Uuid);

impl std::fmt::Display for SubscriptionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl SubscriptionId {
    /// A new random ID. UUIDv7 sorts by creation time, which keeps database
    /// indexes compact.
    pub fn generate() -> Self {
        SubscriptionId(Uuid::now_v7())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Active,
    Paused,
    Cancelled,
}

/// What is billed and when, for as long as this plan is in effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Plan {
    pub schedule: Schedule,
    /// Amount per payment, in minor units of the subscription's currency: the
    /// exact amount, or the expected one when `estimate` is set. Everything
    /// that adds up money uses it.
    pub amount: i64,
    /// Set when the amount isn't exact. Left out of the JSON when absent, so
    /// exact plans serialize as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimate: Option<Estimate>,
    /// When each payment is due after its bill; left out of the JSON when due
    /// on billing, the default.
    #[serde(default, skip_serializing_if = "Due::is_on_billing")]
    pub due: Due,
}

impl Plan {
    /// A plan with an exact amount.
    pub fn exact(schedule: Schedule, amount: i64) -> Self {
        Plan {
            schedule,
            amount,
            estimate: None,
            due: Due::OnBilling,
        }
    }

    /// The same plan with payments due as `due` says.
    pub fn due(self, due: Due) -> Self {
        Plan { due, ..self }
    }

    /// A plan whose amount isn't exact: between `low` and `high` (either may
    /// be missing, not both), expected as `expected` says. `manual` is the
    /// expected amount when entered by hand, and ignored otherwise.
    pub fn estimated(
        schedule: Schedule,
        low: Option<i64>,
        high: Option<i64>,
        expected: Expected,
        manual: Option<i64>,
    ) -> Result<Self, SubscriptionError> {
        let estimate = Estimate {
            low,
            high,
            expected,
        };
        let amount = match expected {
            Expected::Midpoint => match (low, high) {
                (Some(l), Some(h)) => midpoint(l, h),
                _ => return Err(SubscriptionError::InvalidEstimate),
            },
            Expected::Bound => match (low, high) {
                (Some(b), None) | (None, Some(b)) => b,
                _ => return Err(SubscriptionError::InvalidEstimate),
            },
            Expected::Manual => manual.ok_or(SubscriptionError::InvalidEstimate)?,
        };
        let plan = Plan {
            schedule,
            amount,
            estimate: Some(estimate),
            due: Due::OnBilling,
        };
        plan.check()?;
        Ok(plan)
    }

    /// The lowest and highest the amount can be: both the amount when exact; a
    /// missing lower bound counts as 0 and a missing upper bound as unknown.
    pub fn range(&self) -> (i64, Option<i64>) {
        match self.estimate {
            None => (self.amount, Some(self.amount)),
            Some(e) => (e.low.unwrap_or(0), e.high),
        }
    }

    fn check(&self) -> Result<(), SubscriptionError> {
        if self.amount < 0 {
            return Err(SubscriptionError::NegativeAmount);
        }
        let Some(e) = self.estimate else {
            return Ok(());
        };
        let bounds_ok = match (e.low, e.high) {
            (None, None) => false,
            (Some(l), Some(h)) => 0 <= l && l <= h,
            (Some(b), None) | (None, Some(b)) => 0 <= b,
        };
        let expected_ok = match (e.expected, e.low, e.high) {
            (Expected::Midpoint, Some(l), Some(h)) => self.amount == midpoint(l, h),
            (Expected::Bound, Some(b), None) | (Expected::Bound, None, Some(b)) => self.amount == b,
            (Expected::Manual, low, high) => {
                low.is_none_or(|l| l <= self.amount) && high.is_none_or(|h| self.amount <= h)
            }
            _ => false,
        };
        if bounds_ok && expected_ok {
            Ok(())
        } else {
            Err(SubscriptionError::InvalidEstimate)
        }
    }
}

/// An amount that isn't exact: its bounds, and how the expected amount (the
/// plan's `amount`) was chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Estimate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub low: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub high: Option<i64>,
    pub expected: Expected,
}

/// How an estimate's expected amount is chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Expected {
    /// The average of the two bounds.
    Midpoint,
    /// The only bound given.
    Bound,
    /// Entered by hand, within the bounds.
    Manual,
}

/// The average of two amounts in minor units, rounded half to even.
fn midpoint(low: i64, high: i64) -> i64 {
    let sum = low + high;
    let half = sum.div_euclid(2);
    if sum.rem_euclid(2) == 1 && half % 2 != 0 {
        half + 1
    } else {
        half
    }
}

/// A value that takes effect on a date and lasts until the next change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Dated<T> {
    pub from: Date,
    pub value: T,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SubscriptionError {
    #[error("name must not be empty")]
    EmptyName,
    #[error("amount must not be negative")]
    NegativeAmount,
    #[error(
        "an estimate needs a lower or upper bound (lower first), and an expected amount within them"
    )]
    InvalidEstimate,
    #[error("a subscription needs at least one plan")]
    NoPlan,
    #[error("change on {0} is before the subscription started on {1}")]
    BeforeStart(Date, Date),
    #[error("changes must be in date order with at most one per day (problem at {0})")]
    Unordered(Date),
    #[error("a free period must end after it starts ({0} to {1})")]
    EmptyFreePeriod(Date, Date),
    #[error("no payment is billed on {0}")]
    NoSuchPayment(Date),
    #[error("only a payment of 0 can be removed (the one billed on {0})")]
    RemovedPaymentNotZero(Date),
}

/// A change to one scheduled payment: what was actually paid, and when.
/// Identified by the bill it changes; an adjustment whose bill no longer
/// exists (after a plan change) is kept but has no effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Adjustment {
    /// The scheduled billing date of the payment it changes.
    pub billed: Date,
    /// The amount paid instead, in minor units; `None` keeps the plan's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<i64>,
    /// When it's due instead.
    #[serde(default, skip_serializing_if = "AdjustedDue::is_scheduled")]
    pub due: AdjustedDue,
}

/// When an adjusted payment is due.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AdjustedDue {
    /// As scheduled.
    #[default]
    Scheduled,
    /// On another date, e.g. paid late.
    On { date: Date },
    /// Not at all: a skipped payment, which must have amount 0.
    Removed,
}

impl AdjustedDue {
    pub fn is_scheduled(&self) -> bool {
        *self == AdjustedDue::Scheduled
    }
}

impl Adjustment {
    fn check(&self) -> Result<(), SubscriptionError> {
        if self.amount.is_some_and(|a| a < 0) {
            return Err(SubscriptionError::NegativeAmount);
        }
        if self.due == AdjustedDue::Removed && self.amount != Some(0) {
            return Err(SubscriptionError::RemovedPaymentNotZero(self.billed));
        }
        Ok(())
    }
}

/// A subscription and its full history.
///
/// Fields are private so that every value satisfies the invariants checked in
/// [`Subscription::new`]. "Edits" return a new value instead of mutating.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "UncheckedSubscription")]
pub struct Subscription {
    id: SubscriptionId,
    name: String,
    currency: Currency,
    /// Non-empty and strictly increasing by date. The first plan's date is the
    /// subscription's start.
    plans: Vec<Dated<Plan>>,
    /// Status changes, strictly increasing by date and none before the start.
    /// Before the first change the subscription is active.
    status_changes: Vec<Dated<Status>>,
    /// Free-form plain text; may be empty. Left out of the JSON when empty, so
    /// snapshots written before notes existed read and compare unchanged.
    #[serde(skip_serializing_if = "String::is_empty")]
    notes: String,
    /// Changes to single payments, by billing date, one per bill. Left out of
    /// the JSON when empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    adjustments: Vec<Adjustment>,
}

/// The serialized form of a [`Subscription`], validated on deserialization.
#[derive(Deserialize)]
struct UncheckedSubscription {
    id: SubscriptionId,
    name: String,
    currency: Currency,
    plans: Vec<Dated<Plan>>,
    status_changes: Vec<Dated<Status>>,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    adjustments: Vec<Adjustment>,
}

impl TryFrom<UncheckedSubscription> for Subscription {
    type Error = SubscriptionError;

    fn try_from(raw: UncheckedSubscription) -> Result<Self, Self::Error> {
        Subscription::from_parts(
            raw.id,
            raw.name,
            raw.currency,
            raw.plans,
            raw.status_changes,
        )
        .map(|s| s.with_notes(&raw.notes))?
        .with_adjustments(raw.adjustments)
    }
}

impl Subscription {
    /// A new, active subscription with a single plan starting on `start`.
    pub fn new(
        id: SubscriptionId,
        name: &str,
        currency: Currency,
        start: Date,
        plan: Plan,
    ) -> Result<Self, SubscriptionError> {
        Subscription::from_parts(
            id,
            name.to_owned(),
            currency,
            vec![Dated {
                from: start,
                value: plan,
            }],
            Vec::new(),
        )
    }

    /// Builds a subscription from its stored parts, checking every invariant.
    pub fn from_parts(
        id: SubscriptionId,
        name: String,
        currency: Currency,
        plans: Vec<Dated<Plan>>,
        status_changes: Vec<Dated<Status>>,
    ) -> Result<Self, SubscriptionError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(SubscriptionError::EmptyName);
        }
        let start = plans.first().ok_or(SubscriptionError::NoPlan)?.from;
        for plan in &plans {
            plan.value.check()?;
        }
        check_increasing(&plans)?;
        check_increasing(&status_changes)?;
        if let Some(first) = status_changes.first().filter(|c| c.from < start) {
            return Err(SubscriptionError::BeforeStart(first.from, start));
        }
        Ok(Subscription {
            id,
            name: name.to_owned(),
            currency,
            plans,
            status_changes,
            notes: String::new(),
            adjustments: Vec::new(),
        })
    }

    pub fn id(&self) -> SubscriptionId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn currency(&self) -> Currency {
        self.currency
    }

    pub fn start(&self) -> Date {
        self.plans[0].from
    }

    pub fn plans(&self) -> &[Dated<Plan>] {
        &self.plans
    }

    pub fn status_changes(&self) -> &[Dated<Status>] {
        &self.status_changes
    }

    pub fn notes(&self) -> &str {
        &self.notes
    }

    /// The same subscription with new notes. Line endings become `\n` and
    /// trailing whitespace is dropped, so the same text always compares equal.
    pub fn with_notes(&self, notes: &str) -> Self {
        Subscription {
            notes: notes.replace("\r\n", "\n").trim_end().to_owned(),
            ..self.clone()
        }
    }

    /// This subscription with new parts, rechecked, keeping its ID, currency
    /// and notes.
    fn rebuilt(
        &self,
        name: String,
        plans: Vec<Dated<Plan>>,
        status_changes: Vec<Dated<Status>>,
    ) -> Result<Self, SubscriptionError> {
        Subscription::from_parts(self.id, name, self.currency, plans, status_changes)?
            .with_notes(&self.notes)
            .with_adjustments(self.adjustments.clone())
    }

    pub fn adjustments(&self) -> &[Adjustment] {
        &self.adjustments
    }

    /// The same subscription with these changes to single payments, checked:
    /// no negative amounts, one per bill, and only payments of 0 removed.
    pub fn with_adjustments(
        &self,
        mut adjustments: Vec<Adjustment>,
    ) -> Result<Self, SubscriptionError> {
        adjustments.sort_by_key(|a| a.billed);
        if let Some(w) = adjustments.windows(2).find(|w| w[0].billed == w[1].billed) {
            return Err(SubscriptionError::Unordered(w[1].billed));
        }
        for a in &adjustments {
            a.check()?;
        }
        Ok(Subscription {
            adjustments,
            ..self.clone()
        })
    }

    /// The subscription with one payment changed, replacing any earlier change
    /// to it. The bill must exist.
    pub fn with_adjustment(&self, adjustment: Adjustment) -> Result<Self, SubscriptionError> {
        if !self.is_scheduled_bill(adjustment.billed) {
            return Err(SubscriptionError::NoSuchPayment(adjustment.billed));
        }
        let mut adjustments: Vec<Adjustment> = self
            .adjustments
            .iter()
            .filter(|a| a.billed != adjustment.billed)
            .copied()
            .collect();
        adjustments.push(adjustment);
        self.with_adjustments(adjustments)
    }

    /// The subscription with the change to the payment billed on `billed`
    /// undone, so it is paid as scheduled again.
    pub fn without_adjustment(&self, billed: Date) -> Self {
        Subscription {
            adjustments: self
                .adjustments
                .iter()
                .filter(|a| a.billed != billed)
                .copied()
                .collect(),
            ..self.clone()
        }
    }

    /// Whether a payment is billed on `date` under the plans and statuses,
    /// before any change to single payments.
    pub fn is_scheduled_bill(&self, date: Date) -> bool {
        let next_day = date.tomorrow().unwrap_or(Date::MAX);
        self.scheduled_bills(date, next_day)
            .any(|p| p.billed == date)
    }

    /// How far adjustments move payments: the most days later and the most
    /// days earlier than their bills.
    fn adjustment_shifts(&self) -> (i64, i64) {
        self.adjustments
            .iter()
            .filter_map(|a| match a.due {
                AdjustedDue::On { date } => Some(i64::from(a.billed.until(date).ok()?.get_days())),
                _ => None,
            })
            .fold((0, 0), |(late, early), d| (late.max(d), early.max(-d)))
    }

    /// The same subscription under a new name.
    pub fn renamed(&self, name: &str) -> Result<Self, SubscriptionError> {
        self.rebuilt(
            name.to_owned(),
            self.plans.clone(),
            self.status_changes.clone(),
        )
    }

    /// The subscription with `plan` in effect from `from` until the next recorded
    /// plan change. A change on the date of an existing one replaces it, so this
    /// covers both recording a price change and correcting one.
    pub fn with_plan(&self, from: Date, plan: Plan) -> Result<Self, SubscriptionError> {
        let plans = insert_dated(&self.plans, Dated { from, value: plan });
        self.rebuilt(self.name.clone(), plans, self.status_changes.clone())
    }

    /// The subscription with nothing to pay in `[from, until)`, such as a free
    /// trial or time already paid for: the plan in effect on `from` continues
    /// with amount 0, then at its own amount again from `until`. Recorded as
    /// plans, so the free period shows in the history like any price change.
    ///
    /// Unchanged if no payment falls in the period anyway, so a next payment
    /// that is simply the next one on the schedule adds nothing.
    pub fn with_free_period(&self, from: Date, until: Date) -> Result<Self, SubscriptionError> {
        if until <= from {
            return Err(SubscriptionError::EmptyFreePeriod(from, until));
        }
        if self.payments_between(from, until).next().is_none() {
            return Ok(self.clone());
        }
        let plan = *self
            .plan_on(from)
            .ok_or(SubscriptionError::BeforeStart(from, self.start()))?;
        let free = Plan {
            amount: 0,
            estimate: None,
            ..plan
        };
        self.with_plan(from, free)?.with_plan(until, plan)
    }

    /// The subscription with its status changing to `status` on `from`. Like
    /// [`Subscription::with_plan`], this replaces a change on the same date.
    pub fn with_status(&self, from: Date, status: Status) -> Result<Self, SubscriptionError> {
        let changes = insert_dated(
            &self.status_changes,
            Dated {
                from,
                value: status,
            },
        );
        self.rebuilt(self.name.clone(), self.plans.clone(), changes)
    }

    /// The plan in effect on `date`, or `None` before the subscription started.
    pub fn plan_on(&self, date: Date) -> Option<&Plan> {
        value_on(&self.plans, date)
    }

    /// The status on `date`, or `None` before the subscription started.
    pub fn status_on(&self, date: Date) -> Option<Status> {
        if date < self.start() {
            return None;
        }
        Some(
            value_on(&self.status_changes, date)
                .copied()
                .unwrap_or(Status::Active),
        )
    }

    /// Every payment due in `[from, to)`, by due date, ordered by billing date
    /// (which is due-date order unless plans with different due rules meet).
    /// Plans with amount 0 have none. Plans and status changes apply by
    /// billing date.
    ///
    /// These come from the subscription's recorded plans and statuses, not from
    /// bank data: payments in the past are estimates of what was paid.
    pub fn payments_between(&self, from: Date, to: Date) -> impl Iterator<Item = Payment> + '_ {
        // A bill issued before `from` can fall due within the range, up to the
        // longest due delay or later payment; a bill after `to` can only if a
        // payment was moved earlier.
        let (late, early) = self.adjustment_shifts();
        let lookback = self
            .plans
            .iter()
            .map(|p| p.value.due.max_delay_days())
            .max()
            .unwrap_or(0)
            .max(late);
        let bills_from = from
            .checked_sub(jiff::Span::new().days(lookback))
            .unwrap_or(Date::MIN);
        let bills_to = to
            .checked_add(jiff::Span::new().days(early))
            .unwrap_or(Date::MAX);
        self.bills_between(bills_from, bills_to)
            .filter(move |p| from <= p.date && p.date < to)
    }

    /// Every payment billed in `[from, to)`, whenever it falls due, in billing
    /// order, with changes to single payments applied.
    fn bills_between(&self, from: Date, to: Date) -> impl Iterator<Item = Payment> + '_ {
        self.scheduled_bills(from, to).filter_map(move |p| {
            let Ok(i) = self
                .adjustments
                .binary_search_by_key(&p.billed, |a| a.billed)
            else {
                return Some(p);
            };
            let a = self.adjustments[i];
            let date = match a.due {
                AdjustedDue::Scheduled => p.date,
                AdjustedDue::On { date } => date,
                AdjustedDue::Removed => return None,
            };
            Some(Payment {
                date,
                amount: a.amount.map_or(p.amount, |m| Money::new(m, self.currency)),
                adjusted: true,
                ..p
            })
        })
    }

    /// Every payment billed in `[from, to)` under the plans and statuses, in
    /// billing order, before any change to single payments.
    fn scheduled_bills(&self, from: Date, to: Date) -> impl Iterator<Item = Payment> + '_ {
        let plan_ends = self.plans.iter().skip(1).map(|p| p.from).chain([Date::MAX]);
        self.plans
            .iter()
            .zip(plan_ends)
            .flat_map(move |(plan, plan_end)| {
                let lo = plan.from.max(from);
                let hi = plan_end.min(to);
                plan.value
                    .schedule
                    .dates_between(lo, hi)
                    .map(move |billed| (billed, plan.value))
            })
            // Nothing is paid under a plan with amount 0 (a free period), so
            // it has no payments, and the next payment is the next real one.
            .filter(|(_, plan)| plan.amount != 0)
            .filter(move |&(billed, _)| self.status_on(billed) == Some(Status::Active))
            .filter_map(move |(billed, plan)| {
                Some(Payment {
                    subscription: self.id,
                    billed,
                    date: plan.due.on(billed)?,
                    amount: Money::new(plan.amount, self.currency),
                    adjusted: false,
                })
            })
    }

    /// The next payment on or after `date`, if any is still due.
    pub fn next_payment(&self, date: Date) -> Option<Payment> {
        // Once the last status change isn't "active", nothing is due after it.
        // Bounding the search by it stops a cancelled subscription from scanning
        // the calendar until the year 9999.
        let horizon = match self.status_changes.last() {
            Some(last) if last.value != Status::Active => last.from,
            _ => Date::MAX,
        };
        // Bills come in billing order, and each falls due on or after its
        // billing date. So once a bill is issued after the earliest due date
        // seen, no later bill can fall due sooner.
        // A payment moved earlier can fall due before its bill, by at most
        // `early` days.
        let (late, early) = self.adjustment_shifts();
        let lookback = self
            .plans
            .iter()
            .map(|p| p.value.due.max_delay_days())
            .max()
            .unwrap_or(0)
            .max(late);
        let bills_from = date
            .checked_sub(jiff::Span::new().days(lookback))
            .unwrap_or(Date::MIN);
        let mut best: Option<Payment> = None;
        for payment in self.bills_between(bills_from, horizon) {
            let earliest_due = payment
                .billed
                .checked_sub(jiff::Span::new().days(early))
                .unwrap_or(Date::MIN);
            if best.is_some_and(|b| earliest_due > b.date) {
                break;
            }
            if payment.date >= date && best.is_none_or(|b| payment.date < b.date) {
                best = Some(payment);
            }
        }
        best
    }
}

/// One payment of a subscription.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Payment {
    pub subscription: SubscriptionId,
    /// When it's billed.
    pub billed: Date,
    /// When it's due: when the money actually leaves. Cash flow uses this.
    pub date: Date,
    pub amount: Money,
    /// Changed from what the plan says (see [`Adjustment`]).
    pub adjusted: bool,
}

fn check_increasing<T>(changes: &[Dated<T>]) -> Result<(), SubscriptionError> {
    match changes.windows(2).find(|w| w[0].from >= w[1].from) {
        Some(w) => Err(SubscriptionError::Unordered(w[1].from)),
        None => Ok(()),
    }
}

/// The value of the last change on or before `date`.
fn value_on<T>(changes: &[Dated<T>], date: Date) -> Option<&T> {
    let after = changes.partition_point(|c| c.from <= date);
    after.checked_sub(1).map(|i| &changes[i].value)
}

/// A copy of `changes` with `new` inserted in date order, replacing any change on
/// the same date.
fn insert_dated<T: Clone>(changes: &[Dated<T>], new: Dated<T>) -> Vec<Dated<T>> {
    let mut result: Vec<Dated<T>> = changes
        .iter()
        .filter(|c| c.from != new.from)
        .cloned()
        .collect();
    let at = result.partition_point(|c| c.from < new.from);
    result.insert(at, new);
    result
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::recurrence::{DayOfMonth, Due, Rule};
    use jiff::civil::date;
    use std::num::NonZeroU16;

    pub(crate) fn monthly_plan(anchor: Date, amount: i64) -> Plan {
        let rule = Rule::Months {
            every: NonZeroU16::MIN,
            day: DayOfMonth::new(anchor.day() as u8).unwrap(),
        };
        Plan {
            schedule: Schedule::new(rule, anchor).unwrap(),
            amount,
            estimate: None,
            due: Default::default(),
        }
    }

    pub(crate) fn subscription(start: Date, plan: Plan) -> Subscription {
        Subscription::new(
            SubscriptionId::generate(),
            "Test",
            Currency::EUR,
            start,
            plan,
        )
        .unwrap()
    }

    fn payment_dates(s: &Subscription, from: Date, to: Date) -> Vec<(Date, i64)> {
        s.payments_between(from, to)
            .map(|p| (p.date, p.amount.minor))
            .collect()
    }

    #[test]
    fn notes_survive_other_edits_and_are_normalised() {
        let s = subscription(date(2026, 1, 15), monthly_plan(date(2026, 1, 15), 1000))
            .with_notes("Family plan\r\nShared with Ana  \n\n");
        assert_eq!(s.notes(), "Family plan\nShared with Ana");
        let edited = s
            .renamed("Music")
            .unwrap()
            .with_plan(date(2026, 3, 1), monthly_plan(date(2026, 1, 15), 1200))
            .unwrap()
            .with_status(date(2026, 4, 1), Status::Paused)
            .unwrap();
        assert_eq!(edited.notes(), s.notes());
    }

    #[test]
    fn empty_notes_are_left_out_of_the_json() {
        let s = subscription(date(2026, 1, 15), monthly_plan(date(2026, 1, 15), 1000));
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("notes"), "{json}");
        let back: Subscription = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);

        let with = s.with_notes("Shared");
        let json = serde_json::to_string(&with).unwrap();
        assert!(json.contains(r#""notes":"Shared""#), "{json}");
        assert_eq!(serde_json::from_str::<Subscription>(&json).unwrap(), with);
    }

    fn schedule() -> Schedule {
        monthly_plan(date(2026, 1, 15), 0).schedule
    }

    #[test]
    fn estimates_choose_their_expected_amount() {
        let avg = Plan::estimated(schedule(), Some(2000), Some(3001), Expected::Midpoint, None);
        // 25.005 rounds half to even.
        assert_eq!(avg.unwrap().amount, 2500);
        let upper = Plan::estimated(schedule(), None, Some(3000), Expected::Bound, None).unwrap();
        assert_eq!(upper.amount, 3000);
        assert_eq!(upper.range(), (0, Some(3000)));
        let lower = Plan::estimated(schedule(), Some(2000), None, Expected::Bound, None).unwrap();
        assert_eq!(lower.range(), (2000, None));
        let manual = Plan::estimated(
            schedule(),
            Some(2000),
            Some(3000),
            Expected::Manual,
            Some(2800),
        );
        assert_eq!(manual.unwrap().amount, 2800);
        assert_eq!(Plan::exact(schedule(), 999).range(), (999, Some(999)));
    }

    #[test]
    fn rejects_estimates_that_dont_add_up() {
        let bad = |low, high, expected, manual| {
            Plan::estimated(schedule(), low, high, expected, manual)
                == Err(SubscriptionError::InvalidEstimate)
        };
        assert!(bad(None, None, Expected::Manual, Some(100)));
        assert!(bad(Some(3000), Some(2000), Expected::Midpoint, None));
        assert!(bad(Some(2000), None, Expected::Midpoint, None));
        assert!(bad(Some(2000), Some(3000), Expected::Bound, None));
        assert!(bad(Some(2000), Some(3000), Expected::Manual, Some(3500)));
        assert!(bad(Some(2000), Some(3000), Expected::Manual, None));
        // A stored plan whose amount disagrees with its estimate is refused too.
        let mut plan =
            Plan::estimated(schedule(), Some(2000), Some(3000), Expected::Midpoint, None).unwrap();
        plan.amount = 2600;
        let s = subscription(date(2026, 1, 15), monthly_plan(date(2026, 1, 15), 999));
        assert_eq!(
            s.with_plan(date(2026, 3, 1), plan),
            Err(SubscriptionError::InvalidEstimate)
        );
    }

    #[test]
    fn exact_plans_serialize_as_before() {
        let json = serde_json::to_value(monthly_plan(date(2026, 1, 15), 999)).unwrap();
        assert!(json.get("estimate").is_none(), "{json}");
        let estimated =
            Plan::estimated(schedule(), None, Some(3000), Expected::Bound, None).unwrap();
        let json = serde_json::to_value(estimated).unwrap();
        assert_eq!(
            json["estimate"],
            serde_json::json!({"high": 3000, "expected": "bound"})
        );
        assert_eq!(serde_json::from_value::<Plan>(json).unwrap(), estimated);
    }

    fn due_dates(s: &Subscription, from: Date, to: Date) -> Vec<(Date, Date)> {
        s.payments_between(from, to)
            .map(|p| (p.billed, p.date))
            .collect()
    }

    #[test]
    fn payments_fall_on_their_due_date() {
        // Billed on the 6th, due on the 20th.
        let plan = monthly_plan(date(2026, 1, 6), 3000).due(Due::DayOfMonth {
            day: DayOfMonth::new(20).unwrap(),
        });
        let s = subscription(date(2026, 1, 6), plan);
        assert_eq!(
            due_dates(&s, date(2026, 11, 1), date(2027, 1, 1)),
            [
                (date(2026, 11, 6), date(2026, 11, 20)),
                (date(2026, 12, 6), date(2026, 12, 20))
            ]
        );
        // A bill before the range that falls due in it counts; one due after
        // the range doesn't.
        assert_eq!(
            due_dates(&s, date(2026, 11, 15), date(2026, 12, 15)),
            [(date(2026, 11, 6), date(2026, 11, 20))]
        );
        let next = s.next_payment(date(2026, 11, 10)).unwrap();
        assert_eq!(
            (next.billed, next.date),
            (date(2026, 11, 6), date(2026, 11, 20))
        );
    }

    #[test]
    fn the_next_payment_is_the_soonest_due_across_plans() {
        // Due 20 days after billing until December, then due when billed: the
        // December 5th bill (due that day) comes before the November 25th
        // bill's due date (December 15th).
        let late = monthly_plan(date(2026, 1, 25), 1000).due(Due::DaysAfter { days: 20 });
        let s = subscription(date(2026, 1, 25), late)
            .with_plan(date(2026, 12, 1), monthly_plan(date(2026, 12, 5), 1000))
            .unwrap();
        let next = s.next_payment(date(2026, 12, 1)).unwrap();
        assert_eq!(
            (next.billed, next.date),
            (date(2026, 12, 5), date(2026, 12, 5))
        );
    }

    fn paid(s: &Subscription, from: Date, to: Date) -> Vec<(Date, Date, i64, bool)> {
        s.payments_between(from, to)
            .map(|p| (p.billed, p.date, p.amount.minor, p.adjusted))
            .collect()
    }

    #[test]
    fn single_payments_can_be_changed() {
        let s = subscription(date(2026, 1, 15), monthly_plan(date(2026, 1, 15), 1000));
        let (nov, dec) = (date(2026, 11, 15), date(2026, 12, 15));
        let changed = s
            .with_adjustment(Adjustment {
                billed: nov,
                amount: Some(1350),
                due: AdjustedDue::Scheduled,
            })
            .unwrap()
            .with_adjustment(Adjustment {
                billed: dec,
                amount: None,
                due: AdjustedDue::On {
                    date: date(2027, 1, 3),
                },
            })
            .unwrap();
        assert_eq!(
            paid(&changed, date(2026, 11, 1), date(2027, 2, 1)),
            // In billing order: December's bill, paid on January 3rd, comes
            // before January's.
            [
                (nov, nov, 1350, true),
                (dec, date(2027, 1, 3), 1000, true),
                (date(2027, 1, 15), date(2027, 1, 15), 1000, false),
            ]
        );
        // The December payment, paid in January, counts in January's range.
        assert_eq!(paid(&changed, date(2026, 12, 1), date(2027, 1, 1)), []);
        // Survives other edits and JSON.
        let renamed = changed.renamed("Music").unwrap();
        assert_eq!(renamed.adjustments(), changed.adjustments());
        let json = serde_json::to_string(&renamed).unwrap();
        assert_eq!(
            serde_json::from_str::<Subscription>(&json).unwrap(),
            renamed
        );
        // Undone, it's paid as scheduled again.
        let undone = changed.without_adjustment(nov);
        assert_eq!(paid(&undone, nov, dec), [(nov, nov, 1000, false)]);
    }

    #[test]
    fn a_skipped_payment_is_zero_and_can_be_removed() {
        let s = subscription(date(2026, 1, 15), monthly_plan(date(2026, 1, 15), 1000));
        let nov = date(2026, 11, 15);
        let zero = s
            .with_adjustment(Adjustment {
                billed: nov,
                amount: Some(0),
                due: AdjustedDue::Scheduled,
            })
            .unwrap();
        // A payment of 0 keeps its date by default.
        assert_eq!(paid(&zero, nov, date(2026, 12, 1)), [(nov, nov, 0, true)]);
        let removed = s
            .with_adjustment(Adjustment {
                billed: nov,
                amount: Some(0),
                due: AdjustedDue::Removed,
            })
            .unwrap();
        assert_eq!(paid(&removed, nov, date(2026, 12, 1)), []);
        // Only a payment of 0 can be removed, and only a real bill changed.
        assert_eq!(
            s.with_adjustment(Adjustment {
                billed: nov,
                amount: None,
                due: AdjustedDue::Removed,
            }),
            Err(SubscriptionError::RemovedPaymentNotZero(nov))
        );
        assert_eq!(
            s.with_adjustment(Adjustment {
                billed: date(2026, 11, 16),
                amount: Some(5),
                due: AdjustedDue::Scheduled,
            }),
            Err(SubscriptionError::NoSuchPayment(date(2026, 11, 16)))
        );
    }

    #[test]
    fn a_payment_moved_earlier_can_be_the_next_one() {
        let s = subscription(date(2026, 1, 15), monthly_plan(date(2026, 1, 15), 1000))
            .with_adjustment(Adjustment {
                billed: date(2026, 12, 15),
                amount: None,
                due: AdjustedDue::On {
                    date: date(2026, 11, 20),
                },
            })
            .unwrap();
        let next = s.next_payment(date(2026, 11, 16)).unwrap();
        assert_eq!(
            (next.billed, next.date),
            (date(2026, 12, 15), date(2026, 11, 20))
        );
    }

    #[test]
    fn a_free_period_skips_payments_then_bills_as_before() {
        let s = subscription(date(2026, 1, 12), monthly_plan(date(2026, 1, 12), 2000))
            .with_free_period(date(2026, 11, 3), date(2027, 3, 12))
            .unwrap();
        assert_eq!(
            payment_dates(&s, date(2026, 10, 1), date(2027, 5, 1)),
            [
                (date(2026, 10, 12), 2000),
                (date(2027, 3, 12), 2000),
                (date(2027, 4, 12), 2000)
            ]
        );
        assert_eq!(s.plan_on(date(2026, 12, 1)).unwrap().amount, 0);
    }

    #[test]
    fn a_free_period_without_payments_in_it_changes_nothing() {
        let s = subscription(date(2026, 1, 12), monthly_plan(date(2026, 1, 12), 2000));
        let same = s
            .with_free_period(date(2026, 11, 3), date(2026, 11, 12))
            .unwrap();
        assert_eq!(same, s);
        assert!(
            s.with_free_period(date(2026, 11, 3), date(2026, 11, 3))
                .is_err()
        );
    }

    #[test]
    fn no_payments_before_the_start() {
        let s = subscription(date(2026, 3, 10), monthly_plan(date(2026, 1, 10), 999));
        assert_eq!(
            payment_dates(&s, date(2026, 1, 1), date(2026, 5, 1)),
            [(date(2026, 3, 10), 999), (date(2026, 4, 10), 999)]
        );
        assert_eq!(s.status_on(date(2026, 3, 9)), None);
        assert_eq!(s.plan_on(date(2026, 3, 9)), None);
    }

    #[test]
    fn price_changes_apply_from_their_date() {
        let s = subscription(date(2026, 1, 15), monthly_plan(date(2026, 1, 15), 1000))
            .with_plan(date(2026, 3, 1), monthly_plan(date(2026, 1, 15), 1200))
            .unwrap();
        assert_eq!(
            payment_dates(&s, date(2026, 1, 1), date(2026, 5, 1)),
            [
                (date(2026, 1, 15), 1000),
                (date(2026, 2, 15), 1000),
                (date(2026, 3, 15), 1200),
                (date(2026, 4, 15), 1200)
            ]
        );
    }

    #[test]
    fn switching_from_monthly_to_yearly_billing() {
        let yearly = Plan {
            schedule: Schedule::new(
                Rule::Years {
                    every: NonZeroU16::MIN,
                    month: crate::recurrence::Month::new(4).unwrap(),
                    day: DayOfMonth::new(1).unwrap(),
                },
                date(2026, 4, 1),
            )
            .unwrap(),
            amount: 9900,
            estimate: None,
            due: Default::default(),
        };
        let s = subscription(date(2026, 1, 1), monthly_plan(date(2026, 1, 1), 999))
            .with_plan(date(2026, 4, 1), yearly)
            .unwrap();
        assert_eq!(
            payment_dates(&s, date(2026, 1, 1), date(2028, 1, 1)),
            [
                (date(2026, 1, 1), 999),
                (date(2026, 2, 1), 999),
                (date(2026, 3, 1), 999),
                (date(2026, 4, 1), 9900),
                (date(2027, 4, 1), 9900)
            ]
        );
    }

    #[test]
    fn pauses_and_cancellations_stop_payments_from_their_date() {
        let s = subscription(date(2026, 1, 5), monthly_plan(date(2026, 1, 5), 500))
            .with_status(date(2026, 3, 1), Status::Paused)
            .unwrap()
            .with_status(date(2026, 5, 5), Status::Active)
            .unwrap()
            .with_status(date(2026, 7, 5), Status::Cancelled)
            .unwrap();
        let dates: Vec<Date> = s
            .payments_between(date(2026, 1, 1), date(2027, 1, 1))
            .map(|p| p.date)
            .collect();
        // Paused for March and April; the cancellation date itself isn't billed.
        assert_eq!(
            dates,
            [
                date(2026, 1, 5),
                date(2026, 2, 5),
                date(2026, 5, 5),
                date(2026, 6, 5)
            ]
        );
        assert_eq!(s.status_on(date(2026, 4, 1)), Some(Status::Paused));
        assert_eq!(s.status_on(date(2030, 1, 1)), Some(Status::Cancelled));
    }

    #[test]
    fn next_payment_skips_pauses_and_ends_after_cancellation() {
        let s = subscription(date(2026, 1, 5), monthly_plan(date(2026, 1, 5), 500))
            .with_status(date(2026, 3, 1), Status::Paused)
            .unwrap();
        assert_eq!(s.next_payment(date(2026, 2, 6)), None);

        let resumed = s.with_status(date(2026, 6, 20), Status::Active).unwrap();
        assert_eq!(
            resumed.next_payment(date(2026, 2, 6)).unwrap().date,
            date(2026, 7, 5)
        );

        let cancelled = resumed
            .with_status(date(2026, 7, 1), Status::Cancelled)
            .unwrap();
        assert_eq!(cancelled.next_payment(date(2026, 2, 6)), None);
        assert_eq!(
            cancelled.next_payment(date(2026, 1, 6)).unwrap().date,
            date(2026, 2, 5)
        );
    }

    #[test]
    fn a_change_on_an_existing_date_replaces_it() {
        let s = subscription(date(2026, 1, 1), monthly_plan(date(2026, 1, 1), 1000))
            .with_plan(date(2026, 6, 1), monthly_plan(date(2026, 1, 1), 1100))
            .unwrap()
            .with_plan(date(2026, 6, 1), monthly_plan(date(2026, 1, 1), 1150))
            .unwrap();
        assert_eq!(s.plans().len(), 2);
        assert_eq!(s.plan_on(date(2026, 6, 1)).unwrap().amount, 1150);
        // Replacing the first plan keeps the start date.
        let s = s
            .with_plan(date(2026, 1, 1), monthly_plan(date(2026, 1, 1), 900))
            .unwrap();
        assert_eq!(s.plan_on(date(2026, 1, 1)).unwrap().amount, 900);
    }

    #[test]
    fn changes_can_be_recorded_out_of_order() {
        let s = subscription(date(2024, 1, 1), monthly_plan(date(2024, 1, 1), 1000))
            .with_plan(date(2026, 1, 1), monthly_plan(date(2024, 1, 1), 1300))
            .unwrap()
            .with_plan(date(2025, 1, 1), monthly_plan(date(2024, 1, 1), 1150))
            .unwrap();
        let amounts: Vec<i64> = s.plans().iter().map(|p| p.value.amount).collect();
        assert_eq!(amounts, [1000, 1150, 1300]);
    }

    #[test]
    fn a_plan_before_the_start_moves_the_start() {
        let s = subscription(date(2026, 3, 1), monthly_plan(date(2026, 3, 1), 1000))
            .with_plan(date(2026, 1, 1), monthly_plan(date(2026, 1, 1), 800))
            .unwrap();
        assert_eq!(s.start(), date(2026, 1, 1));
    }

    #[test]
    fn rejects_invalid_subscriptions() {
        let plan = monthly_plan(date(2026, 1, 1), 1000);
        let id = SubscriptionId::generate();
        assert_eq!(
            Subscription::new(id, "  ", Currency::EUR, date(2026, 1, 1), plan),
            Err(SubscriptionError::EmptyName)
        );
        let negative = Plan { amount: -1, ..plan };
        assert_eq!(
            Subscription::new(id, "X", Currency::EUR, date(2026, 1, 1), negative),
            Err(SubscriptionError::NegativeAmount)
        );
        let s = subscription(date(2026, 1, 1), plan);
        assert_eq!(
            s.with_status(date(2025, 12, 31), Status::Paused),
            Err(SubscriptionError::BeforeStart(
                date(2025, 12, 31),
                date(2026, 1, 1)
            ))
        );
        assert!(matches!(
            Subscription::from_parts(id, "X".into(), Currency::EUR, vec![], vec![]),
            Err(SubscriptionError::NoPlan)
        ));
    }

    #[test]
    fn json_round_trip_and_validation() {
        let s = subscription(date(2026, 1, 31), monthly_plan(date(2026, 1, 31), 1299))
            .with_status(date(2026, 5, 1), Status::Paused)
            .unwrap();
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Subscription>(&json).unwrap(), s);

        let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
        value["name"] = "".into();
        assert!(serde_json::from_value::<Subscription>(value).is_err());
    }
}
