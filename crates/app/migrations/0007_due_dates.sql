-- When payments are due after their bill (see docs/DECISIONS.md, "Estimated
-- amounts, due dates and one-off changes"): when billed, a number of days
-- later, or on a day of the month (the first such day on or after the bill).
ALTER TABLE plans ADD COLUMN due_kind TEXT NOT NULL DEFAULT 'on_billing'
    CHECK (due_kind IN ('on_billing', 'days_after', 'day_of_month'));
ALTER TABLE plans ADD COLUMN due_value INTEGER
    CHECK (
        (due_kind = 'on_billing' AND due_value IS NULL)
        OR (due_kind = 'days_after' AND due_value BETWEEN 0 AND 65535)
        OR (due_kind = 'day_of_month' AND due_value BETWEEN 1 AND 31)
    );
