-- Currencies the user switches between quickly, in their order.
CREATE TABLE favorite_currencies (
    currency TEXT PRIMARY KEY NOT NULL CHECK (length(currency) = 3),
    position INTEGER NOT NULL
) STRICT;
