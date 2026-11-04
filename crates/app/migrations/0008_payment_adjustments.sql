-- Changes to single payments (see docs/DECISIONS.md, "Estimated amounts, due
-- dates and one-off changes"): what was actually paid for one bill, and when.
CREATE TABLE payment_adjustments (
    subscription_id TEXT NOT NULL REFERENCES subscriptions (id) ON DELETE CASCADE,
    -- The scheduled billing date of the payment it changes.
    billed TEXT NOT NULL,
    -- Minor units; NULL keeps the plan's amount.
    amount INTEGER CHECK (amount >= 0),
    due_kind TEXT NOT NULL CHECK (due_kind IN ('scheduled', 'on', 'removed')),
    due_date TEXT,
    PRIMARY KEY (subscription_id, billed),
    CHECK ((due_kind = 'on') = (due_date IS NOT NULL)),
    -- Only a payment of 0 can be removed.
    CHECK (due_kind <> 'removed' OR amount = 0)
) STRICT;
