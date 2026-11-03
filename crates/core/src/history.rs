//! Change history: every saved version of every subscription, linked to the
//! version it replaced, like the commits of a Git repository with one branch.
//!
//! Importing a backup merges the backup's history into this device's. Because
//! both sides keep their history, an import can tell an outdated copy of a
//! subscription from a real conflict, where both sides changed it separately.
//! See DECISIONS.md, "Backups".

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::subscription::{Subscription, SubscriptionId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RevisionId(pub Uuid);

impl fmt::Display for RevisionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Why a revision was recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cause {
    Created,
    Edited,
    Deleted,
    /// Recorded by an import, to settle what the subscription is afterwards.
    Imported,
}

impl Cause {
    pub fn as_str(self) -> &'static str {
        match self {
            Cause::Created => "created",
            Cause::Edited => "edited",
            Cause::Deleted => "deleted",
            Cause::Imported => "imported",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("unknown revision cause {0:?}")]
pub struct ParseCauseError(String);

impl FromStr for Cause {
    type Err = ParseCauseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "created" => Ok(Cause::Created),
            "edited" => Ok(Cause::Edited),
            "deleted" => Ok(Cause::Deleted),
            "imported" => Ok(Cause::Imported),
            other => Err(ParseCauseError(other.to_owned())),
        }
    }
}

/// One entry of a subscription's change history. Revisions are never changed
/// once recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revision {
    pub id: RevisionId,
    pub subscription_id: SubscriptionId,
    /// The revisions this one replaces: none for the first, and two when it
    /// settles a conflict between two histories that diverged.
    pub parents: Vec<RevisionId>,
    /// The device that recorded it.
    pub device_id: Uuid,
    pub created_at: Timestamp,
    pub cause: Cause,
    /// The subscription after the change; `None` exactly when it was deleted.
    pub subscription: Option<Subscription>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HistoryError {
    #[error("revision {0} appears more than once")]
    Duplicate(RevisionId),
    #[error("revision {0} has more than two parents, or the same parent twice")]
    BadParents(RevisionId),
    #[error("revision {0} must have no subscription exactly when it is a deletion")]
    DeletionMismatch(RevisionId),
    #[error("revision {0} belongs to subscription {1} but contains subscription {2}")]
    WrongSubscription(RevisionId, SubscriptionId, SubscriptionId),
    #[error("revision {0} refers to parent {1}, which is missing")]
    MissingParent(RevisionId, RevisionId),
    #[error("revision {0} and its parent {1} belong to different subscriptions")]
    ParentOfOtherSubscription(RevisionId, RevisionId),
    #[error("revision {0} is its own ancestor")]
    Cycle(RevisionId),
    #[error("subscription {0} has more than one latest revision")]
    MultipleHeads(SubscriptionId),
    #[error("revision {0} differs between the backup and this device")]
    Mismatch(RevisionId),
}

/// A consistent set of revisions: every parent is present and belongs to the
/// same subscription, there are no cycles, and each subscription has exactly
/// one latest revision (its head).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct History {
    /// Parents before children, otherwise by ID, so the order is deterministic.
    revisions: Vec<Revision>,
    index: BTreeMap<RevisionId, usize>,
    heads: BTreeMap<SubscriptionId, usize>,
}

impl History {
    pub fn new(revisions: impl IntoIterator<Item = Revision>) -> Result<History, HistoryError> {
        let mut by_id = BTreeMap::new();
        for revision in revisions {
            check_revision(&revision)?;
            let id = revision.id;
            if by_id.insert(id, revision).is_some() {
                return Err(HistoryError::Duplicate(id));
            }
        }

        let mut children: BTreeMap<RevisionId, Vec<RevisionId>> = BTreeMap::new();
        let mut unsorted_parents: BTreeMap<RevisionId, usize> = BTreeMap::new();
        for revision in by_id.values() {
            for parent_id in &revision.parents {
                let parent = by_id
                    .get(parent_id)
                    .ok_or(HistoryError::MissingParent(revision.id, *parent_id))?;
                if parent.subscription_id != revision.subscription_id {
                    return Err(HistoryError::ParentOfOtherSubscription(
                        revision.id,
                        *parent_id,
                    ));
                }
                children.entry(*parent_id).or_default().push(revision.id);
            }
            unsorted_parents.insert(revision.id, revision.parents.len());
        }

        // Topological sort (Kahn's algorithm), taking the smallest ready ID first.
        let mut ready: BTreeSet<RevisionId> = unsorted_parents
            .iter()
            .filter(|(_, count)| **count == 0)
            .map(|(id, _)| *id)
            .collect();
        let mut order = Vec::with_capacity(by_id.len());
        while let Some(id) = ready.pop_first() {
            order.push(id);
            for child in children.get(&id).into_iter().flatten() {
                if let Some(count) = unsorted_parents.get_mut(child) {
                    *count -= 1;
                    if *count == 0 {
                        ready.insert(*child);
                    }
                }
            }
        }
        if order.len() != by_id.len() {
            let stuck = unsorted_parents
                .iter()
                .find(|(_, count)| **count > 0)
                .map(|(id, _)| *id);
            if let Some(id) = stuck {
                return Err(HistoryError::Cycle(id));
            }
        }

        let revisions: Vec<Revision> = order.iter().filter_map(|id| by_id.remove(id)).collect();
        let index = revisions
            .iter()
            .enumerate()
            .map(|(i, r)| (r.id, i))
            .collect();
        let mut heads = BTreeMap::new();
        for (i, revision) in revisions.iter().enumerate() {
            if !children.contains_key(&revision.id)
                && heads.insert(revision.subscription_id, i).is_some()
            {
                return Err(HistoryError::MultipleHeads(revision.subscription_id));
            }
        }
        Ok(History {
            revisions,
            index,
            heads,
        })
    }

    /// All revisions, parents before children.
    pub fn revisions(&self) -> &[Revision] {
        &self.revisions
    }

    pub fn get(&self, id: RevisionId) -> Option<&Revision> {
        self.index.get(&id).map(|&i| &self.revisions[i])
    }

    /// The latest revision of a subscription.
    pub fn head(&self, id: SubscriptionId) -> Option<&Revision> {
        self.heads.get(&id).map(|&i| &self.revisions[i])
    }

    /// Current subscriptions: the latest revision of each that isn't deleted.
    pub fn current(&self) -> impl Iterator<Item = &Subscription> {
        self.heads
            .values()
            .filter_map(|&i| self.revisions[i].subscription.as_ref())
    }

    /// Whether `ancestor` is `revision` or one of the revisions it replaced.
    pub fn has_ancestor(&self, revision: RevisionId, ancestor: RevisionId) -> bool {
        let mut stack = vec![revision];
        let mut seen = BTreeSet::new();
        while let Some(id) = stack.pop() {
            if id == ancestor {
                return true;
            }
            if seen.insert(id)
                && let Some(r) = self.get(id)
            {
                stack.extend(&r.parents);
            }
        }
        false
    }
}

fn check_revision(r: &Revision) -> Result<(), HistoryError> {
    let distinct = match r.parents.as_slice() {
        [] | [_] => true,
        [a, b] => a != b,
        _ => false,
    };
    if !distinct {
        return Err(HistoryError::BadParents(r.id));
    }
    if (r.cause == Cause::Deleted) != r.subscription.is_none() {
        return Err(HistoryError::DeletionMismatch(r.id));
    }
    if let Some(s) = &r.subscription
        && s.id() != r.subscription_id
    {
        return Err(HistoryError::WrongSubscription(
            r.id,
            r.subscription_id,
            s.id(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportMode {
    /// Add what is new in the backup and keep this device's own changes.
    Merge,
    /// Make this device match the backup. What it replaces stays in the history.
    Replace,
}

/// The two sides of an import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    /// This device.
    Local,
    /// The backup being imported.
    Incoming,
}

/// How an import moves a subscription's latest revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewHead {
    /// A revision from the backup becomes the latest.
    Existing(RevisionId),
    /// A new revision is recorded, replacing `parents`.
    Record {
        parents: Vec<RevisionId>,
        cause: Cause,
    },
}

/// Both sides changed a subscription separately and one version was kept. The
/// other stays in the history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub kept: Side,
    pub local: Option<Subscription>,
    pub local_changed_at: Timestamp,
    pub incoming: Option<Subscription>,
    pub incoming_changed_at: Timestamp,
}

/// What an import does to one subscription.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub subscription_id: SubscriptionId,
    /// The subscription before the import; `None` if absent or deleted.
    pub before: Option<Subscription>,
    /// The subscription after the import; `None` if absent or deleted.
    pub after: Option<Subscription>,
    pub head: NewHead,
    pub conflict: Option<Conflict>,
}

/// Everything an import does, worked out before anything is written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportPlan {
    /// Revisions from the backup that this device doesn't have, parents first.
    pub new_revisions: Vec<Revision>,
    /// Subscriptions whose latest revision changes.
    pub changes: Vec<Change>,
}

impl ImportPlan {
    /// Subscriptions that appear.
    pub fn added(&self) -> usize {
        self.count(|c| c.before.is_none() && c.after.is_some())
    }

    /// Subscriptions that stay but change.
    pub fn updated(&self) -> usize {
        self.count(|c| c.before.is_some() && c.after.is_some() && c.before != c.after)
    }

    /// Subscriptions that disappear.
    pub fn removed(&self) -> usize {
        self.count(|c| c.before.is_some() && c.after.is_none())
    }

    pub fn conflicts(&self) -> impl Iterator<Item = &Conflict> {
        self.changes.iter().filter_map(|c| c.conflict.as_ref())
    }

    fn count(&self, f: impl Fn(&Change) -> bool) -> usize {
        self.changes.iter().filter(|c| f(c)).count()
    }
}

/// Works out how to import `incoming` into `local`.
///
/// For each subscription, comparing the two latest revisions gives one of:
/// - the same revision: nothing to do;
/// - the backup's is an older point in this device's history: keep this
///   device's (in Replace mode, record the backup's version on top);
/// - this device's is an older point in the backup's history: take the
///   backup's;
/// - neither (both changed it separately): in Merge mode the later change
///   wins and a tie keeps this device's, in Replace mode the backup's wins.
///   A new revision records both as parents, so the other version stays in
///   the history.
///
/// In Replace mode, subscriptions only on this device are deleted.
pub fn plan_import(
    local: &History,
    incoming: &History,
    mode: ImportMode,
) -> Result<ImportPlan, HistoryError> {
    let mut new_revisions = Vec::new();
    for revision in incoming.revisions() {
        match local.get(revision.id) {
            Some(existing) if existing != revision => {
                return Err(HistoryError::Mismatch(revision.id));
            }
            Some(_) => {}
            None => new_revisions.push(revision.clone()),
        }
    }

    let ids: BTreeSet<SubscriptionId> = local
        .heads
        .keys()
        .chain(incoming.heads.keys())
        .copied()
        .collect();
    let changes = ids
        .into_iter()
        .filter_map(|id| resolve(local, incoming, id, mode))
        .collect();
    Ok(ImportPlan {
        new_revisions,
        changes,
    })
}

fn resolve(
    local: &History,
    incoming: &History,
    id: SubscriptionId,
    mode: ImportMode,
) -> Option<Change> {
    let local_head = local.head(id);
    let incoming_head = incoming.head(id);
    let before = local_head.and_then(|r| r.subscription.clone());

    let record = |parents: Vec<RevisionId>, after: Option<Subscription>| Change {
        subscription_id: id,
        before: before.clone(),
        head: NewHead::Record {
            parents,
            cause: if after.is_some() {
                Cause::Imported
            } else {
                Cause::Deleted
            },
        },
        after,
        conflict: None,
    };
    let take = |r: &Revision| Change {
        subscription_id: id,
        before: before.clone(),
        after: r.subscription.clone(),
        head: NewHead::Existing(r.id),
        conflict: None,
    };

    match (local_head, incoming_head) {
        (None, None) => None,
        (Some(l), None) => match mode {
            ImportMode::Merge => None,
            ImportMode::Replace => before.is_some().then(|| record(vec![l.id], None)),
        },
        (None, Some(i)) => Some(take(i)),
        (Some(l), Some(i)) if l.id == i.id => None,
        // This device already has the backup's version and changed it since.
        (Some(l), Some(i)) if local.has_ancestor(l.id, i.id) => match mode {
            ImportMode::Merge => None,
            ImportMode::Replace => (l.subscription != i.subscription)
                .then(|| record(vec![l.id], i.subscription.clone())),
        },
        // The backup has this device's version and changed it since.
        (Some(l), Some(i)) if incoming.has_ancestor(i.id, l.id) => Some(take(i)),
        (Some(l), Some(i)) => {
            let kept = match mode {
                ImportMode::Replace => Side::Incoming,
                ImportMode::Merge if i.created_at > l.created_at => Side::Incoming,
                ImportMode::Merge => Side::Local,
            };
            let after = match kept {
                Side::Local => l.subscription.clone(),
                Side::Incoming => i.subscription.clone(),
            };
            let mut change = record(vec![l.id, i.id], after);
            if mode == ImportMode::Merge && l.subscription != i.subscription {
                change.conflict = Some(Conflict {
                    kept,
                    local: l.subscription.clone(),
                    local_changed_at: l.created_at,
                    incoming: i.subscription.clone(),
                    incoming_changed_at: i.created_at,
                });
            }
            Some(change)
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::subscription::tests::{monthly_plan, subscription};
    use jiff::civil::date;

    const DEVICE: Uuid = Uuid::from_u128(0xd);

    /// Builds histories by hand. Revision IDs and timestamps count up from 1,
    /// so a later step is a later change.
    pub(crate) struct Builder {
        next: u128,
    }

    impl Builder {
        pub(crate) fn new() -> Self {
            Builder { next: 1 }
        }

        pub(crate) fn revision(
            &mut self,
            parents: &[&Revision],
            cause: Cause,
            subscription: Option<Subscription>,
            subscription_id: SubscriptionId,
        ) -> Revision {
            let n = self.next;
            self.next += 1;
            Revision {
                id: RevisionId(Uuid::from_u128(n)),
                subscription_id,
                parents: parents.iter().map(|p| p.id).collect(),
                device_id: DEVICE,
                created_at: Timestamp::from_second(1_800_000_000 + n as i64).unwrap(),
                cause,
                subscription,
            }
        }

        pub(crate) fn create(&mut self, s: &Subscription) -> Revision {
            self.revision(&[], Cause::Created, Some(s.clone()), s.id())
        }

        pub(crate) fn edit(&mut self, parent: &Revision, s: &Subscription) -> Revision {
            self.revision(&[parent], Cause::Edited, Some(s.clone()), s.id())
        }

        pub(crate) fn delete(&mut self, parent: &Revision) -> Revision {
            self.revision(&[parent], Cause::Deleted, None, parent.subscription_id)
        }
    }

    fn sub(name: &str) -> Subscription {
        subscription(date(2026, 1, 15), monthly_plan(date(2026, 1, 15), 999))
            .renamed(name)
            .unwrap()
    }

    fn history(revisions: &[&Revision]) -> History {
        History::new(revisions.iter().map(|r| (*r).clone())).unwrap()
    }

    fn plan(local: &[&Revision], incoming: &[&Revision], mode: ImportMode) -> ImportPlan {
        plan_import(&history(local), &history(incoming), mode).unwrap()
    }

    #[test]
    fn orders_parents_first_and_finds_heads() {
        let mut b = Builder::new();
        let a = sub("A");
        let r1 = b.create(&a);
        let r2 = b.edit(&r1, &a.renamed("A2").unwrap());
        let r3 = b.edit(&r2, &a.renamed("A3").unwrap());
        // Given in reverse, still sorted parents first.
        let h = history(&[&r3, &r1, &r2]);
        let ids: Vec<_> = h.revisions().iter().map(|r| r.id).collect();
        assert_eq!(ids, [r1.id, r2.id, r3.id]);
        assert_eq!(h.head(a.id()), Some(&r3));
        assert!(h.has_ancestor(r3.id, r1.id));
        assert!(h.has_ancestor(r2.id, r2.id));
        assert!(!h.has_ancestor(r1.id, r3.id));
        assert_eq!(h.current().map(|s| s.name()).collect::<Vec<_>>(), ["A3"]);
    }

    #[test]
    fn deleted_subscriptions_are_not_current() {
        let mut b = Builder::new();
        let r1 = b.create(&sub("A"));
        let r2 = b.delete(&r1);
        let h = history(&[&r1, &r2]);
        assert_eq!(h.current().count(), 0);
        assert_eq!(h.head(r1.subscription_id), Some(&r2));
    }

    #[test]
    fn rejects_inconsistent_histories() {
        let mut b = Builder::new();
        let a = sub("A");
        let r1 = b.create(&a);
        let r2 = b.edit(&r1, &a);
        let check = |revisions: Vec<Revision>| History::new(revisions).unwrap_err();

        assert_eq!(
            check(vec![r1.clone(), r1.clone()]),
            HistoryError::Duplicate(r1.id)
        );
        assert_eq!(
            check(vec![r2.clone()]),
            HistoryError::MissingParent(r2.id, r1.id)
        );

        let sibling = b.edit(&r1, &a);
        assert_eq!(
            check(vec![r1.clone(), r2.clone(), sibling]),
            HistoryError::MultipleHeads(a.id())
        );

        let mut cycle_a = b.edit(&r1, &a);
        let cycle_b = b.edit(&cycle_a, &a);
        cycle_a.parents = vec![cycle_b.id];
        assert!(matches!(
            check(vec![r1.clone(), cycle_a, cycle_b]),
            HistoryError::Cycle(_)
        ));

        let other = sub("B");
        let mut wrong = b.create(&other);
        wrong.subscription_id = a.id();
        assert_eq!(
            check(vec![wrong.clone()]),
            HistoryError::WrongSubscription(wrong.id, a.id(), other.id())
        );

        let mut crossed = b.edit(&r1, &a);
        let foreign = b.create(&other);
        crossed.parents = vec![foreign.id];
        assert_eq!(
            check(vec![foreign.clone(), crossed.clone()]),
            HistoryError::ParentOfOtherSubscription(crossed.id, foreign.id)
        );

        let mut deletion = b.delete(&r1);
        deletion.subscription = Some(a.clone());
        assert_eq!(
            check(vec![r1.clone(), deletion.clone()]),
            HistoryError::DeletionMismatch(deletion.id)
        );

        let mut twice = b.edit(&r1, &a);
        twice.parents = vec![r1.id, r1.id];
        assert_eq!(
            check(vec![r1.clone(), twice.clone()]),
            HistoryError::BadParents(twice.id)
        );
    }

    #[test]
    fn importing_the_same_history_does_nothing() {
        let mut b = Builder::new();
        let r1 = b.create(&sub("A"));
        for mode in [ImportMode::Merge, ImportMode::Replace] {
            assert_eq!(plan(&[&r1], &[&r1], mode), ImportPlan::default());
        }
    }

    #[test]
    fn adds_subscriptions_this_device_does_not_have() {
        let mut b = Builder::new();
        let a = sub("A");
        let r1 = b.create(&a);
        let p = plan(&[], &[&r1], ImportMode::Merge);
        assert_eq!(p.new_revisions, std::slice::from_ref(&r1));
        assert_eq!(p.changes.len(), 1);
        assert_eq!(p.changes[0].head, NewHead::Existing(r1.id));
        assert_eq!(p.changes[0].after.as_ref(), Some(&a));
        assert_eq!((p.added(), p.updated(), p.removed()), (1, 0, 0));
    }

    #[test]
    fn an_older_backup_does_not_undo_later_changes() {
        let mut b = Builder::new();
        let a = sub("A");
        let r1 = b.create(&a);
        let edited = b.edit(&r1, &a.renamed("A2").unwrap());
        assert_eq!(
            plan(&[&r1, &edited], &[&r1], ImportMode::Merge),
            ImportPlan::default()
        );

        // In particular, a deleted subscription stays deleted.
        let deleted = b.delete(&r1);
        assert_eq!(
            plan(&[&r1, &deleted], &[&r1], ImportMode::Merge),
            ImportPlan::default()
        );
    }

    #[test]
    fn takes_newer_versions_from_the_backup() {
        let mut b = Builder::new();
        let a = sub("A");
        let r1 = b.create(&a);
        let r2 = b.edit(&r1, &a.renamed("A2").unwrap());
        let p = plan(&[&r1], &[&r1, &r2], ImportMode::Merge);
        assert_eq!(p.new_revisions, std::slice::from_ref(&r2));
        assert_eq!(p.changes[0].head, NewHead::Existing(r2.id));
        assert_eq!(p.changes[0].after, r2.subscription);
        assert_eq!((p.added(), p.updated(), p.removed()), (0, 1, 0));

        let deleted = b.delete(&r1);
        let p = plan(&[&r1], &[&r1, &deleted], ImportMode::Merge);
        assert_eq!(p.changes[0].head, NewHead::Existing(deleted.id));
        assert_eq!((p.added(), p.updated(), p.removed()), (0, 0, 1));
    }

    #[test]
    fn merge_keeps_the_later_of_two_separate_changes() {
        let mut b = Builder::new();
        let a = sub("A");
        let r1 = b.create(&a);
        let earlier = b.edit(&r1, &a.renamed("Earlier").unwrap());
        let later = b.edit(&r1, &a.renamed("Later").unwrap());

        for (local, incoming, kept) in [
            (&earlier, &later, Side::Incoming),
            (&later, &earlier, Side::Local),
        ] {
            let p = plan(&[&r1, local], &[&r1, incoming], ImportMode::Merge);
            let change = &p.changes[0];
            assert_eq!(
                change.head,
                NewHead::Record {
                    parents: vec![local.id, incoming.id],
                    cause: Cause::Imported
                }
            );
            assert_eq!(change.after, later.subscription);
            let conflict = change.conflict.as_ref().unwrap();
            assert_eq!(conflict.kept, kept);
            assert_eq!(conflict.local, local.subscription);
            assert_eq!(conflict.incoming, incoming.subscription);
        }
    }

    #[test]
    fn merge_tie_keeps_this_devices_version() {
        let mut b = Builder::new();
        let a = sub("A");
        let r1 = b.create(&a);
        let mine = b.edit(&r1, &a.renamed("Mine").unwrap());
        let mut theirs = b.edit(&r1, &a.renamed("Theirs").unwrap());
        theirs.created_at = mine.created_at;
        let p = plan(&[&r1, &mine], &[&r1, &theirs], ImportMode::Merge);
        assert_eq!(p.changes[0].after, mine.subscription);
        assert_eq!(p.changes[0].conflict.as_ref().unwrap().kept, Side::Local);
    }

    #[test]
    fn identical_separate_changes_are_not_a_conflict() {
        let mut b = Builder::new();
        let a = sub("A");
        let r1 = b.create(&a);
        let renamed = a.renamed("Same").unwrap();
        let mine = b.edit(&r1, &renamed);
        let theirs = b.edit(&r1, &renamed);
        let p = plan(&[&r1, &mine], &[&r1, &theirs], ImportMode::Merge);
        // Still joined into one history, but nothing to report.
        assert!(matches!(p.changes[0].head, NewHead::Record { .. }));
        assert_eq!(p.conflicts().count(), 0);
        assert_eq!(p.updated(), 0);
    }

    #[test]
    fn a_deletion_can_win_a_conflict() {
        let mut b = Builder::new();
        let a = sub("A");
        let r1 = b.create(&a);
        let edited = b.edit(&r1, &a.renamed("A2").unwrap());
        let deleted = b.delete(&r1);
        let p = plan(&[&r1, &edited], &[&r1, &deleted], ImportMode::Merge);
        assert_eq!(p.changes[0].after, None);
        assert_eq!(
            p.changes[0].head,
            NewHead::Record {
                parents: vec![edited.id, deleted.id],
                cause: Cause::Deleted
            }
        );
        assert_eq!(p.removed(), 1);
    }

    #[test]
    fn merge_keeps_subscriptions_only_on_this_device() {
        let mut b = Builder::new();
        let r1 = b.create(&sub("Mine"));
        assert_eq!(plan(&[&r1], &[], ImportMode::Merge), ImportPlan::default());
    }

    #[test]
    fn replace_makes_this_device_match_the_backup() {
        let mut b = Builder::new();
        let a = sub("A");
        let only_here = b.create(&sub("Only here"));
        let gone = b.create(&sub("Already deleted"));
        let gone_deleted = b.delete(&gone);
        let r1 = b.create(&a);
        let local_edit = b.edit(&r1, &a.renamed("Edited here").unwrap());

        let p = plan(
            &[&only_here, &gone, &gone_deleted, &r1, &local_edit],
            &[&r1],
            ImportMode::Replace,
        );
        assert_eq!(p.changes.len(), 2);
        let removed = p
            .changes
            .iter()
            .find(|c| c.subscription_id == only_here.subscription_id)
            .unwrap();
        assert_eq!(removed.after, None);
        assert_eq!(
            removed.head,
            NewHead::Record {
                parents: vec![only_here.id],
                cause: Cause::Deleted
            }
        );
        // The backup's older version is recorded on top of this device's.
        let reverted = p
            .changes
            .iter()
            .find(|c| c.subscription_id == a.id())
            .unwrap();
        assert_eq!(reverted.after.as_ref(), Some(&a));
        assert_eq!(
            reverted.head,
            NewHead::Record {
                parents: vec![local_edit.id],
                cause: Cause::Imported
            }
        );
        assert_eq!((p.added(), p.updated(), p.removed()), (0, 1, 1));
        assert_eq!(p.conflicts().count(), 0);
    }

    #[test]
    fn replace_takes_the_backups_side_of_a_conflict() {
        let mut b = Builder::new();
        let a = sub("A");
        let r1 = b.create(&a);
        let theirs = b.edit(&r1, &a.renamed("Theirs").unwrap());
        let mine = b.edit(&r1, &a.renamed("Mine, later").unwrap());
        let p = plan(&[&r1, &mine], &[&r1, &theirs], ImportMode::Replace);
        assert_eq!(p.changes[0].after, theirs.subscription);
        assert_eq!(p.conflicts().count(), 0);
    }

    #[test]
    fn rejects_revisions_that_differ_between_sides() {
        let mut b = Builder::new();
        let a = sub("A");
        let r1 = b.create(&a);
        let mut tampered = r1.clone();
        tampered.subscription = Some(a.renamed("Changed").unwrap());
        assert_eq!(
            plan_import(&history(&[&r1]), &history(&[&tampered]), ImportMode::Merge),
            Err(HistoryError::Mismatch(r1.id))
        );
    }
}
