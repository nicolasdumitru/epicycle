//! Reminders: what to tell the user ahead of time, and when.
//!
//! Pure: from the subscriptions, the reminder settings and the current time,
//! [`reminders`] lists the reminders due within a window. The app keeps the
//! platform's notification schedule in step with that list (see
//! docs/DECISIONS.md, "Reminders").

use jiff::ToSpan;
use jiff::civil::{Date, DateTime, Time};
use serde::{Deserialize, Serialize};

use crate::money::Money;
use crate::subscription::{Subscription, SubscriptionId};

/// When to remind: how many days before each event, and at what time of day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReminderSettings {
    pub enabled: bool,
    /// Days before the event, each giving one reminder; sorted, no duplicates.
    pub lead_days: Vec<u16>,
    /// Local time of day the reminders go out.
    pub time_of_day: Time,
}

impl Default for ReminderSettings {
    /// One reminder a day ahead, at 09:00.
    fn default() -> Self {
        ReminderSettings {
            enabled: true,
            lead_days: vec![1],
            time_of_day: Time::constant(9, 0, 0, 0),
        }
    }
}

impl ReminderSettings {
    /// The same settings with lead times sorted and deduplicated.
    pub fn normalized(mut self) -> Self {
        self.lead_days.sort_unstable();
        self.lead_days.dedup();
        self
    }
}

/// What a reminder is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReminderKind {
    /// A payment falls due.
    PaymentDue,
    /// A bill is issued, on a different day from when it's due.
    Billed,
    /// A free period (such as a trial) ends and paying starts.
    TrialEnds,
    /// A new price or billing schedule takes effect.
    PriceChange,
}

/// One reminder to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reminder {
    pub subscription: SubscriptionId,
    pub name: String,
    pub kind: ReminderKind,
    /// The day of the event.
    pub date: Date,
    /// How many days ahead of the event this reminder is.
    pub lead_days: u16,
    /// When to show it, in local time.
    pub at: DateTime,
    /// The payment's amount, or the new price after a trial or price change.
    pub amount: Money,
}

impl Reminder {
    /// A stable, positive ID for the platform's notification, derived from
    /// what the reminder is about, so rescheduling never duplicates one.
    pub fn id(&self) -> i32 {
        // FNV-1a, 32 bits: small, deterministic across runs and platforms.
        let mut hash: u32 = 0x811c_9dc5;
        let kind = match self.kind {
            ReminderKind::PaymentDue => 1u8,
            ReminderKind::Billed => 2,
            ReminderKind::TrialEnds => 3,
            ReminderKind::PriceChange => 4,
        };
        let bytes = self
            .subscription
            .0
            .as_bytes()
            .iter()
            .copied()
            .chain([kind])
            .chain(self.date.to_string().into_bytes())
            .chain(self.lead_days.to_le_bytes());
        for b in bytes {
            hash ^= u32::from(b);
            hash = hash.wrapping_mul(0x0100_0193);
        }
        // Positive, so it is valid on every platform.
        (hash & 0x7fff_ffff) as i32
    }
}

/// The reminders to show after `now` and up to `days` days ahead, soonest
/// first, at most `limit` of them. Payments of 0 (skipped or free) get none.
pub fn reminders(
    subscriptions: &[Subscription],
    settings: &ReminderSettings,
    now: DateTime,
    days: i32,
    limit: usize,
) -> Vec<Reminder> {
    if !settings.enabled || settings.lead_days.is_empty() {
        return Vec::new();
    }
    let today = now.date();
    let window_end = now.checked_add(days.days()).unwrap_or(DateTime::MAX);
    let longest_lead = settings.lead_days.iter().copied().max().unwrap_or(0);
    // Events up to the window's end plus the longest lead can be reminded of
    // within the window.
    let events_to = window_end
        .date()
        .checked_add(i32::from(longest_lead).days())
        .unwrap_or(Date::MAX);

    let mut out: Vec<Reminder> = subscriptions
        .iter()
        .flat_map(|s| events(s, today, events_to))
        .flat_map(|(s, kind, date, amount)| {
            settings.lead_days.iter().filter_map(move |&lead| {
                let day = date.checked_sub(i32::from(lead).days()).ok()?;
                let at = day.to_datetime(settings.time_of_day);
                (now < at && at <= window_end).then(|| Reminder {
                    subscription: s.id(),
                    name: s.name().to_owned(),
                    kind,
                    date,
                    lead_days: lead,
                    at,
                    amount,
                })
            })
        })
        .collect();
    out.sort_by(|a, b| a.at.cmp(&b.at).then(a.id().cmp(&b.id())));
    out.truncate(limit);
    out
}

/// A subscription's events in `[from, to)`: payments due, separate billing
/// days, trials ending and price changes.
fn events(
    s: &Subscription,
    from: Date,
    to: Date,
) -> impl Iterator<Item = (&Subscription, ReminderKind, Date, Money)> {
    let payments = s
        .payments_between(from, to)
        .filter(|p| p.amount.minor != 0)
        .flat_map(move |p| {
            let due = (s, ReminderKind::PaymentDue, p.date, p.amount);
            let billed = (p.billed != p.date && from <= p.billed && p.billed < to).then_some((
                s,
                ReminderKind::Billed,
                p.billed,
                p.amount,
            ));
            std::iter::once(due).chain(billed)
        });
    let changes = s.plans().windows(2).filter_map(move |w| {
        let (before, after) = (&w[0].value, &w[1].value);
        let date = w[1].from;
        if !(from <= date && date < to) || after.amount == 0 {
            return None;
        }
        let kind = if before.amount == 0 {
            ReminderKind::TrialEnds
        } else if before.amount != after.amount {
            ReminderKind::PriceChange
        } else {
            return None;
        };
        Some((s, kind, date, Money::new(after.amount, s.currency())))
    });
    payments.chain(changes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recurrence::{DayOfMonth, Due};
    use crate::subscription::tests::{monthly_plan, subscription};
    use jiff::civil::date;

    fn kinds(rs: &[Reminder]) -> Vec<(ReminderKind, Date, u16, DateTime)> {
        rs.iter()
            .map(|r| (r.kind, r.date, r.lead_days, r.at))
            .collect()
    }

    fn now() -> DateTime {
        date(2026, 11, 3).at(12, 0, 0, 0)
    }

    #[test]
    fn reminds_a_day_before_each_payment_by_default() {
        let s = subscription(date(2026, 1, 15), monthly_plan(date(2026, 1, 15), 1000));
        let rs = reminders(&[s], &ReminderSettings::default(), now(), 45, 60);
        assert_eq!(
            kinds(&rs),
            [
                (
                    ReminderKind::PaymentDue,
                    date(2026, 11, 15),
                    1,
                    date(2026, 11, 14).at(9, 0, 0, 0)
                ),
                (
                    ReminderKind::PaymentDue,
                    date(2026, 12, 15),
                    1,
                    date(2026, 12, 14).at(9, 0, 0, 0)
                ),
            ]
        );
        assert_eq!(rs[0].amount.minor, 1000);
    }

    #[test]
    fn several_lead_times_give_several_reminders_and_skip_the_past() {
        let s = subscription(date(2026, 1, 5), monthly_plan(date(2026, 1, 5), 1000));
        let settings = ReminderSettings {
            lead_days: vec![1, 3],
            ..ReminderSettings::default()
        };
        let rs = reminders(&[s], &settings, now(), 10, 60);
        // November 5th: 3 days before (Nov 2) is past; 1 day before isn't.
        assert_eq!(
            kinds(&rs),
            [(
                ReminderKind::PaymentDue,
                date(2026, 11, 5),
                1,
                date(2026, 11, 4).at(9, 0, 0, 0)
            )]
        );
    }

    #[test]
    fn reminds_of_bills_trials_and_price_changes() {
        let billed_6th = monthly_plan(date(2026, 1, 6), 3000).due(Due::DayOfMonth {
            day: DayOfMonth::new(20).unwrap(),
        });
        let vodafone = subscription(date(2026, 1, 6), billed_6th);
        let trial = subscription(date(2026, 11, 1), monthly_plan(date(2026, 11, 1), 2000))
            .with_free_period(date(2026, 11, 1), date(2026, 11, 20))
            .unwrap();
        let rising = subscription(date(2026, 1, 25), monthly_plan(date(2026, 1, 25), 1000))
            .with_plan(date(2026, 11, 25), monthly_plan(date(2026, 1, 25), 1200))
            .unwrap();
        let rs = reminders(
            &[vodafone, trial, rising],
            &ReminderSettings::default(),
            now(),
            // To November 28th: the price change's reminder is on the 24th.
            25,
            60,
        );
        let got: Vec<(ReminderKind, Date)> = rs.iter().map(|r| (r.kind, r.date)).collect();
        assert!(got.contains(&(ReminderKind::Billed, date(2026, 11, 6))));
        assert!(got.contains(&(ReminderKind::PaymentDue, date(2026, 11, 20))));
        assert!(got.contains(&(ReminderKind::TrialEnds, date(2026, 11, 20))));
        assert!(got.contains(&(ReminderKind::PriceChange, date(2026, 11, 25))));
        // Soonest first.
        assert!(rs.windows(2).all(|w| w[0].at <= w[1].at));
    }

    #[test]
    fn ids_are_stable_and_distinct() {
        let s = subscription(date(2026, 1, 15), monthly_plan(date(2026, 1, 15), 1000));
        let settings = ReminderSettings {
            lead_days: vec![1, 3],
            ..ReminderSettings::default()
        };
        let a = reminders(std::slice::from_ref(&s), &settings, now(), 60, 60);
        let b = reminders(&[s], &settings, now(), 60, 60);
        let ids: Vec<i32> = a.iter().map(Reminder::id).collect();
        assert_eq!(ids, b.iter().map(Reminder::id).collect::<Vec<_>>());
        let mut unique = ids.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), ids.len());
        assert!(ids.iter().all(|&id| id > 0));
    }

    #[test]
    fn nothing_when_disabled_and_at_most_the_limit() {
        let s = subscription(date(2026, 1, 15), monthly_plan(date(2026, 1, 15), 1000));
        let off = ReminderSettings {
            enabled: false,
            ..ReminderSettings::default()
        };
        assert!(reminders(std::slice::from_ref(&s), &off, now(), 365, 60).is_empty());
        assert_eq!(
            reminders(&[s], &ReminderSettings::default(), now(), 365, 5).len(),
            5
        );
    }
}
