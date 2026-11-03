//! Keeping ECB exchange rates up to date: when to fetch, and fetching.
//!
//! Rates are refreshed opportunistically, when the app asks and the network is
//! available. Failing to fetch is never an error the user has to deal with:
//! the cached or manual rates keep working, and their dates show how old they
//! are.

use std::time::Duration;

use epicycle_core::ecb;
use epicycle_core::fx::Rate;
use jiff::civil::Date;
use jiff::{SignedDuration, Timestamp, ToSpan};
use thiserror::Error;

use tauri::{AppHandle, Emitter, Manager};

use crate::db::{Db, DbError};

/// How many days of rates to fetch the first time.
const FIRST_FETCH_DAYS: i64 = 14;

/// Don't ask the ECB again within this long of the last fetch that brought
/// new rates. It publishes once a day, around 16:00 CET.
const MIN_INTERVAL: SignedDuration = SignedDuration::from_hours(1);

#[derive(Debug, Error)]
pub enum RefreshError {
    #[error("couldn't reach the ECB: {0}")]
    Network(#[from] reqwest::Error),
    #[error(transparent)]
    Ecb(#[from] ecb::EcbError),
    #[error(transparent)]
    Db(#[from] DbError),
}

/// The first date to fetch rates from, or `None` if the cached rates are as
/// new as they can be. `last` is the newest cached ECB rate's date and when it
/// was fetched.
pub fn fetch_since(last: Option<(Date, Timestamp)>, today: Date, now: Timestamp) -> Option<Date> {
    match last {
        None => today.checked_sub(FIRST_FETCH_DAYS.days()).ok(),
        Some((latest, _)) if latest >= today => None,
        Some((_, fetched_at)) if now.duration_since(fetched_at) < MIN_INTERVAL => None,
        // Never after today: the API rejects start dates in the future.
        Some((latest, _)) => latest.tomorrow().ok().map(|d| d.min(today)),
    }
}

async fn fetch(since: Date, now: Timestamp) -> Result<Vec<Rate>, RefreshError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent(concat!("Epicycle/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let response = client.get(ecb::rates_url(since)).send().await?;
    // The API answers "no results" with 404 on some versions; that just means
    // nothing new.
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Vec::new());
    }
    let body = response.error_for_status()?.text().await?;
    Ok(ecb::parse_rates(&body, now)?)
}

/// How often the background task checks whether new rates are due. Most
/// checks make no request: see [`fetch_since`].
const CHECK_EVERY: Duration = Duration::from_secs(60 * 60);

/// Sent to the UI whenever new rates were stored, so totals can be redone.
pub const UPDATED_EVENT: &str = "rates-updated";

/// Keeps rates current while the app runs: checks at start-up and then every
/// [`CHECK_EVERY`]. Failures (usually being offline) are retried at the next
/// check; cached and manual rates keep working meanwhile.
pub fn spawn_refresher(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let db = app.state::<Db>();
            if let Ok(stored) = refresh(&db, crate::commands::today()).await
                && stored > 0
            {
                // Nothing to do if no window is listening.
                let _ = app.emit(UPDATED_EVENT, stored);
            }
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

/// Fetches new ECB rates if the cache is out of date. Returns how many rates
/// were stored.
pub async fn refresh(db: &Db, today: Date) -> Result<usize, RefreshError> {
    let now = Timestamp::now();
    let Some(since) = fetch_since(db.last_ecb_fetch().await?, today, now) else {
        return Ok(0);
    };
    let rates = fetch(since, now).await?;
    db.save_rates(&rates).await?;
    Ok(rates.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn at(hour: i8) -> Timestamp {
        date(2026, 11, 3)
            .at(hour, 0, 0, 0)
            .to_zoned(jiff::tz::TimeZone::UTC)
            .unwrap()
            .timestamp()
    }

    #[test]
    fn fetches_two_weeks_the_first_time() {
        assert_eq!(
            fetch_since(None, date(2026, 11, 3), at(12)),
            Some(date(2026, 10, 20))
        );
    }

    #[test]
    fn fetches_from_the_day_after_the_newest_rate() {
        let last = Some((date(2026, 10, 30), at(1)));
        assert_eq!(
            fetch_since(last, date(2026, 11, 3), at(12)),
            Some(date(2026, 10, 31))
        );
    }

    #[test]
    fn does_not_fetch_when_up_to_date_or_recently_fetched() {
        let today = date(2026, 11, 3);
        assert_eq!(fetch_since(Some((today, at(1))), today, at(12)), None);
        let yesterday = date(2026, 11, 2);
        assert_eq!(fetch_since(Some((yesterday, at(12))), today, at(12)), None);
    }

    /// Fetches from the real ECB API. Run with `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore = "needs the network"]
    async fn fetches_real_rates() {
        let db = Db::open_in_memory().await.unwrap();
        let today = jiff::Zoned::now().date();
        let stored = refresh(&db, today).await.unwrap();
        assert!(stored > 0);
        assert!(
            db.rates()
                .await
                .unwrap()
                .iter()
                .any(|r| r.currency().code() == "USD")
        );
        // Right after, nothing is fetched again.
        assert_eq!(refresh(&db, today).await.unwrap(), 0);
    }
}
