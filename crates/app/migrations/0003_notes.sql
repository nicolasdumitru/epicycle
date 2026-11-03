-- Plain-text notes on each subscription; empty when there are none.
--
-- Revision snapshots carry the notes too, from snapshot version 2. Version 1
-- snapshots are the same JSON without notes.
ALTER TABLE subscriptions ADD COLUMN notes TEXT NOT NULL DEFAULT '';
