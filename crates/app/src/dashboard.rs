//! The dashboard's views of scheduled cash flow: upcoming payments, and a
//! month-by-month chart that keeps cash flow and commitment apart.
//!
//! Cash flow is what is actually paid in a month (an annual plan pays its whole
//! price in one month). Commitment is the monthly equivalent of what is active,
//! the same in every month while nothing changes. The chart shows both so the
//! difference is visible, never one standing in for the other.

use epicycle_core::cashflow::{monthly_totals, payments_between};
use epicycle_core::commitment::total_commitment_on;
use epicycle_core::fx::{self, RateTable};
use epicycle_core::money::{Currency, MultiCurrency};
use epicycle_core::subscription::Subscription;
use jiff::ToSpan;
use jiff::civil::Date;
use serde::Serialize;
use tauri::State;
use ts_rs::TS;

use crate::commands::{CommandError, MoneyView, currency_in_use, rounded, today};
use crate::db::DbHandle;

/// One payment due soon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpcomingPayment {
    pub date: String,
    pub subscription_id: String,
    pub name: String,
    /// In the subscription's own currency.
    pub amount: MoneyView,
}

/// Payments due from `today` for `days` days, in date order.
pub fn upcoming_of(subscriptions: &[Subscription], today: Date, days: i32) -> Vec<UpcomingPayment> {
    let end = today.checked_add(days.days()).unwrap_or(Date::MAX);
    payments_between(subscriptions, today, end)
        .into_iter()
        .filter_map(|p| {
            let s = subscriptions.iter().find(|s| s.id() == p.subscription)?;
            Some(UpcomingPayment {
                date: p.date.to_string(),
                subscription_id: p.subscription.to_string(),
                name: s.name().to_owned(),
                amount: p.amount.into(),
            })
        })
        .collect()
}

/// One month of the chart, converted to one currency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MonthView {
    /// First day of the month, ISO.
    pub month: String,
    /// What is paid in the month.
    pub cash_flow: MoneyView,
    /// The monthly equivalent of what is active at the start of the month
    /// (today, for the current month).
    pub commitment: MoneyView,
    /// Before the current month: estimated from the recorded plans, not
    /// verified payments.
    pub estimated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MonthlyChart {
    pub currency: String,
    /// Oldest first; empty when a rate is missing.
    pub months: Vec<MonthView>,
    /// Currencies with no exchange rate at all.
    pub missing: Vec<String>,
}

/// Months shown before and after the current one.
const MONTHS_BEFORE: i32 = 6;
const MONTHS_AFTER: i32 = 6;

fn convert(
    rates: &RateTable,
    total: &MultiCurrency,
    to: Currency,
    date: Date,
) -> Result<MoneyView, CommandError> {
    rounded(fx::convert_total(rates, total, to, date)?.amount)
}

/// Cash flow and commitment for each month around `today`, in `to`. Each
/// month is converted with the rates for its own date, as near as the cached
/// rates allow.
pub fn monthly_chart_of(
    subscriptions: &[Subscription],
    rates: &RateTable,
    to: Currency,
    today: Date,
) -> Result<MonthlyChart, CommandError> {
    let this_month = today.first_of_month();
    let from = this_month
        .checked_sub(MONTHS_BEFORE.months())
        .unwrap_or(Date::MIN);
    let end = this_month
        .checked_add((MONTHS_AFTER + 1).months())
        .unwrap_or(Date::MAX);
    let cash_flow = monthly_totals(subscriptions, from, end)?;
    let commitments = cash_flow
        .iter()
        .map(|m| {
            let on = if m.month == this_month {
                today
            } else {
                m.month
            };
            Ok((on, total_commitment_on(subscriptions, on)?.monthly))
        })
        .collect::<Result<Vec<_>, CommandError>>()?;

    let mut currencies: Vec<Currency> = cash_flow
        .iter()
        .flat_map(|m| m.total.iter())
        .chain(commitments.iter().flat_map(|(_, c)| c.iter()))
        .map(|f| f.currency)
        .chain([to])
        .collect();
    currencies.sort();
    currencies.dedup();
    let missing: Vec<String> = currencies
        .iter()
        .filter(|&&c| c != Currency::EUR && rates.rate_for(c, today).is_none())
        .map(|c| c.code().to_owned())
        .collect();
    if !missing.is_empty() {
        return Ok(MonthlyChart {
            currency: to.code().to_owned(),
            months: Vec::new(),
            missing,
        });
    }

    let months = cash_flow
        .iter()
        .zip(&commitments)
        .map(|(m, (on, commitment))| {
            Ok(MonthView {
                month: m.month.to_string(),
                cash_flow: convert(rates, &m.total, to, m.month)?,
                commitment: convert(rates, commitment, to, *on)?,
                estimated: m.month < this_month,
            })
        })
        .collect::<Result<_, CommandError>>()?;
    Ok(MonthlyChart {
        currency: to.code().to_owned(),
        months,
        missing,
    })
}

/// Payments due in the next `days` days (at most a year).
#[tauri::command]
pub async fn upcoming_payments(
    db: State<'_, DbHandle>,
    days: u16,
) -> Result<Vec<UpcomingPayment>, CommandError> {
    let db = db.ready().await;
    Ok(upcoming_of(
        &db.subscriptions().await?,
        today(),
        i32::from(days.min(366)),
    ))
}

/// The monthly chart in `currency`, or the reporting currency if none.
#[tauri::command]
pub async fn monthly_chart(
    db: State<'_, DbHandle>,
    currency: Option<String>,
) -> Result<MonthlyChart, CommandError> {
    let db = db.ready().await;
    let to = match currency.as_deref() {
        Some(code) => currency_in_use(code)?,
        None => db.reporting_currency().await?,
    };
    monthly_chart_of(&db.subscriptions().await?, &db.rates().await?, to, today())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{NewSubscription, Unit, new_subscription};
    use epicycle_core::fx::{Rate, RateSource};
    use jiff::civil::date;

    fn sub(
        name: &str,
        amount: &str,
        currency: &str,
        every: u16,
        unit: Unit,
        next: &str,
    ) -> Subscription {
        new_subscription(
            &NewSubscription {
                name: name.into(),
                amount: amount.into(),
                currency: currency.into(),
                every,
                unit,
                next_payment: next.into(),
                since: Some("2025-01-01".into()),
            },
            date(2026, 11, 3),
        )
        .unwrap()
    }

    #[test]
    fn lists_upcoming_payments_in_date_order() {
        let subs = [
            sub("Music", "10", "EUR", 1, Unit::Months, "2026-11-15"),
            sub("Cloud", "5", "EUR", 1, Unit::Months, "2026-11-05"),
        ];
        let upcoming = upcoming_of(&subs, date(2026, 11, 3), 30);
        let seen: Vec<(&str, &str)> = upcoming
            .iter()
            .map(|p| (p.date.as_str(), p.name.as_str()))
            .collect();
        assert_eq!(seen, [("2026-11-05", "Cloud"), ("2026-11-15", "Music")]);
    }

    #[test]
    fn separates_cash_flow_from_commitment() {
        // A yearly plan: 120 paid in March, 10 a month of commitment.
        let subs = [sub("News", "120", "EUR", 1, Unit::Years, "2027-03-10")];
        let rates = RateTable::new([]);
        let chart = monthly_chart_of(&subs, &rates, Currency::EUR, date(2026, 11, 3)).unwrap();
        assert_eq!(chart.months.len(), 13);
        assert_eq!(chart.months.first().unwrap().month, "2026-05-01");
        let march = chart
            .months
            .iter()
            .find(|m| m.month == "2027-03-01")
            .unwrap();
        assert_eq!(march.cash_flow.amount, "120.00");
        assert_eq!(march.commitment.amount, "10.00");
        let april = chart
            .months
            .iter()
            .find(|m| m.month == "2027-04-01")
            .unwrap();
        assert_eq!(april.cash_flow.amount, "0.00");
        assert_eq!(april.commitment.amount, "10.00");
        assert!(chart.months[0].estimated);
        assert!(!chart.months[6].estimated);
    }

    #[test]
    fn converts_and_names_missing_rates() {
        let subs = [sub("Music", "53.40", "RON", 1, Unit::Months, "2026-11-15")];
        let ron = Rate::new(
            "RON".parse().unwrap(),
            date(2026, 11, 2),
            "5.34".parse().unwrap(),
            RateSource::Ecb {
                fetched_at: jiff::Timestamp::UNIX_EPOCH,
            },
        )
        .unwrap();
        let chart = monthly_chart_of(
            &subs,
            &RateTable::new([ron]),
            Currency::EUR,
            date(2026, 11, 3),
        )
        .unwrap();
        let november = &chart.months[6];
        assert_eq!(november.cash_flow.amount, "10.00");
        assert_eq!(november.commitment.amount, "10.00");

        let none =
            monthly_chart_of(&subs, &RateTable::new([]), Currency::EUR, date(2026, 11, 3)).unwrap();
        assert!(none.months.is_empty());
        assert_eq!(none.missing, ["RON"]);
    }
}
