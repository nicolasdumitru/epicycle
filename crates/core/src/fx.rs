//! Exchange rates and conversion to a reporting currency.
//!
//! Rates are quoted as the ECB publishes them: units of a currency per 1 EUR.
//! Any other pair is converted through EUR. Each rate keeps the date it applies
//! to and where it came from, so the UI can show when a total relies on an old
//! or manually entered rate.

use std::collections::BTreeMap;

use jiff::civil::Date;
use jiff::{Timestamp, Unit};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::money::{Currency, Fractional, MoneyError, MultiCurrency};

/// An ECB rate older than this many days is stale. The longest regular gap
/// between ECB publications is Easter: Thursday's rate is the latest until the
/// following Tuesday afternoon, five days later.
pub const STALE_AFTER_DAYS: i64 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RateSource {
    /// Downloaded from the European Central Bank.
    Ecb { fetched_at: Timestamp },
    /// Entered by the user, e.g. while offline.
    Manual { entered_at: Timestamp },
}

/// How much a rate can be trusted on a given day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Freshness {
    Fresh,
    Stale,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FxError {
    #[error("no exchange rate for {0}")]
    MissingRate(Currency),
    #[error("a rate must be positive")]
    NonPositiveRate,
    #[error("EUR is the base currency and has no rate")]
    BaseCurrency,
    #[error(transparent)]
    Money(#[from] MoneyError),
}

/// Units of `currency` per 1 EUR on `date`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "UncheckedRate")]
pub struct Rate {
    currency: Currency,
    date: Date,
    per_eur: Decimal,
    source: RateSource,
}

#[derive(Deserialize)]
struct UncheckedRate {
    currency: Currency,
    date: Date,
    per_eur: Decimal,
    source: RateSource,
}

impl TryFrom<UncheckedRate> for Rate {
    type Error = FxError;

    fn try_from(raw: UncheckedRate) -> Result<Self, Self::Error> {
        Rate::new(raw.currency, raw.date, raw.per_eur, raw.source)
    }
}

impl Rate {
    pub fn new(
        currency: Currency,
        date: Date,
        per_eur: Decimal,
        source: RateSource,
    ) -> Result<Self, FxError> {
        if currency == Currency::EUR {
            return Err(FxError::BaseCurrency);
        }
        if per_eur <= Decimal::ZERO {
            return Err(FxError::NonPositiveRate);
        }
        Ok(Rate {
            currency,
            date,
            per_eur,
            source,
        })
    }

    pub fn currency(&self) -> Currency {
        self.currency
    }

    pub fn date(&self) -> Date {
        self.date
    }

    pub fn per_eur(&self) -> Decimal {
        self.per_eur
    }

    pub fn source(&self) -> RateSource {
        self.source
    }

    pub fn freshness(&self, today: Date) -> Freshness {
        match self.source {
            RateSource::Manual { .. } => Freshness::Manual,
            RateSource::Ecb { .. } => {
                let age = self
                    .date
                    .until((Unit::Day, today))
                    .map_or(i64::MAX, |span| i64::from(span.get_days()));
                if age <= STALE_AFTER_DAYS {
                    Freshness::Fresh
                } else {
                    Freshness::Stale
                }
            }
        }
    }
}

/// The known rates, at most one per currency and date.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RateTable(BTreeMap<(Currency, Date), Rate>);

impl RateTable {
    /// Builds a table. When two rates share a currency and date, a manual rate
    /// wins over an ECB one (the user entered it deliberately), and otherwise the
    /// later one in `rates` wins.
    pub fn new(rates: impl IntoIterator<Item = Rate>) -> Self {
        let mut table = BTreeMap::new();
        for rate in rates {
            let key = (rate.currency, rate.date);
            let keep_existing = matches!(
                (table.get(&key).map(|r: &Rate| r.source), rate.source),
                (Some(RateSource::Manual { .. }), RateSource::Ecb { .. })
            );
            if !keep_existing {
                table.insert(key, rate);
            }
        }
        RateTable(table)
    }

    /// The rate to use for `currency` on `date`: the latest one on or before
    /// `date`, or, if there is none that old, the earliest one after it. The
    /// returned rate carries its own date, so callers can see how close it is.
    pub fn rate_for(&self, currency: Currency, date: Date) -> Option<&Rate> {
        let range = (currency, Date::MIN)..=(currency, Date::MAX);
        let on_or_before = self
            .0
            .range((currency, Date::MIN)..=(currency, date))
            .next_back();
        on_or_before
            .or_else(|| self.0.range(range).next())
            .map(|(_, rate)| rate)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Rate> {
        self.0.values()
    }
}

/// An amount converted to another currency, with the rates it relied on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Converted {
    pub amount: Fractional,
    /// The rates used, without duplicates. Empty if no conversion was needed.
    pub rates: Vec<Rate>,
}

/// Converts `amount` to `to` using the rates applicable on `date`.
pub fn convert(
    rates: &RateTable,
    amount: Fractional,
    to: Currency,
    date: Date,
) -> Result<Converted, FxError> {
    if amount.currency == to {
        return Ok(Converted {
            amount,
            rates: Vec::new(),
        });
    }
    let lookup = |currency| -> Result<Option<Rate>, FxError> {
        if currency == Currency::EUR {
            Ok(None)
        } else {
            rates
                .rate_for(currency, date)
                .copied()
                .map(Some)
                .ok_or(FxError::MissingRate(currency))
        }
    };
    let from_rate = lookup(amount.currency)?;
    let to_rate = lookup(to)?;

    // major(from) / per_eur(from) * per_eur(to) = major(to), and
    // major = minor / 10^exponent, so scale by the exponent difference too.
    let per_eur = |rate: Option<Rate>| rate.map_or(Decimal::ONE, |r| r.per_eur);
    let power = |exponent: u32| Decimal::from(10_i64.pow(exponent));
    let numerator = per_eur(to_rate)
        .checked_mul(power(to.exponent()))
        .ok_or(MoneyError::Overflow)?;
    let denominator = per_eur(from_rate)
        .checked_mul(power(amount.currency.exponent()))
        .ok_or(MoneyError::Overflow)?;
    let minor = amount
        .checked_scale(numerator, denominator)
        .ok_or(MoneyError::Overflow)?
        .minor;

    Ok(Converted {
        amount: Fractional::new(minor, to),
        rates: from_rate.into_iter().chain(to_rate).collect(),
    })
}

/// Converts a multi-currency total to a single currency.
pub fn convert_total(
    rates: &RateTable,
    total: &MultiCurrency,
    to: Currency,
    date: Date,
) -> Result<Converted, FxError> {
    total.iter().try_fold(
        Converted {
            amount: Fractional::zero(to),
            rates: Vec::new(),
        },
        |acc, amount| {
            let converted = convert(rates, amount, to, date)?;
            let mut used = acc.rates;
            for rate in converted.rates {
                if !used.contains(&rate) {
                    used.push(rate);
                }
            }
            Ok(Converted {
                amount: acc.amount.checked_add(converted.amount)?,
                rates: used,
            })
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;
    use rust_decimal_macros::dec;

    fn currency(code: &str) -> Currency {
        code.parse().unwrap()
    }

    fn ecb(code: &str, on: Date, per_eur: Decimal) -> Rate {
        let fetched_at = Timestamp::UNIX_EPOCH;
        Rate::new(currency(code), on, per_eur, RateSource::Ecb { fetched_at }).unwrap()
    }

    fn manual(code: &str, on: Date, per_eur: Decimal) -> Rate {
        let entered_at = Timestamp::UNIX_EPOCH;
        Rate::new(
            currency(code),
            on,
            per_eur,
            RateSource::Manual { entered_at },
        )
        .unwrap()
    }

    fn to(rates: &RateTable, minor: Decimal, from: &str, target: &str) -> Decimal {
        let amount = Fractional::new(minor, currency(from));
        convert(rates, amount, currency(target), date(2026, 10, 27))
            .unwrap()
            .amount
            .minor
    }

    fn table() -> RateTable {
        let day = date(2026, 10, 26);
        RateTable::new([
            ecb("USD", day, dec!(1.25)),
            ecb("GBP", day, dec!(0.8)),
            ecb("JPY", day, dec!(160)),
            ecb("KWD", day, dec!(0.4)),
        ])
    }

    #[test]
    fn same_currency_needs_no_rate() {
        let converted = convert(
            &RateTable::default(),
            Fractional::new(dec!(5), Currency::EUR),
            Currency::EUR,
            date(2026, 1, 1),
        )
        .unwrap();
        assert_eq!(converted.amount.minor, dec!(5));
        assert!(converted.rates.is_empty());
    }

    #[test]
    fn converts_to_and_from_eur() {
        let rates = table();
        assert_eq!(to(&rates, dec!(1000), "USD", "EUR"), dec!(800));
        assert_eq!(to(&rates, dec!(800), "EUR", "USD"), dec!(1000));
    }

    #[test]
    fn converts_between_other_currencies_through_eur() {
        // 10 USD = 8 EUR = 6.40 GBP.
        assert_eq!(to(&table(), dec!(1000), "USD", "GBP"), dec!(640));
    }

    #[test]
    fn accounts_for_different_minor_units() {
        let rates = table();
        // 1000 JPY (no minor unit) = 6.25 EUR = 625 cents.
        assert_eq!(to(&rates, dec!(1000), "JPY", "EUR"), dec!(625));
        // 10 EUR = 4 KWD = 4000 fils.
        assert_eq!(to(&rates, dec!(1000), "EUR", "KWD"), dec!(4000));
        // 1000 JPY = 6.25 EUR = 2.5 KWD = 2500 fils.
        assert_eq!(to(&rates, dec!(1000), "JPY", "KWD"), dec!(2500));
    }

    #[test]
    fn missing_rates_are_reported() {
        let amount = Fractional::new(dec!(100), currency("CHF"));
        assert_eq!(
            convert(&table(), amount, Currency::EUR, date(2026, 1, 1)),
            Err(FxError::MissingRate(currency("CHF")))
        );
    }

    #[test]
    fn picks_the_latest_rate_on_or_before_the_date() {
        let rates = RateTable::new([
            ecb("USD", date(2026, 1, 5), dec!(1.10)),
            ecb("USD", date(2026, 2, 5), dec!(1.20)),
        ]);
        let usd = currency("USD");
        assert_eq!(
            rates.rate_for(usd, date(2026, 2, 4)).unwrap().per_eur(),
            dec!(1.10)
        );
        assert_eq!(
            rates.rate_for(usd, date(2026, 2, 5)).unwrap().per_eur(),
            dec!(1.20)
        );
        assert_eq!(
            rates.rate_for(usd, date(2027, 1, 1)).unwrap().per_eur(),
            dec!(1.20)
        );
        // Before the first known rate, the earliest one is the best available.
        assert_eq!(
            rates.rate_for(usd, date(2025, 1, 1)).unwrap().per_eur(),
            dec!(1.10)
        );
        assert!(rates.rate_for(currency("GBP"), date(2026, 1, 5)).is_none());
    }

    #[test]
    fn manual_rates_win_over_ecb_rates_on_the_same_date() {
        let day = date(2026, 3, 2);
        let usd = currency("USD");
        let rates = RateTable::new([manual("USD", day, dec!(1.3)), ecb("USD", day, dec!(1.2))]);
        assert_eq!(rates.rate_for(usd, day).unwrap().per_eur(), dec!(1.3));
        let rates = RateTable::new([ecb("USD", day, dec!(1.2)), manual("USD", day, dec!(1.3))]);
        assert_eq!(rates.rate_for(usd, day).unwrap().per_eur(), dec!(1.3));
    }

    #[test]
    fn freshness_depends_on_source_and_age() {
        let rate = ecb("USD", date(2026, 4, 2), dec!(1.1));
        assert_eq!(rate.freshness(date(2026, 4, 2)), Freshness::Fresh);
        assert_eq!(rate.freshness(date(2026, 4, 7)), Freshness::Fresh);
        assert_eq!(rate.freshness(date(2026, 4, 8)), Freshness::Stale);
        let rate = manual("USD", date(2020, 1, 1), dec!(1.1));
        assert_eq!(rate.freshness(date(2026, 4, 8)), Freshness::Manual);
    }

    #[test]
    fn converts_a_multi_currency_total_and_lists_the_rates_used() {
        let total = crate::money::sum_by_currency([
            Fractional::new(dec!(1000), Currency::EUR),
            Fractional::new(dec!(1000), currency("USD")),
            Fractional::new(dec!(500), currency("USD")),
        ])
        .unwrap();
        let converted = convert_total(&table(), &total, Currency::EUR, date(2026, 10, 27)).unwrap();
        assert_eq!(converted.amount.minor, dec!(2200));
        assert_eq!(converted.rates.len(), 1);
        assert_eq!(converted.rates[0].currency(), currency("USD"));
    }

    #[test]
    fn rejects_invalid_rates() {
        let ts = Timestamp::UNIX_EPOCH;
        let day = date(2026, 1, 1);
        assert_eq!(
            Rate::new(
                Currency::EUR,
                day,
                dec!(1),
                RateSource::Ecb { fetched_at: ts }
            ),
            Err(FxError::BaseCurrency)
        );
        assert_eq!(
            Rate::new(
                currency("USD"),
                day,
                dec!(0),
                RateSource::Ecb { fetched_at: ts }
            ),
            Err(FxError::NonPositiveRate)
        );
        let json = r#"{"currency":"USD","date":"2026-01-01","per_eur":"-1","source":{"kind":"manual","entered_at":"2026-01-01T00:00:00Z"}}"#;
        assert!(serde_json::from_str::<Rate>(json).is_err());
    }
}
