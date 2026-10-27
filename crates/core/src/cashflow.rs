//! Scheduled cash flow: when payments actually happen and how much they are.
//!
//! The same calculation gives upcoming payments (dates in the future) and
//! estimated historical spending (dates in the past). Past payments are
//! estimates derived from the recorded plans, never verified transactions.

use jiff::ToSpan;
use jiff::civil::Date;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::money::{MoneyError, MultiCurrency, sum_by_currency};
use crate::subscription::{Payment, Subscription};

/// All payments of the given subscriptions in `[from, to)`, ordered by date.
pub fn payments_between<'a>(
    subscriptions: impl IntoIterator<Item = &'a Subscription>,
    from: Date,
    to: Date,
) -> Vec<Payment> {
    let mut payments: Vec<Payment> = subscriptions
        .into_iter()
        .flat_map(|s| s.payments_between(from, to))
        .collect();
    payments.sort_by_key(|p| (p.date, p.subscription));
    payments
}

/// Total paid in `[from, to)`, per currency.
pub fn total_between<'a>(
    subscriptions: impl IntoIterator<Item = &'a Subscription>,
    from: Date,
    to: Date,
) -> Result<MultiCurrency, MoneyError> {
    sum_by_currency(
        subscriptions
            .into_iter()
            .flat_map(|s| s.payments_between(from, to))
            .map(|p| p.amount),
    )
}

/// The total paid in one calendar month.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonthTotal {
    /// First day of the month.
    pub month: Date,
    pub total: MultiCurrency,
}

/// Totals for each calendar month from the month containing `from` up to, but
/// not including, the month containing `to`. Months without payments are
/// included with an empty total, so the result can be charted directly.
pub fn monthly_totals<'a>(
    subscriptions: impl IntoIterator<Item = &'a Subscription> + Clone,
    from: Date,
    to: Date,
) -> Result<Vec<MonthTotal>, MoneyError> {
    month_starts(from, to)
        .map(|month| {
            let next = month.checked_add(1.month()).unwrap_or(Date::MAX);
            let total = total_between(subscriptions.clone(), month, next)?;
            Ok(MonthTotal { month, total })
        })
        .collect()
}

/// Average monthly spending over the calendar months from the month containing
/// `from` up to, but not including, the month containing `to`. Empty if the
/// range has no whole month.
pub fn average_per_month<'a>(
    subscriptions: impl IntoIterator<Item = &'a Subscription> + Clone,
    from: Date,
    to: Date,
) -> Result<MultiCurrency, MoneyError> {
    let months = month_starts(from, to).count();
    if months == 0 {
        return Ok(MultiCurrency::new());
    }
    let first = from.first_of_month();
    let end = to.first_of_month();
    total_between(subscriptions, first, end)?
        .checked_scale(Decimal::ONE, Decimal::from(months))
        .ok_or(MoneyError::Overflow)
}

/// The first day of each month from `from`'s month up to `to`'s month (exclusive).
fn month_starts(from: Date, to: Date) -> impl Iterator<Item = Date> {
    let end = to.first_of_month();
    std::iter::successors(Some(from.first_of_month()), |m| {
        m.checked_add(1.month()).ok()
    })
    .take_while(move |&m| m < end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Currency;
    use crate::recurrence::{DayOfMonth, Month, Rule, Schedule};
    use crate::subscription::tests::{monthly_plan, subscription};
    use crate::subscription::{Plan, Status, SubscriptionId};
    use jiff::civil::date;
    use rust_decimal_macros::dec;
    use std::num::NonZeroU16;

    fn yearly_plan(anchor: Date, amount: i64) -> Plan {
        let rule = Rule::Years {
            every: NonZeroU16::MIN,
            month: Month::new(anchor.month() as u8).unwrap(),
            day: DayOfMonth::new(anchor.day() as u8).unwrap(),
        };
        Plan {
            schedule: Schedule::new(rule, anchor).unwrap(),
            amount,
        }
    }

    fn eur(total: &MultiCurrency) -> Decimal {
        total.get(Currency::EUR).minor
    }

    #[test]
    fn annual_subscription_pays_once_a_year() {
        // EUR 120 a year: 120 in the renewal month and 0 in the other eleven.
        let s = subscription(date(2026, 3, 15), yearly_plan(date(2026, 3, 15), 12000));
        let months = monthly_totals([&s], date(2026, 1, 1), date(2027, 1, 1)).unwrap();
        assert_eq!(months.len(), 12);
        let amounts: Vec<Decimal> = months.iter().map(|m| eur(&m.total)).collect();
        let mut expected = vec![dec!(0); 12];
        expected[2] = dec!(12000);
        assert_eq!(amounts, expected);
        assert_eq!(months[2].month, date(2026, 3, 1));
    }

    #[test]
    fn totals_over_arbitrary_periods() {
        let s = subscription(date(2026, 1, 10), monthly_plan(date(2026, 1, 10), 999));
        assert_eq!(
            eur(&total_between([&s], date(2026, 1, 1), date(2026, 4, 1)).unwrap()),
            dec!(2997)
        );
        // Half-open: a payment on the end date isn't counted.
        assert_eq!(
            eur(&total_between([&s], date(2026, 1, 10), date(2026, 2, 10)).unwrap()),
            dec!(999)
        );
        assert_eq!(
            eur(&total_between([&s], date(2026, 1, 11), date(2026, 2, 10)).unwrap()),
            dec!(0)
        );
    }

    #[test]
    fn average_spreads_an_annual_payment_over_the_months() {
        let s = subscription(date(2026, 3, 15), yearly_plan(date(2026, 3, 15), 12000));
        let avg = average_per_month([&s], date(2026, 1, 1), date(2027, 1, 1)).unwrap();
        assert_eq!(eur(&avg), dec!(1000));
        // Over a quarter without the renewal, the average is zero.
        let avg = average_per_month([&s], date(2026, 4, 1), date(2026, 7, 1)).unwrap();
        assert_eq!(eur(&avg), dec!(0));
        assert!(
            average_per_month([&s], date(2026, 4, 1), date(2026, 4, 20))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn historical_estimates_use_the_price_at_the_time() {
        let s = subscription(date(2025, 1, 1), monthly_plan(date(2025, 1, 1), 1000))
            .with_plan(date(2025, 7, 1), monthly_plan(date(2025, 1, 1), 1200))
            .unwrap();
        let total = total_between([&s], date(2025, 1, 1), date(2026, 1, 1)).unwrap();
        assert_eq!(eur(&total), dec!(6000) + dec!(7200));
    }

    #[test]
    fn payments_are_merged_in_date_order() {
        let a = subscription(date(2026, 1, 20), monthly_plan(date(2026, 1, 20), 100));
        let b = subscription(date(2026, 1, 5), monthly_plan(date(2026, 1, 5), 200))
            .with_status(date(2026, 2, 1), Status::Cancelled)
            .unwrap();
        let dates: Vec<Date> = payments_between([&a, &b], date(2026, 1, 1), date(2026, 3, 1))
            .iter()
            .map(|p| p.date)
            .collect();
        assert_eq!(
            dates,
            [date(2026, 1, 5), date(2026, 1, 20), date(2026, 2, 20)]
        );
    }

    #[test]
    fn currencies_are_never_summed_together() {
        let usd: Currency = "USD".parse().unwrap();
        let a = subscription(date(2026, 1, 1), monthly_plan(date(2026, 1, 1), 1000));
        let b = Subscription::new(
            SubscriptionId::generate(),
            "US service",
            usd,
            date(2026, 1, 1),
            monthly_plan(date(2026, 1, 1), 500),
        )
        .unwrap();
        let total = total_between([&a, &b], date(2026, 1, 1), date(2026, 2, 1)).unwrap();
        assert_eq!(eur(&total), dec!(1000));
        assert_eq!(total.get(usd).minor, dec!(500));
    }
}
