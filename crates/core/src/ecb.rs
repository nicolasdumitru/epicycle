//! Reading exchange rates from the European Central Bank's Data API.
//!
//! The app asks for daily reference rates against the euro as CSV with
//! `detail=dataonly`, which gives one rate per line with no free-text columns:
//!
//! ```text
//! KEY,FREQ,CURRENCY,CURRENCY_DENOM,EXR_TYPE,EXR_SUFFIX,TIME_PERIOD,OBS_VALUE
//! EXR.D.USD.EUR.SP00.A,D,USD,EUR,SP00,A,2026-11-02,1.1216
//! ```
//!
//! See <https://data.ecb.europa.eu/help/api/data>. Fetching is the app's job;
//! this module only turns the response into [`Rate`]s.

use jiff::Timestamp;
use jiff::civil::Date;
use rust_decimal::Decimal;
use thiserror::Error;

use crate::fx::{Rate, RateSource};
use crate::money::Currency;

/// The URL for every currency's daily rate against the euro from `since` on.
pub fn rates_url(since: Date) -> String {
    format!(
        "https://data-api.ecb.europa.eu/service/data/EXR/D..EUR.SP00.A\
         ?startPeriod={since}&detail=dataonly&format=csvdata"
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EcbError {
    #[error("the ECB's response has no {0} column")]
    MissingColumn(&'static str),
    #[error("the ECB's response has a malformed line {0}")]
    Malformed(usize),
}

/// Parses a CSV response into rates, all fetched at `fetched_at`.
///
/// Lines without a value (the ECB leaves gaps on holidays) are skipped, and so
/// are currencies this app doesn't know or can't pay in. An empty response,
/// which the API gives when there is nothing new, is no rates rather than an
/// error.
pub fn parse_rates(csv: &str, fetched_at: Timestamp) -> Result<Vec<Rate>, EcbError> {
    let mut lines = csv.lines().filter(|l| !l.trim().is_empty());
    let Some(header) = lines.next() else {
        return Ok(Vec::new());
    };
    let columns: Vec<&str> = header.split(',').map(str::trim).collect();
    let column = |name: &'static str| {
        columns
            .iter()
            .position(|c| *c == name)
            .ok_or(EcbError::MissingColumn(name))
    };
    let (currency_col, denom_col) = (column("CURRENCY")?, column("CURRENCY_DENOM")?);
    let (date_col, value_col) = (column("TIME_PERIOD")?, column("OBS_VALUE")?);

    let mut rates = Vec::new();
    for (i, line) in lines.enumerate() {
        let fields: Vec<&str> = line.split(',').map(str::trim).collect();
        if fields.len() != columns.len() {
            // Line numbers count from 1 and include the header.
            return Err(EcbError::Malformed(i + 2));
        }
        if fields[denom_col] != "EUR" || fields[value_col].is_empty() {
            continue;
        }
        let date: Date = fields[date_col]
            .parse()
            .map_err(|_| EcbError::Malformed(i + 2))?;
        let Ok(per_eur) = fields[value_col].parse::<Decimal>() else {
            // "NaN" marks a missing observation.
            continue;
        };
        let Ok(currency) = fields[currency_col].parse::<Currency>() else {
            continue;
        };
        if let Ok(rate) = Rate::new(currency, date, per_eur, RateSource::Ecb { fetched_at }) {
            rates.push(rate);
        }
    }
    Ok(rates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;
    use rust_decimal_macros::dec;

    const SAMPLE: &str = include_str!("../testdata/ecb-sample.csv");

    fn now() -> Timestamp {
        Timestamp::from_second(1_793_700_000).unwrap()
    }

    #[test]
    fn reads_a_real_response() {
        let rates = parse_rates(SAMPLE, now()).unwrap();
        assert_eq!(rates.len(), 8);
        let ron = rates
            .iter()
            .find(|r| r.currency().code() == "RON" && r.date() == date(2026, 11, 2))
            .unwrap();
        assert_eq!(ron.per_eur(), dec!(5.3405));
        assert_eq!(ron.source(), RateSource::Ecb { fetched_at: now() });
    }

    #[test]
    fn skips_gaps_and_unknown_currencies() {
        let csv = "KEY,FREQ,CURRENCY,CURRENCY_DENOM,EXR_TYPE,EXR_SUFFIX,TIME_PERIOD,OBS_VALUE\n\
                   EXR.D.USD.EUR.SP00.A,D,USD,EUR,SP00,A,2026-11-02,\n\
                   EXR.D.USD.EUR.SP00.A,D,USD,EUR,SP00,A,2026-11-03,NaN\n\
                   EXR.D.XYZ.EUR.SP00.A,D,XYZ,EUR,SP00,A,2026-11-02,2.5\n\
                   EXR.D.USD.EUR.SP00.A,D,USD,EUR,SP00,A,2026-11-04,1.12\n";
        let rates = parse_rates(csv, now()).unwrap();
        assert_eq!(rates.len(), 1);
        assert_eq!(rates[0].date(), date(2026, 11, 4));
    }

    #[test]
    fn nothing_new_is_not_an_error() {
        assert_eq!(parse_rates("", now()), Ok(Vec::new()));
    }

    #[test]
    fn explains_unexpected_responses() {
        assert_eq!(
            parse_rates("<html>Service unavailable</html>\n", now()),
            Err(EcbError::MissingColumn("CURRENCY"))
        );
        let short = "CURRENCY,CURRENCY_DENOM,TIME_PERIOD,OBS_VALUE\nUSD,EUR,2026-11-02\n";
        assert_eq!(parse_rates(short, now()), Err(EcbError::Malformed(2)));
    }

    #[test]
    fn asks_for_rates_from_a_date() {
        assert!(rates_url(date(2026, 10, 20)).contains("startPeriod=2026-10-20"));
    }
}
