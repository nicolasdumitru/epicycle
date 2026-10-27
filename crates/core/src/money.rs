//! Currencies and amounts of money.
//!
//! Stored amounts are integers in a currency's minor unit (cents for EUR), so they
//! are exact. Values derived from them, such as the monthly equivalent of a weekly
//! price, are [`Fractional`] and stay exact decimals until display. Amounts in
//! different currencies can't be added without an explicit conversion.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// An ISO 4217 currency with a defined number of minor units.
///
/// Codes without minor units (precious metals, XDR, test codes) are rejected,
/// because a subscription can't be billed in them.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Currency(iso_currency::Currency);

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CurrencyError {
    #[error("unknown currency code {0:?}")]
    Unknown(String),
    #[error("currency {0} has no minor unit and can't be used for payments")]
    NoMinorUnit(String),
}

impl Currency {
    pub const EUR: Currency = Currency(iso_currency::Currency::EUR);

    /// The three-letter ISO 4217 code, e.g. `"EUR"`.
    pub fn code(self) -> &'static str {
        self.0.code()
    }

    /// Number of decimal places of the minor unit: 2 for EUR, 0 for JPY, 3 for KWD.
    pub fn exponent(self) -> u32 {
        // Checked in `from_str`.
        self.0.exponent().map_or(0, u32::from)
    }
}

impl FromStr for Currency {
    type Err = CurrencyError;

    fn from_str(code: &str) -> Result<Self, Self::Err> {
        let currency = iso_currency::Currency::from_code(code)
            .ok_or_else(|| CurrencyError::Unknown(code.to_owned()))?;
        match currency.exponent() {
            Some(_) => Ok(Currency(currency)),
            None => Err(CurrencyError::NoMinorUnit(code.to_owned())),
        }
    }
}

impl TryFrom<String> for Currency {
    type Error = CurrencyError;

    fn try_from(code: String) -> Result<Self, Self::Error> {
        code.parse()
    }
}

impl From<Currency> for String {
    fn from(currency: Currency) -> Self {
        currency.code().to_owned()
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl fmt::Debug for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum MoneyError {
    #[error("can't combine {0} and {1} without a conversion")]
    CurrencyMismatch(Currency, Currency),
    #[error("amount out of range")]
    Overflow,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseMoneyError {
    #[error("{0:?} is not an amount")]
    NotANumber(String),
    #[error("{currency} amounts have at most {places} decimal places")]
    TooManyDecimals { currency: Currency, places: u32 },
    #[error("amount out of range")]
    Overflow,
}

/// An exact amount: an integer number of minor units in a currency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Money {
    pub minor: i64,
    pub currency: Currency,
}

impl Money {
    pub fn new(minor: i64, currency: Currency) -> Self {
        Money { minor, currency }
    }

    pub fn zero(currency: Currency) -> Self {
        Money::new(0, currency)
    }

    pub fn checked_add(self, other: Money) -> Result<Money, MoneyError> {
        if self.currency != other.currency {
            return Err(MoneyError::CurrencyMismatch(self.currency, other.currency));
        }
        let minor = self
            .minor
            .checked_add(other.minor)
            .ok_or(MoneyError::Overflow)?;
        Ok(Money::new(minor, self.currency))
    }

    /// The amount in major units, e.g. `12.34` for 1234 cents.
    pub fn to_major(self) -> Decimal {
        Decimal::new(self.minor, self.currency.exponent())
    }

    /// Parses an amount typed in major units: `"12.99"` EUR is 1299 cents.
    ///
    /// A comma is accepted as the decimal separator when there is no dot, since
    /// many locales (and their numeric keypads) use one. Thousands separators are
    /// not accepted. More decimals than the currency has is an error rather than
    /// a silent rounding.
    pub fn parse_major(text: &str, currency: Currency) -> Result<Money, ParseMoneyError> {
        let trimmed = text.trim();
        let normalized = if trimmed.contains('.') {
            trimmed.to_owned()
        } else {
            trimmed.replacen(',', ".", 1)
        };
        let value = Decimal::from_str(&normalized)
            .map_err(|_| ParseMoneyError::NotANumber(text.to_owned()))?
            .normalize();
        let places = currency.exponent();
        if value.scale() > places {
            return Err(ParseMoneyError::TooManyDecimals { currency, places });
        }
        let minor = value
            .checked_mul(Decimal::from(10_i64.pow(places)))
            .and_then(|minor| i64::try_from(minor).ok())
            .ok_or(ParseMoneyError::Overflow)?;
        Ok(Money::new(minor, currency))
    }
}

impl From<Money> for Fractional {
    fn from(money: Money) -> Self {
        Fractional::new(Decimal::from(money.minor), money.currency)
    }
}

/// An amount that may include fractions of a minor unit, produced by
/// calculations such as averages, unit conversions and exchange rates.
///
/// Keeping these exact and rounding only for display avoids accumulating
/// rounding errors when they are summed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Fractional {
    /// Value in minor units, e.g. `433.3333…` cents.
    pub minor: Decimal,
    pub currency: Currency,
}

impl Fractional {
    pub fn new(minor: Decimal, currency: Currency) -> Self {
        Fractional { minor, currency }
    }

    pub fn zero(currency: Currency) -> Self {
        Fractional::new(Decimal::ZERO, currency)
    }

    pub fn checked_add(self, other: Fractional) -> Result<Fractional, MoneyError> {
        if self.currency != other.currency {
            return Err(MoneyError::CurrencyMismatch(self.currency, other.currency));
        }
        let minor = self
            .minor
            .checked_add(other.minor)
            .ok_or(MoneyError::Overflow)?;
        Ok(Fractional::new(minor, self.currency))
    }

    /// Multiplies by `numerator / denominator`, dividing last to keep precision.
    pub fn checked_scale(self, numerator: Decimal, denominator: Decimal) -> Option<Fractional> {
        let minor = self
            .minor
            .checked_mul(numerator)?
            .checked_div(denominator)?;
        Some(Fractional::new(minor, self.currency))
    }

    /// Rounds to whole minor units, half to even ("banker's rounding"), so that
    /// rounding many values doesn't drift systematically up or down.
    pub fn round(self) -> Result<Money, MoneyError> {
        let rounded = self
            .minor
            .round_dp_with_strategy(0, RoundingStrategy::MidpointNearestEven);
        let minor = i64::try_from(rounded).map_err(|_| MoneyError::Overflow)?;
        Ok(Money::new(minor, self.currency))
    }
}

/// Amounts in several currencies, kept apart per currency.
///
/// This is the result of summing subscriptions billed in different currencies.
/// Converting it to a single currency needs exchange rates (see [`crate::fx`]).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiCurrency(BTreeMap<Currency, Decimal>);

impl MultiCurrency {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn checked_add(mut self, amount: impl Into<Fractional>) -> Result<Self, MoneyError> {
        let amount = amount.into();
        let entry = self.0.entry(amount.currency).or_insert(Decimal::ZERO);
        *entry = entry
            .checked_add(amount.minor)
            .ok_or(MoneyError::Overflow)?;
        Ok(self)
    }

    /// The total in one currency; zero if nothing was added in it.
    pub fn get(&self, currency: Currency) -> Fractional {
        Fractional::new(self.0.get(&currency).copied().unwrap_or_default(), currency)
    }

    /// The total in each currency that has one, in currency-code order.
    pub fn iter(&self) -> impl Iterator<Item = Fractional> + '_ {
        self.0
            .iter()
            .map(|(&currency, &minor)| Fractional::new(minor, currency))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Multiplies every total by `numerator / denominator`.
    pub fn checked_scale(&self, numerator: Decimal, denominator: Decimal) -> Option<Self> {
        self.iter()
            .map(|amount| amount.checked_scale(numerator, denominator))
            .try_fold(MultiCurrency::new(), |acc, scaled| {
                acc.checked_add(scaled?).ok()
            })
    }
}

/// Sums amounts per currency.
pub fn sum_by_currency<I>(amounts: I) -> Result<MultiCurrency, MoneyError>
where
    I: IntoIterator,
    I::Item: Into<Fractional>,
{
    amounts
        .into_iter()
        .try_fold(MultiCurrency::new(), MultiCurrency::checked_add)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn currency(code: &str) -> Currency {
        code.parse().unwrap()
    }

    #[test]
    fn parses_currencies_with_their_minor_units() {
        assert_eq!(currency("EUR").exponent(), 2);
        assert_eq!(currency("JPY").exponent(), 0);
        assert_eq!(currency("KWD").exponent(), 3);
    }

    #[test]
    fn rejects_unknown_codes_and_codes_without_minor_units() {
        assert_eq!(
            "EUX".parse::<Currency>(),
            Err(CurrencyError::Unknown("EUX".into()))
        );
        assert_eq!(
            "eur".parse::<Currency>(),
            Err(CurrencyError::Unknown("eur".into()))
        );
        assert_eq!(
            "XAU".parse::<Currency>(),
            Err(CurrencyError::NoMinorUnit("XAU".into()))
        );
    }

    #[test]
    fn currency_serializes_as_its_code() {
        let json = serde_json::to_string(&Currency::EUR).unwrap();
        assert_eq!(json, r#""EUR""#);
        assert_eq!(
            serde_json::from_str::<Currency>(&json).unwrap(),
            Currency::EUR
        );
        assert!(serde_json::from_str::<Currency>(r#""XAU""#).is_err());
    }

    #[test]
    fn to_major_uses_the_currency_exponent() {
        assert_eq!(Money::new(1234, Currency::EUR).to_major(), dec!(12.34));
        assert_eq!(Money::new(1234, currency("JPY")).to_major(), dec!(1234));
        assert_eq!(Money::new(1234, currency("KWD")).to_major(), dec!(1.234));
    }

    #[test]
    fn adding_different_currencies_is_an_error() {
        let usd = currency("USD");
        assert_eq!(
            Money::new(1, Currency::EUR).checked_add(Money::new(1, usd)),
            Err(MoneyError::CurrencyMismatch(Currency::EUR, usd))
        );
    }

    #[test]
    fn parses_amounts_typed_in_major_units() {
        let parse = |text, code| Money::parse_major(text, currency(code)).map(|m| m.minor);
        assert_eq!(parse("12.99", "EUR"), Ok(1299));
        assert_eq!(parse(" 12,99 ", "EUR"), Ok(1299));
        assert_eq!(parse("12", "EUR"), Ok(1200));
        assert_eq!(parse("12.5", "EUR"), Ok(1250));
        assert_eq!(parse("12.990", "EUR"), Ok(1299));
        assert_eq!(parse("1500", "JPY"), Ok(1500));
        assert_eq!(parse("1.234", "KWD"), Ok(1234));
        assert_eq!(
            parse("12.999", "EUR"),
            Err(ParseMoneyError::TooManyDecimals {
                currency: Currency::EUR,
                places: 2
            })
        );
        assert_eq!(
            parse("1.5", "JPY"),
            Err(ParseMoneyError::TooManyDecimals {
                currency: currency("JPY"),
                places: 0
            })
        );
        assert_eq!(
            parse("1,234.5", "EUR"),
            Err(ParseMoneyError::NotANumber("1,234.5".into()))
        );
        assert_eq!(
            parse("twelve", "EUR"),
            Err(ParseMoneyError::NotANumber("twelve".into()))
        );
        assert_eq!(
            parse("", "EUR"),
            Err(ParseMoneyError::NotANumber("".into()))
        );
        assert_eq!(
            parse("99999999999999999999", "EUR"),
            Err(ParseMoneyError::Overflow)
        );
    }

    #[test]
    fn adding_detects_overflow() {
        let max = Money::new(i64::MAX, Currency::EUR);
        assert_eq!(
            max.checked_add(Money::new(1, Currency::EUR)),
            Err(MoneyError::Overflow)
        );
    }

    #[test]
    fn rounds_half_to_even() {
        let round = |minor| Fractional::new(minor, Currency::EUR).round().unwrap().minor;
        assert_eq!(round(dec!(0.5)), 0);
        assert_eq!(round(dec!(1.5)), 2);
        assert_eq!(round(dec!(2.5)), 2);
        assert_eq!(round(dec!(2.5001)), 3);
        assert_eq!(round(dec!(-2.5)), -2);
    }

    #[test]
    fn sums_per_currency_without_mixing() {
        let usd = currency("USD");
        let total = sum_by_currency([
            Money::new(1000, Currency::EUR),
            Money::new(250, usd),
            Money::new(99, Currency::EUR),
        ])
        .unwrap();
        assert_eq!(total.get(Currency::EUR).minor, dec!(1099));
        assert_eq!(total.get(usd).minor, dec!(250));
        assert_eq!(total.get(currency("GBP")).minor, dec!(0));
        assert_eq!(total.iter().count(), 2);
    }
}
