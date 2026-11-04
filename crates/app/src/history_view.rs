//! A subscription's change history as the UI shows it: each revision with what
//! it changed compared with the version it replaced.
//!
//! Changes are structured data rather than sentences, so the UI formats dates
//! and amounts in the device's locale.

use std::collections::BTreeMap;

use epicycle_core::history::{Cause, History, Revision};
use epicycle_core::money::Money;
use epicycle_core::subscription::{Dated, Plan, Status, Subscription};
use jiff::civil::Date;
use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

use crate::commands::{Billing, DueChoice, EstimateView, MoneyView, StatusChoice, estimate_view};

/// One thing a revision changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ChangeView {
    Created,
    /// Created again after being deleted.
    Recreated,
    Deleted,
    Renamed {
        from: String,
        to: String,
    },
    /// The notes after the change; empty if they were removed.
    Notes {
        notes: String,
    },
    /// A plan added from a date, or the plan from that date changed.
    PlanSet {
        from: String,
        price: MoneyView,
        billing: Billing,
        anchor: String,
        estimate: Option<EstimateView>,
        due: DueChoice,
    },
    PlanRemoved {
        from: String,
    },
    StatusSet {
        from: String,
        status: StatusChoice,
    },
    StatusRemoved {
        from: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CauseView {
    Created,
    Edited,
    Deleted,
    Imported,
}

impl From<Cause> for CauseView {
    fn from(cause: Cause) -> Self {
        match cause {
            Cause::Created => CauseView::Created,
            Cause::Edited => CauseView::Edited,
            Cause::Deleted => CauseView::Deleted,
            Cause::Imported => CauseView::Imported,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RevisionView {
    pub id: String,
    /// RFC 3339 timestamp, from the recording device's clock.
    pub created_at: String,
    pub cause: CauseView,
    /// Recorded on this device rather than one whose backup was imported.
    pub this_device: bool,
    /// Settled a conflict between two versions; both stay in the history.
    pub merged: bool,
    /// Compared with the version it replaced.
    pub changes: Vec<ChangeView>,
    /// The subscription's latest revision.
    pub current: bool,
    /// Has a version that differs from the current one, so restoring it would
    /// change something.
    pub restorable: bool,
}

/// A deleted subscription that can be restored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DeletedView {
    pub id: String,
    pub name: String,
    pub price: MoneyView,
    pub billing: Billing,
    /// RFC 3339 timestamp of the deletion.
    pub deleted_at: String,
    /// The revision holding the version that was deleted, to restore.
    pub revision_id: String,
}

fn plan_view(currency: epicycle_core::money::Currency, plan: &Dated<Plan>) -> ChangeView {
    ChangeView::PlanSet {
        from: plan.from.to_string(),
        price: Money::new(plan.value.amount, currency).into(),
        billing: plan.value.schedule.rule().into(),
        anchor: plan.value.schedule.anchor().to_string(),
        estimate: estimate_view(currency, &plan.value),
        due: plan.value.due.into(),
    }
}

fn by_date<T: Copy>(changes: &[Dated<T>]) -> BTreeMap<Date, T> {
    changes.iter().map(|c| (c.from, c.value)).collect()
}

/// Changes between two dated lists: values set (added or different) and dates
/// removed, in date order.
fn dated_diff<T: Copy + PartialEq>(
    before: &[Dated<T>],
    after: &[Dated<T>],
    set: impl Fn(&Dated<T>) -> ChangeView,
    removed: impl Fn(Date) -> ChangeView,
) -> Vec<ChangeView> {
    let (old, new) = (by_date(before), by_date(after));
    let mut dates: Vec<Date> = old.keys().chain(new.keys()).copied().collect();
    dates.sort();
    dates.dedup();
    dates
        .into_iter()
        .filter_map(|date| match (old.get(&date), new.get(&date)) {
            (_, Some(value)) if old.get(&date) != Some(value) => Some(set(&Dated {
                from: date,
                value: *value,
            })),
            (Some(_), None) => Some(removed(date)),
            _ => None,
        })
        .collect()
}

/// What changed from `before` to `after`. `had_parent` tells a new
/// subscription from one recreated after a deletion.
pub fn changes(
    before: Option<&Subscription>,
    after: Option<&Subscription>,
    had_parent: bool,
) -> Vec<ChangeView> {
    match (before, after) {
        (_, None) => vec![ChangeView::Deleted],
        (None, Some(_)) if had_parent => vec![ChangeView::Recreated],
        (None, Some(_)) => vec![ChangeView::Created],
        (Some(b), Some(a)) => {
            let mut out = Vec::new();
            if b.name() != a.name() {
                out.push(ChangeView::Renamed {
                    from: b.name().to_owned(),
                    to: a.name().to_owned(),
                });
            }
            if b.notes() != a.notes() {
                out.push(ChangeView::Notes {
                    notes: a.notes().to_owned(),
                });
            }
            let currency = a.currency();
            out.extend(dated_diff(
                b.plans(),
                a.plans(),
                |p| plan_view(currency, p),
                |from| ChangeView::PlanRemoved {
                    from: from.to_string(),
                },
            ));
            out.extend(dated_diff(
                b.status_changes(),
                a.status_changes(),
                |c: &Dated<Status>| ChangeView::StatusSet {
                    from: c.from.to_string(),
                    status: c.value.into(),
                },
                |from| ChangeView::StatusRemoved {
                    from: from.to_string(),
                },
            ));
            out
        }
    }
}

/// A subscription's revisions, newest first, each with what it changed.
/// `revisions` must all belong to one subscription.
pub fn history_view(revisions: &History, device_id: Uuid) -> Vec<RevisionView> {
    let all = revisions.revisions();
    let head = all
        .first()
        .and_then(|r| revisions.head(r.subscription_id))
        .map(|r| r.id);
    let current = head.and_then(|h| revisions.get(h)?.subscription.as_ref());
    let mut views: Vec<(jiff::Timestamp, RevisionView)> = all
        .iter()
        .map(|r: &Revision| {
            // Compare with the first parent: this device's side of a merge.
            let parent = r.parents.first().and_then(|p| revisions.get(*p));
            let before = parent.and_then(|p| p.subscription.as_ref());
            let view = RevisionView {
                id: r.id.to_string(),
                created_at: r.created_at.to_string(),
                cause: r.cause.into(),
                this_device: r.device_id == device_id,
                merged: r.parents.len() > 1,
                changes: changes(before, r.subscription.as_ref(), parent.is_some()),
                current: Some(r.id) == head,
                restorable: r.subscription.is_some() && r.subscription.as_ref() != current,
            };
            (r.created_at, view)
        })
        .collect();
    // Newest first. Parents come before children, so reversing first makes
    // children win ties under the stable sort.
    views.reverse();
    views.sort_by_key(|v| std::cmp::Reverse(v.0));
    views.into_iter().map(|(_, v)| v).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{NewSubscription, Unit, new_subscription};
    use jiff::civil::date;

    fn music() -> Subscription {
        new_subscription(
            &NewSubscription {
                name: "Music".into(),
                amount: "10".into(),
                currency: "EUR".into(),
                every: 1,
                unit: Unit::Months,
                next_payment: "2026-10-15".into(),
                since: None,
                notes: String::new(),
                estimate: None,
                due: Default::default(),
            },
            date(2026, 10, 1),
        )
        .unwrap()
    }

    #[test]
    fn describes_creation_and_deletion() {
        let s = music();
        assert_eq!(changes(None, Some(&s), false), [ChangeView::Created]);
        assert_eq!(changes(None, Some(&s), true), [ChangeView::Recreated]);
        assert_eq!(changes(Some(&s), None, true), [ChangeView::Deleted]);
    }

    #[test]
    fn lists_each_field_that_changed() {
        let before = music();
        let plan = *before.plan_on(date(2026, 10, 15)).unwrap();
        let after = before
            .renamed("Music Family")
            .unwrap()
            .with_notes("Shared")
            .with_plan(
                date(2026, 12, 1),
                Plan {
                    amount: 1500,
                    ..plan
                },
            )
            .unwrap()
            .with_status(date(2027, 1, 1), Status::Paused)
            .unwrap();
        let c = changes(Some(&before), Some(&after), true);
        assert_eq!(
            c[0],
            ChangeView::Renamed {
                from: "Music".into(),
                to: "Music Family".into()
            }
        );
        assert_eq!(
            c[1],
            ChangeView::Notes {
                notes: "Shared".into()
            }
        );
        assert!(matches!(&c[2], ChangeView::PlanSet { from, price, .. }
            if from == "2026-12-01" && price.amount == "15.00"));
        assert_eq!(
            c[3],
            ChangeView::StatusSet {
                from: "2027-01-01".into(),
                status: StatusChoice::Paused
            }
        );
        assert_eq!(c.len(), 4);
        // And back again: the plan and status are removed.
        let back = changes(Some(&after), Some(&before), true);
        assert!(back.contains(&ChangeView::PlanRemoved {
            from: "2026-12-01".into()
        }));
        assert!(back.contains(&ChangeView::StatusRemoved {
            from: "2027-01-01".into()
        }));
        assert!(changes(Some(&before), Some(&before), true).is_empty());
    }
}
