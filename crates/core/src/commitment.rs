//! The current recurring commitment: what active subscriptions cost per month and
//! per year at the prices in effect on a given day, regardless of when the
//! payments actually fall.
//!
//! Calendar rules convert exactly: amount `a` every `n` months is `12a / n` per
//! year. Day and week rules have no whole number of payments per year, so they
//! use the mean Gregorian year of 365.2425 days (see `DECISIONS.md`).

use jiff::civil::Date;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::money::{Currency, Fractional, Money, MoneyError, MultiCurrency};
use crate::recurrence::Rule;
use crate::subscription::{Plan, Status, Subscription};

/// Mean length of a Gregorian year in days: 365 + 1/4 − 1/100 + 1/400.
pub const DAYS_PER_YEAR: Decimal = Decimal::from_parts(3_652_425, 0, 0, false, 4);

/// How many payments a rule makes per year, as an exact fraction
/// `(numerator, denominator)`.
pub fn payments_per_year(rule: Rule) -> (Decimal, Decimal) {
    match rule {
        Rule::Days { every } => (DAYS_PER_YEAR, Decimal::from(every.get())),
        Rule::Weeks { every } => (DAYS_PER_YEAR, Decimal::from(7 * u32::from(every.get()))),
        Rule::Months { every, .. } => (Decimal::from(12), Decimal::from(every.get())),
        Rule::Years { every, .. } => (Decimal::ONE, Decimal::from(every.get())),
    }
}

/// Monthly and annual equivalents of a recurring cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commitment {
    pub monthly: Fractional,
    pub annual: Fractional,
}

impl Commitment {
    /// The cost of a plan billed in `currency`, normalised to a month and a year.
    pub fn of_plan(plan: &Plan, currency: Currency) -> Result<Commitment, MoneyError> {
        let amount = Fractional::from(Money::new(plan.amount, currency));
        let (per_year, divisor) = payments_per_year(plan.schedule.rule());
        let monthly_divisor = divisor
            .checked_mul(Decimal::from(12))
            .ok_or(MoneyError::Overflow)?;
        Ok(Commitment {
            annual: amount
                .checked_scale(per_year, divisor)
                .ok_or(MoneyError::Overflow)?,
            monthly: amount
                .checked_scale(per_year, monthly_divisor)
                .ok_or(MoneyError::Overflow)?,
        })
    }
}

/// What a subscription commits to on `date`, or `None` if it isn't active then.
pub fn commitment_on(
    subscription: &Subscription,
    date: Date,
) -> Option<Result<Commitment, MoneyError>> {
    if subscription.status_on(date)? != Status::Active {
        return None;
    }
    let plan = subscription.plan_on(date)?;
    Some(Commitment::of_plan(plan, subscription.currency()))
}

/// The combined commitment of several subscriptions, per currency.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitmentTotals {
    pub monthly: MultiCurrency,
    pub annual: MultiCurrency,
}

/// Sums the commitments of the subscriptions active on `date`.
pub fn total_commitment_on<'a>(
    subscriptions: impl IntoIterator<Item = &'a Subscription>,
    date: Date,
) -> Result<CommitmentTotals, MoneyError> {
    subscriptions
        .into_iter()
        .filter_map(|s| commitment_on(s, date))
        .try_fold(CommitmentTotals::default(), |totals, commitment| {
            let commitment = commitment?;
            Ok(CommitmentTotals {
                monthly: totals.monthly.checked_add(commitment.monthly)?,
                annual: totals.annual.checked_add(commitment.annual)?,
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recurrence::{DayOfMonth, Month, Schedule};
    use crate::subscription::SubscriptionId;
    use jiff::civil::date;
    use rust_decimal_macros::dec;
    use std::num::NonZeroU16;

    fn every(n: u16) -> NonZeroU16 {
        NonZeroU16::new(n).unwrap()
    }

    fn plan(rule: Rule, amount: i64) -> Plan {
        Plan {
            schedule: Schedule::starting(rule, date(2026, 1, 1)).unwrap(),
            amount,
        }
    }

    fn months(n: u16) -> Rule {
        Rule::Months {
            every: every(n),
            day: DayOfMonth::new(1).unwrap(),
        }
    }

    fn years(n: u16) -> Rule {
        Rule::Years {
            every: every(n),
            month: Month::new(1).unwrap(),
            day: DayOfMonth::new(1).unwrap(),
        }
    }

    /// (monthly, annual) in minor units.
    fn equivalents(rule: Rule, amount: i64) -> (Decimal, Decimal) {
        let c = Commitment::of_plan(&plan(rule, amount), Currency::EUR).unwrap();
        (c.monthly.minor, c.annual.minor)
    }

    #[test]
    fn calendar_rules_convert_exactly() {
        assert_eq!(equivalents(months(1), 1000), (dec!(1000), dec!(12000)));
        // The spec's example: EUR 120 a year is EUR 10 a month.
        assert_eq!(equivalents(years(1), 12000), (dec!(1000), dec!(12000)));
        assert_eq!(equivalents(months(3), 3000), (dec!(1000), dec!(12000)));
        assert_eq!(equivalents(years(2), 24000), (dec!(1000), dec!(12000)));
        assert_eq!(equivalents(months(6), 1000).1, dec!(2000));
    }

    #[test]
    fn day_and_week_rules_use_the_mean_gregorian_year() {
        // 365.2425 / 7 = 52.1775 weeks per year.
        assert_eq!(
            equivalents(Rule::Weeks { every: every(1) }, 1000),
            (dec!(4348.125), dec!(52177.5))
        );
        assert_eq!(
            equivalents(Rule::Weeks { every: every(2) }, 1000).1,
            dec!(26088.75)
        );
        assert_eq!(
            equivalents(Rule::Days { every: every(1) }, 100),
            (dec!(3043.6875), dec!(36524.25))
        );
    }

    #[test]
    fn inexact_values_stay_exact_until_rounded() {
        let (monthly, annual) = equivalents(months(3), 1000);
        assert_eq!(annual, dec!(4000));
        // 1000 / 3 cents a month, rounded only for display.
        assert_eq!(
            Fractional::new(monthly, Currency::EUR)
                .round()
                .unwrap()
                .minor,
            333
        );
        assert_eq!((monthly * dec!(12)).round(), dec!(4000));
    }

    #[test]
    fn only_active_subscriptions_count() {
        let s = Subscription::new(
            SubscriptionId::generate(),
            "Music",
            Currency::EUR,
            date(2026, 1, 1),
            plan(months(1), 1000),
        )
        .unwrap()
        .with_status(date(2026, 3, 1), Status::Paused)
        .unwrap();
        assert!(commitment_on(&s, date(2025, 12, 31)).is_none());
        assert!(commitment_on(&s, date(2026, 2, 28)).is_some());
        assert!(commitment_on(&s, date(2026, 3, 1)).is_none());
    }

    #[test]
    fn uses_the_price_in_effect_on_the_date() {
        let s = Subscription::new(
            SubscriptionId::generate(),
            "Video",
            Currency::EUR,
            date(2026, 1, 1),
            plan(months(1), 1000),
        )
        .unwrap()
        .with_plan(date(2026, 6, 1), plan(months(1), 1300))
        .unwrap();
        let monthly = |d| commitment_on(&s, d).unwrap().unwrap().monthly.minor;
        assert_eq!(monthly(date(2026, 5, 31)), dec!(1000));
        assert_eq!(monthly(date(2026, 6, 1)), dec!(1300));
    }

    #[test]
    fn totals_keep_currencies_apart() {
        let usd: Currency = "USD".parse().unwrap();
        let make = |currency, rule, amount| {
            Subscription::new(
                SubscriptionId::generate(),
                "S",
                currency,
                date(2026, 1, 1),
                plan(rule, amount),
            )
            .unwrap()
        };
        let subs = [
            make(Currency::EUR, months(1), 1000),
            make(Currency::EUR, years(1), 12000),
            make(usd, months(1), 500),
            make(usd, months(1), 700)
                .with_status(date(2026, 2, 1), Status::Cancelled)
                .unwrap(),
        ];
        let totals = total_commitment_on(&subs, date(2026, 6, 1)).unwrap();
        assert_eq!(totals.monthly.get(Currency::EUR).minor, dec!(2000));
        assert_eq!(totals.annual.get(Currency::EUR).minor, dec!(24000));
        assert_eq!(totals.monthly.get(usd).minor, dec!(500));
    }
}
