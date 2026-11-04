-- Amounts that aren't exact (see docs/DECISIONS.md, "Estimated amounts, due
-- dates and one-off changes"). `amount` stays the amount expected per payment;
-- these columns hold the bounds and how the expected amount was chosen, and are
-- all NULL for an exact amount.
ALTER TABLE plans ADD COLUMN estimate_low INTEGER CHECK (estimate_low >= 0);
ALTER TABLE plans ADD COLUMN estimate_high INTEGER CHECK (estimate_high >= 0);
ALTER TABLE plans ADD COLUMN estimate_expected TEXT
    CHECK (estimate_expected IN ('midpoint', 'bound', 'manual'))
    -- An estimate has at least one bound; an exact amount has none.
    CHECK ((estimate_expected IS NULL) = (estimate_low IS NULL AND estimate_high IS NULL))
    CHECK (estimate_low IS NULL OR estimate_high IS NULL OR estimate_low <= estimate_high);
