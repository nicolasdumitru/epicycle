-- Revisions with two parents.
--
-- When an import finds that this device and the backup changed a subscription
-- separately, it records a revision that replaces both: `parent_id` is this
-- device's side and `merge_parent_id` the backup's (see DECISIONS.md, "Backups").
ALTER TABLE revisions ADD COLUMN merge_parent_id TEXT REFERENCES revisions (id)
    CHECK (merge_parent_id IS NULL OR (parent_id IS NOT NULL AND merge_parent_id <> parent_id));

-- A subscription's latest revision is the one no other revision names as a parent.
CREATE INDEX revisions_by_parent ON revisions (parent_id);
CREATE INDEX revisions_by_merge_parent ON revisions (merge_parent_id);
