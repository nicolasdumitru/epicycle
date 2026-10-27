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
use crate::recurrence::Schedule;

/// Stable identifier that survives export and import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SubscriptionId(pub Uuid);

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
    /// Amount per payment, in minor units of the subscription's currency.
    pub amount: i64,
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
    #[error("a subscription needs at least one plan")]
    NoPlan,
    #[error("change on {0} is before the subscription started on {1}")]
    BeforeStart(Date, Date),
    #[error("changes must be in date order with at most one per day (problem at {0})")]
    Unordered(Date),
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
}

/// The serialized form of a [`Subscription`], validated on deserialization.
#[derive(Deserialize)]
struct UncheckedSubscription {
    id: SubscriptionId,
    name: String,
    currency: Currency,
    plans: Vec<Dated<Plan>>,
    status_changes: Vec<Dated<Status>>,
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
        if plans.iter().any(|p| p.value.amount < 0) {
            return Err(SubscriptionError::NegativeAmount);
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

    /// The same subscription under a new name.
    pub fn renamed(&self, name: &str) -> Result<Self, SubscriptionError> {
        Subscription::from_parts(
            self.id,
            name.to_owned(),
            self.currency,
            self.plans.clone(),
            self.status_changes.clone(),
        )
    }

    /// The subscription with `plan` in effect from `from` until the next recorded
    /// plan change. A change on the date of an existing one replaces it, so this
    /// covers both recording a price change and correcting one.
    pub fn with_plan(&self, from: Date, plan: Plan) -> Result<Self, SubscriptionError> {
        let plans = insert_dated(&self.plans, Dated { from, value: plan });
        Subscription::from_parts(
            self.id,
            self.name.clone(),
            self.currency,
            plans,
            self.status_changes.clone(),
        )
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
        Subscription::from_parts(
            self.id,
            self.name.clone(),
            self.currency,
            self.plans.clone(),
            changes,
        )
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

    /// Every payment due in `[from, to)`, in date order.
    ///
    /// These come from the subscription's recorded plans and statuses, not from
    /// bank data: payments in the past are estimates of what was paid.
    pub fn payments_between(&self, from: Date, to: Date) -> impl Iterator<Item = Payment> + '_ {
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
                    .map(move |date| (date, plan.value.amount))
            })
            .filter(move |&(date, _)| self.status_on(date) == Some(Status::Active))
            .map(move |(date, amount)| Payment {
                subscription: self.id,
                date,
                amount: Money::new(amount, self.currency),
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
        self.payments_between(date, horizon).next()
    }
}

/// One payment of a subscription.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Payment {
    pub subscription: SubscriptionId,
    pub date: Date,
    pub amount: Money,
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
    use crate::recurrence::{DayOfMonth, Rule};
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
