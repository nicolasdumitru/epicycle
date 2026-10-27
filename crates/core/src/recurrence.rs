//! Billing rules and the payment dates they produce.
//!
//! A [`Schedule`] is a [`Rule`] plus an anchor: one real payment date that fixes
//! the phase (which month an "every 3 months" rule counts from). The `k`-th payment
//! is computed directly from the anchor, never from the previous payment, so dates
//! can't drift: a rule billed on the 31st returns to the 31st after a short month.
//!
//! Calendar rules ([`Rule::Months`], [`Rule::Years`]) keep a fixed day of the month
//! and clamp it to the month's last day when the month is shorter. Duration rules
//! ([`Rule::Days`], [`Rule::Weeks`]) add a fixed number of days.

use std::fmt;
use std::num::NonZeroU16;

use jiff::Unit;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A day of the month, 1–31. Days past the end of a month fall on its last day,
/// so 31 means "the last day of every month".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct DayOfMonth(u8);

impl DayOfMonth {
    pub const LAST: DayOfMonth = DayOfMonth(31);

    pub fn new(day: u8) -> Result<Self, ScheduleError> {
        match day {
            1..=31 => Ok(DayOfMonth(day)),
            _ => Err(ScheduleError::InvalidDay(day)),
        }
    }

    pub fn get(self) -> u8 {
        self.0
    }

    /// This day in the given month, clamped to the month's last day.
    fn in_month(self, year: i16, month: Month) -> Option<Date> {
        let first = Date::new(year, month.0 as i8, 1).ok()?;
        let day = (self.0 as i8).min(first.days_in_month());
        Date::new(year, month.0 as i8, day).ok()
    }
}

impl TryFrom<u8> for DayOfMonth {
    type Error = ScheduleError;

    fn try_from(day: u8) -> Result<Self, Self::Error> {
        DayOfMonth::new(day)
    }
}

impl From<DayOfMonth> for u8 {
    fn from(day: DayOfMonth) -> Self {
        day.0
    }
}

/// A month of the year, 1–12.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct Month(u8);

impl Month {
    pub fn new(month: u8) -> Result<Self, ScheduleError> {
        match month {
            1..=12 => Ok(Month(month)),
            _ => Err(ScheduleError::InvalidMonth(month)),
        }
    }

    pub fn get(self) -> u8 {
        self.0
    }

    fn of(date: Date) -> Month {
        Month(date.month() as u8)
    }

    /// The longest this month can be: 29 for February.
    fn max_days(self) -> u8 {
        match self.0 {
            2 => 29,
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        }
    }
}

impl TryFrom<u8> for Month {
    type Error = ScheduleError;

    fn try_from(month: u8) -> Result<Self, Self::Error> {
        Month::new(month)
    }
}

impl From<Month> for u8 {
    fn from(month: Month) -> Self {
        month.0
    }
}

/// How often a subscription bills.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "unit", rename_all = "snake_case")]
pub enum Rule {
    /// Every `every` days.
    Days { every: NonZeroU16 },
    /// Every `every` weeks, on the anchor's weekday.
    Weeks { every: NonZeroU16 },
    /// Every `every` months on a fixed day, clamped to the month's last day.
    Months { every: NonZeroU16, day: DayOfMonth },
    /// Every `every` years on a fixed date. February 29 falls on February 28 in
    /// years that aren't leap years.
    Years {
        every: NonZeroU16,
        month: Month,
        day: DayOfMonth,
    },
}

impl Rule {
    /// The payment date this rule produces in the anchor's period, if the anchor
    /// itself isn't already one.
    fn expected_anchor(self, anchor: Date) -> Option<Date> {
        match self {
            Rule::Days { .. } | Rule::Weeks { .. } => Some(anchor),
            Rule::Months { day, .. } => day.in_month(anchor.year(), Month::of(anchor)),
            Rule::Years { month, day, .. } => day.in_month(anchor.year(), month),
        }
    }

    fn validate(self) -> Result<(), ScheduleError> {
        match self {
            Rule::Years { month, day, .. } if day.get() > month.max_days() => {
                Err(ScheduleError::DayNotInMonth { month, day })
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ScheduleError {
    #[error("day of month must be between 1 and 31, got {0}")]
    InvalidDay(u8),
    #[error("month must be between 1 and 12, got {0}")]
    InvalidMonth(u8),
    #[error("month {} never has a day {}", month.get(), day.get())]
    DayNotInMonth { month: Month, day: DayOfMonth },
    #[error("{anchor} is not a payment date of this rule (expected {expected})")]
    AnchorNotOnRule { anchor: Date, expected: Date },
    #[error("no payment date of this rule falls within the supported date range")]
    OutOfRange,
}

/// A billing rule anchored to one real payment date.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "UncheckedSchedule")]
pub struct Schedule {
    rule: Rule,
    anchor: Date,
}

/// The serialized form of a [`Schedule`], validated on deserialization.
#[derive(Deserialize)]
struct UncheckedSchedule {
    rule: Rule,
    anchor: Date,
}

impl TryFrom<UncheckedSchedule> for Schedule {
    type Error = ScheduleError;

    fn try_from(raw: UncheckedSchedule) -> Result<Self, Self::Error> {
        Schedule::new(raw.rule, raw.anchor)
    }
}

impl Schedule {
    /// A schedule whose payments include `anchor`.
    ///
    /// Fails if `anchor` isn't a date the rule can produce, e.g. the 15th for a
    /// rule billed on the 31st, or February 28, 2024 for a rule billed on the last
    /// day of the month (2024 is a leap year).
    pub fn new(rule: Rule, anchor: Date) -> Result<Self, ScheduleError> {
        rule.validate()?;
        let expected = rule
            .expected_anchor(anchor)
            .ok_or(ScheduleError::OutOfRange)?;
        if expected != anchor {
            return Err(ScheduleError::AnchorNotOnRule { anchor, expected });
        }
        Ok(Schedule { rule, anchor })
    }

    /// A schedule whose first payment is the earliest date on or after `date`
    /// that fits the rule. For example, a rule billed on the 31st starting
    /// 2026-02-10 has its first payment on 2026-02-28.
    pub fn starting(rule: Rule, date: Date) -> Result<Self, ScheduleError> {
        rule.validate()?;
        // The rule's date in `date`'s own month or year may already have passed,
        // in which case the one in the following month or year is the answer.
        let next_period = match rule {
            Rule::Days { .. } | Rule::Weeks { .. } => None,
            Rule::Months { .. } => add_months(date.first_of_month(), 1),
            Rule::Years { .. } => date
                .year()
                .checked_add(1)
                .and_then(|year| Date::new(year, 1, 1).ok()),
        };
        let anchor = [Some(date), next_period]
            .into_iter()
            .flatten()
            .filter_map(|period| rule.expected_anchor(period))
            .find(|&candidate| candidate >= date)
            .ok_or(ScheduleError::OutOfRange)?;
        Schedule::new(rule, anchor)
    }

    pub fn rule(&self) -> Rule {
        self.rule
    }

    pub fn anchor(&self) -> Date {
        self.anchor
    }

    /// The `k`-th payment date counted from the anchor (`k = 0`), or `None` if it
    /// falls outside the supported date range. Negative `k` gives earlier dates.
    pub fn nth(&self, k: i64) -> Option<Date> {
        match self.rule {
            Rule::Days { every } => add_days(self.anchor, k.checked_mul(i64::from(every.get()))?),
            Rule::Weeks { every } => {
                add_days(self.anchor, k.checked_mul(7 * i64::from(every.get()))?)
            }
            Rule::Months { every, day } => {
                let first = add_months(
                    self.anchor.first_of_month(),
                    k.checked_mul(i64::from(every.get()))?,
                )?;
                day.in_month(first.year(), Month::of(first))
            }
            Rule::Years { every, month, day } => {
                let years = k.checked_mul(i64::from(every.get()))?;
                let year = i16::try_from(i64::from(self.anchor.year()).checked_add(years)?).ok()?;
                day.in_month(year, month)
            }
        }
    }

    /// The index `k` of the first payment on or after `date`.
    fn first_index_on_or_after(&self, date: Date) -> Option<i64> {
        // Estimate an index at or just before the answer, then step forward.
        // Payment dates strictly increase with `k`, so at most a step or two is needed.
        let estimate = match self.rule {
            Rule::Days { every } => {
                days_between(self.anchor, date).div_euclid(i64::from(every.get()))
            }
            Rule::Weeks { every } => {
                days_between(self.anchor, date).div_euclid(7 * i64::from(every.get()))
            }
            Rule::Months { every, .. } => {
                (month_index(date) - month_index(self.anchor)).div_euclid(i64::from(every.get()))
            }
            Rule::Years { every, .. } => {
                i64::from(date.year() - self.anchor.year()).div_euclid(i64::from(every.get()))
            }
        };
        let mut k = estimate;
        while self.nth(k)? < date {
            k += 1;
        }
        Some(k)
    }

    /// The first payment date on or after `date`.
    pub fn next_on_or_after(&self, date: Date) -> Option<Date> {
        self.nth(self.first_index_on_or_after(date)?)
    }

    /// All payment dates in the half-open range `[from, to)`, in order.
    pub fn dates_between(&self, from: Date, to: Date) -> impl Iterator<Item = Date> + '_ {
        let first = if from < to {
            self.first_index_on_or_after(from)
        } else {
            None
        };
        first
            .into_iter()
            .flat_map(|k| k..)
            .map_while(move |k| self.nth(k).filter(|&d| d < to))
    }
}

impl fmt::Debug for Schedule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Schedule({:?} from {})", self.rule, self.anchor)
    }
}

fn add_days(date: Date, days: i64) -> Option<Date> {
    let span = jiff::Span::new().try_days(days).ok()?;
    date.checked_add(span).ok()
}

/// Adds months to the first day of a month (so no clamping is involved).
fn add_months(first_of_month: Date, months: i64) -> Option<Date> {
    let index = month_index(first_of_month).checked_add(months)?;
    let year = i16::try_from(index.div_euclid(12)).ok()?;
    let month = (index.rem_euclid(12) + 1) as i8;
    Date::new(year, month, 1).ok()
}

/// Months since year 0, so that consecutive months differ by one.
fn month_index(date: Date) -> i64 {
    i64::from(date.year()) * 12 + i64::from(date.month()) - 1
}

/// Days from `a` to `b` (negative if `b` is earlier).
fn days_between(a: Date, b: Date) -> i64 {
    a.until((Unit::Day, b))
        .map(|span| i64::from(span.get_days()))
        .expect("the difference between two valid dates is representable in days")
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn every(n: u16) -> NonZeroU16 {
        NonZeroU16::new(n).unwrap()
    }

    fn monthly(n: u16, day: u8) -> Rule {
        Rule::Months {
            every: every(n),
            day: DayOfMonth::new(day).unwrap(),
        }
    }

    fn yearly(n: u16, month: u8, day: u8) -> Rule {
        Rule::Years {
            every: every(n),
            month: Month::new(month).unwrap(),
            day: DayOfMonth::new(day).unwrap(),
        }
    }

    fn dates(schedule: &Schedule, from: Date, to: Date) -> Vec<Date> {
        schedule.dates_between(from, to).collect()
    }

    #[test]
    fn monthly_on_the_31st_clamps_and_returns_to_the_31st() {
        let s = Schedule::new(monthly(1, 31), date(2024, 1, 31)).unwrap();
        assert_eq!(
            dates(&s, date(2024, 1, 1), date(2024, 6, 1)),
            [
                date(2024, 1, 31),
                date(2024, 2, 29),
                date(2024, 3, 31),
                date(2024, 4, 30),
                date(2024, 5, 31)
            ]
        );
        // February outside a leap year.
        assert_eq!(s.nth(13), Some(date(2025, 2, 28)));
    }

    #[test]
    fn monthly_on_the_30th_clamps_only_in_february() {
        let s = Schedule::new(monthly(1, 30), date(2025, 1, 30)).unwrap();
        assert_eq!(
            dates(&s, date(2025, 1, 1), date(2025, 4, 1)),
            [date(2025, 1, 30), date(2025, 2, 28), date(2025, 3, 30)]
        );
    }

    #[test]
    fn every_three_months_keeps_its_phase() {
        let s = Schedule::new(monthly(3, 15), date(2026, 2, 15)).unwrap();
        assert_eq!(
            dates(&s, date(2026, 1, 1), date(2027, 1, 1)),
            [
                date(2026, 2, 15),
                date(2026, 5, 15),
                date(2026, 8, 15),
                date(2026, 11, 15)
            ]
        );
    }

    #[test]
    fn payments_before_the_anchor_follow_the_same_rule() {
        let s = Schedule::new(monthly(1, 31), date(2026, 3, 31)).unwrap();
        assert_eq!(s.nth(-1), Some(date(2026, 2, 28)));
        assert_eq!(s.nth(-2), Some(date(2026, 1, 31)));
        assert_eq!(s.nth(-12), Some(date(2025, 3, 31)));
    }

    #[test]
    fn yearly_on_february_29() {
        let s = Schedule::new(yearly(1, 2, 29), date(2024, 2, 29)).unwrap();
        assert_eq!(
            dates(&s, date(2024, 1, 1), date(2029, 1, 1)),
            [
                date(2024, 2, 29),
                date(2025, 2, 28),
                date(2026, 2, 28),
                date(2027, 2, 28),
                date(2028, 2, 29)
            ]
        );
    }

    #[test]
    fn every_two_years() {
        let s = Schedule::new(yearly(2, 9, 1), date(2025, 9, 1)).unwrap();
        assert_eq!(
            dates(&s, date(2020, 1, 1), date(2030, 1, 1)),
            [
                date(2021, 9, 1),
                date(2023, 9, 1),
                date(2025, 9, 1),
                date(2027, 9, 1),
                date(2029, 9, 1)
            ]
        );
    }

    #[test]
    fn every_two_weeks_keeps_the_weekday() {
        // 2026-10-30 is a Friday.
        let s = Schedule::new(Rule::Weeks { every: every(2) }, date(2026, 10, 30)).unwrap();
        let ds = dates(&s, date(2026, 10, 30), date(2027, 1, 1));
        assert_eq!(
            ds,
            [
                date(2026, 10, 30),
                date(2026, 11, 13),
                date(2026, 11, 27),
                date(2026, 12, 11),
                date(2026, 12, 25)
            ]
        );
        assert!(
            ds.iter()
                .all(|d| d.weekday() == jiff::civil::Weekday::Friday)
        );
    }

    #[test]
    fn every_30_days_ignores_month_boundaries() {
        let s = Schedule::new(Rule::Days { every: every(30) }, date(2024, 1, 15)).unwrap();
        assert_eq!(
            dates(&s, date(2024, 1, 1), date(2024, 5, 1)),
            [
                date(2024, 1, 15),
                date(2024, 2, 14),
                date(2024, 3, 15),
                date(2024, 4, 14)
            ]
        );
    }

    #[test]
    fn weekly_payments_in_a_year_can_be_53() {
        // 2026 starts on a Thursday and isn't a leap year: 53 Thursdays.
        let s = Schedule::new(Rule::Weeks { every: every(1) }, date(2026, 1, 1)).unwrap();
        assert_eq!(
            s.dates_between(date(2026, 1, 1), date(2027, 1, 1)).count(),
            53
        );
    }

    #[test]
    fn range_is_half_open() {
        let s = Schedule::new(monthly(1, 1), date(2026, 1, 1)).unwrap();
        assert_eq!(
            dates(&s, date(2026, 1, 1), date(2026, 3, 1)),
            [date(2026, 1, 1), date(2026, 2, 1)]
        );
        assert_eq!(dates(&s, date(2026, 3, 1), date(2026, 3, 1)), []);
        assert_eq!(dates(&s, date(2026, 4, 1), date(2026, 3, 1)), []);
    }

    #[test]
    fn next_payment_on_or_after() {
        let s = Schedule::new(monthly(1, 31), date(2026, 1, 31)).unwrap();
        assert_eq!(
            s.next_on_or_after(date(2026, 1, 31)),
            Some(date(2026, 1, 31))
        );
        assert_eq!(
            s.next_on_or_after(date(2026, 2, 1)),
            Some(date(2026, 2, 28))
        );
        assert_eq!(
            s.next_on_or_after(date(2026, 3, 1)),
            Some(date(2026, 3, 31))
        );
        assert_eq!(
            s.next_on_or_after(date(2025, 12, 31)),
            Some(date(2025, 12, 31))
        );
    }

    #[test]
    fn rejects_anchors_that_are_not_payment_dates() {
        assert_eq!(
            Schedule::new(monthly(1, 31), date(2026, 1, 15)),
            Err(ScheduleError::AnchorNotOnRule {
                anchor: date(2026, 1, 15),
                expected: date(2026, 1, 31)
            })
        );
        // Feb 28 is the last day in 2025, but not in 2024.
        assert!(Schedule::new(monthly(1, 31), date(2025, 2, 28)).is_ok());
        assert!(Schedule::new(monthly(1, 31), date(2024, 2, 28)).is_err());
        assert!(Schedule::new(yearly(1, 3, 1), date(2026, 4, 1)).is_err());
    }

    #[test]
    fn rejects_yearly_dates_that_never_exist() {
        assert!(matches!(
            Schedule::new(yearly(1, 2, 30), date(2026, 2, 28)),
            Err(ScheduleError::DayNotInMonth { .. })
        ));
        assert!(Schedule::new(yearly(1, 4, 31), date(2026, 4, 30)).is_err());
        assert!(DayOfMonth::new(0).is_err());
        assert!(DayOfMonth::new(32).is_err());
        assert!(Month::new(13).is_err());
    }

    #[test]
    fn starting_picks_the_first_fitting_date() {
        let s = Schedule::starting(monthly(1, 31), date(2026, 2, 10)).unwrap();
        assert_eq!(s.anchor(), date(2026, 2, 28));
        let s = Schedule::starting(monthly(1, 5), date(2026, 2, 10)).unwrap();
        assert_eq!(s.anchor(), date(2026, 3, 5));
        let s = Schedule::starting(monthly(1, 5), date(2026, 12, 10)).unwrap();
        assert_eq!(s.anchor(), date(2027, 1, 5));
        let s = Schedule::starting(yearly(1, 2, 29), date(2025, 3, 1)).unwrap();
        assert_eq!(s.anchor(), date(2026, 2, 28));
        let s = Schedule::starting(Rule::Days { every: every(10) }, date(2026, 2, 10)).unwrap();
        assert_eq!(s.anchor(), date(2026, 2, 10));
    }

    #[test]
    fn serializes_to_readable_json_and_validates_on_load() {
        let s = Schedule::new(monthly(1, 31), date(2026, 1, 31)).unwrap();
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(
            json,
            r#"{"rule":{"unit":"months","every":1,"day":31},"anchor":"2026-01-31"}"#
        );
        assert_eq!(serde_json::from_str::<Schedule>(&json).unwrap(), s);

        let bad_anchor = r#"{"rule":{"unit":"months","every":1,"day":31},"anchor":"2026-01-15"}"#;
        assert!(serde_json::from_str::<Schedule>(bad_anchor).is_err());
        let zero_interval = r#"{"rule":{"unit":"days","every":0},"anchor":"2026-01-15"}"#;
        assert!(serde_json::from_str::<Schedule>(zero_interval).is_err());
    }

    /// Exhaustive check over every day-of-month, interval and anchor month in a
    /// span of years that includes leap years: payments land exactly `every`
    /// months apart, on the requested day clamped to the month's length.
    #[test]
    fn monthly_rules_hold_for_every_day_and_interval() {
        for day in 1..=31 {
            for n in [1, 2, 3, 6, 12] {
                for anchor_month in 1..=12 {
                    let rule = monthly(n, day);
                    let anchor = Schedule::starting(rule, date(2023, anchor_month, 1))
                        .unwrap()
                        .anchor();
                    let s = Schedule::new(rule, anchor).unwrap();
                    let ds = dates(&s, date(2020, 1, 1), date(2030, 1, 1));
                    for pair in ds.windows(2) {
                        assert_eq!(month_index(pair[1]) - month_index(pair[0]), i64::from(n));
                    }
                    for d in &ds {
                        assert_eq!(
                            d.day(),
                            (day as i8).min(d.days_in_month()),
                            "{d} for day {day}"
                        );
                    }
                    // Starting the range on any payment date includes it.
                    for d in &ds {
                        assert_eq!(s.next_on_or_after(*d), Some(*d));
                    }
                }
            }
        }
    }

    /// For every start date in a leap and a non-leap year, the next payment is
    /// never before the start date and no payment is skipped.
    #[test]
    fn next_payment_never_skips_a_payment() {
        let rules = [
            monthly(1, 31),
            monthly(2, 29),
            yearly(1, 2, 29),
            Rule::Days { every: every(30) },
            Rule::Weeks { every: every(3) },
        ];
        for rule in rules {
            let s = Schedule::starting(rule, date(2023, 6, 15)).unwrap();
            let all = dates(&s, date(2022, 1, 1), date(2026, 1, 1));
            let mut day = date(2023, 1, 1);
            while day < date(2025, 1, 1) {
                let expected = all.iter().copied().find(|&d| d >= day);
                assert_eq!(s.next_on_or_after(day), expected, "{rule:?} on {day}");
                day = day.tomorrow().unwrap();
            }
        }
    }
}
