-- Exchange rates, stored the way the ECB publishes them: units of a currency
-- per 1 EUR on a date (see docs/DECISIONS.md, "Exchange rates").
--
-- ECB rates are a cache that can be fetched again; manual rates are the user's
-- own data. Both can exist for the same currency and date, and the manual one
-- wins.
CREATE TABLE exchange_rates (
    currency TEXT NOT NULL CHECK (length(currency) = 3 AND currency <> 'EUR'),
    date TEXT NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('ecb', 'manual')),
    -- An exact decimal, as text, so no precision is lost.
    per_eur TEXT NOT NULL,
    -- When it was fetched or entered (RFC 3339, UTC).
    recorded_at TEXT NOT NULL,
    PRIMARY KEY (currency, date, source)
) STRICT;
