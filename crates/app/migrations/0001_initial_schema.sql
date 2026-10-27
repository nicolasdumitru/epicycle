-- Initial schema.
--
-- Current state lives in `subscriptions`, `plans` and `status_changes`. Every
-- change also appends a full JSON copy of the subscription to `revisions`, which
-- is never updated or deleted from (see DECISIONS.md, "Data history").
--
-- Dates are ISO 8601 text (YYYY-MM-DD), timestamps RFC 3339 text in UTC, and IDs
-- are UUIDs as text. STRICT tables make SQLite enforce the declared types.

-- One row of per-install state.
CREATE TABLE app_state (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
    -- Identifies this installation in the change history.
    device_id TEXT NOT NULL,
    reporting_currency TEXT NOT NULL CHECK (length(reporting_currency) = 3)
) STRICT;

-- Append-only history of every subscription change.
CREATE TABLE revisions (
    id TEXT PRIMARY KEY NOT NULL,
    -- No foreign key: a deleted subscription keeps its history.
    subscription_id TEXT NOT NULL,
    -- The revision this one replaces; NULL for the first one.
    parent_id TEXT REFERENCES revisions (id),
    device_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    cause TEXT NOT NULL CHECK (cause IN ('created', 'edited', 'deleted', 'imported')),
    -- Version of the JSON format of `snapshot`, so old revisions can be upgraded.
    snapshot_version INTEGER NOT NULL,
    -- The subscription after the change; NULL exactly when it was deleted.
    snapshot TEXT,
    CHECK ((cause = 'deleted') = (snapshot IS NULL))
) STRICT;

CREATE INDEX revisions_by_subscription ON revisions (subscription_id, created_at);

CREATE TABLE subscriptions (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    currency TEXT NOT NULL CHECK (length(currency) = 3),
    -- The latest revision, which the next change will name as its parent.
    head_revision_id TEXT NOT NULL REFERENCES revisions (id)
) STRICT;

-- A subscription's plans. The earliest `effective_from` is its start date.
CREATE TABLE plans (
    subscription_id TEXT NOT NULL REFERENCES subscriptions (id) ON DELETE CASCADE,
    effective_from TEXT NOT NULL,
    unit TEXT NOT NULL CHECK (unit IN ('days', 'weeks', 'months', 'years')),
    every INTEGER NOT NULL CHECK (every BETWEEN 1 AND 65535),
    month INTEGER CHECK (month BETWEEN 1 AND 12),
    day INTEGER CHECK (day BETWEEN 1 AND 31),
    anchor TEXT NOT NULL,
    -- Minor units of the subscription's currency.
    amount INTEGER NOT NULL CHECK (amount >= 0),
    PRIMARY KEY (subscription_id, effective_from),
    CHECK (
        (unit IN ('days', 'weeks') AND month IS NULL AND day IS NULL)
        OR (unit = 'months' AND month IS NULL AND day IS NOT NULL)
        OR (unit = 'years' AND month IS NOT NULL AND day IS NOT NULL)
    )
) STRICT;

CREATE TABLE status_changes (
    subscription_id TEXT NOT NULL REFERENCES subscriptions (id) ON DELETE CASCADE,
    effective_from TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('active', 'paused', 'cancelled')),
    PRIMARY KEY (subscription_id, effective_from)
) STRICT;
