//! Backup files: the whole change history and the settings, as versioned JSON.
//!
//! The current state isn't stored separately: it is the latest revision of each
//! subscription. See docs/DECISIONS.md, "Backups".

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::error::Category;
use thiserror::Error;
use uuid::Uuid;

use std::collections::BTreeMap;

use jiff::civil::Date;

use crate::fx::{Rate, RateSource};
use crate::history::{History, HistoryError, Revision};
use crate::money::Currency;

/// Identifies Epicycle backups among other JSON files.
pub const FORMAT: &str = "epicycle-backup";

/// The format version this code writes. Increase it on any change to the
/// format, and keep reading every older version.
///
/// - 1: the first format.
/// - 2: subscriptions may have `notes`. Otherwise the same as 1, so version 1
///   files are read as version 2 files without notes. The version went up so
///   that older apps refuse these files instead of dropping the notes.
/// - 3: adds `settings.favorite_currencies` and `manual_rates`. Older files
///   are read as having none of either.
/// - 4: plans may have an `estimate` (bounds and how the expected amount is
///   chosen). Older files have exact amounts only.
pub const VERSION: u64 = 4;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub reporting_currency: Currency,
    /// Currencies to switch between quickly, in the user's order, without
    /// duplicates.
    #[serde(default)]
    pub favorite_currencies: Vec<Currency>,
}

impl Settings {
    /// Settings with duplicate favourites removed, keeping the first of each.
    pub fn normalized(mut self) -> Self {
        let mut seen = Vec::new();
        self.favorite_currencies.retain(|c| {
            let new = !seen.contains(c);
            seen.push(*c);
            new
        });
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backup {
    pub exported_at: Timestamp,
    /// The device that made the backup.
    pub device_id: Uuid,
    pub settings: Settings,
    pub history: History,
    /// Exchange rates the user entered. ECB rates aren't backed up: they can
    /// be fetched again.
    pub manual_rates: Vec<Rate>,
}

#[derive(Debug, Error)]
pub enum BackupError {
    #[error("the file is not valid JSON ({0})")]
    NotJson(serde_json::Error),
    #[error("the file is not an Epicycle backup")]
    NotABackup,
    #[error(
        "this backup was made by a newer version of Epicycle (format version {0}); \
         update the app to import it"
    )]
    Newer(u64),
    #[error("backup format version {0} is not supported")]
    Unsupported(u64),
    #[error("the backup is invalid: {0}")]
    Invalid(serde_json::Error),
    #[error("the backup's history is inconsistent: {0}")]
    History(#[from] HistoryError),
    #[error("the backup's exchange rate for {0} on {1} is not one entered by hand")]
    NotManualRate(Currency, Date),
    #[error("the backup has two exchange rates for {0} on {1}")]
    DuplicateRate(Currency, Date),
}

/// The fields every version has, read first to decide how to read the rest.
#[derive(Deserialize)]
struct Header {
    format: String,
    version: u64,
}

#[derive(Serialize)]
struct FileRef<'a> {
    format: &'static str,
    version: u64,
    exported_at: Timestamp,
    device_id: Uuid,
    settings: &'a Settings,
    revisions: &'a [Revision],
    manual_rates: &'a [Rate],
}

/// Every version so far reads into this: fields added later default to empty.
#[derive(Deserialize)]
struct File {
    exported_at: Timestamp,
    device_id: Uuid,
    settings: Settings,
    revisions: Vec<Revision>,
    #[serde(default)]
    manual_rates: Vec<Rate>,
}

impl Backup {
    /// Revisions are written parents first, so the output is deterministic.
    pub fn to_json(&self) -> String {
        let file = FileRef {
            format: FORMAT,
            version: VERSION,
            exported_at: self.exported_at,
            device_id: self.device_id,
            settings: &self.settings,
            revisions: self.history.revisions(),
            manual_rates: &self.manual_rates,
        };
        let mut json = serde_json::to_string_pretty(&file).expect("backups serialize");
        json.push('\n');
        json
    }

    /// Reads and fully validates a backup, without touching any stored data.
    pub fn from_json(json: &str) -> Result<Backup, BackupError> {
        let header: Header = serde_json::from_str(json).map_err(|e| match e.classify() {
            Category::Data => BackupError::NotABackup,
            _ => BackupError::NotJson(e),
        })?;
        if header.format != FORMAT {
            return Err(BackupError::NotABackup);
        }
        let file: File = match header.version {
            1..=4 => serde_json::from_str(json).map_err(BackupError::Invalid)?,
            v if v > VERSION => return Err(BackupError::Newer(v)),
            v => return Err(BackupError::Unsupported(v)),
        };
        let mut seen = Vec::new();
        for rate in &file.manual_rates {
            let key = (rate.currency(), rate.date());
            if !matches!(rate.source(), RateSource::Manual { .. }) {
                return Err(BackupError::NotManualRate(key.0, key.1));
            }
            if seen.contains(&key) {
                return Err(BackupError::DuplicateRate(key.0, key.1));
            }
            seen.push(key);
        }
        Ok(Backup {
            exported_at: file.exported_at,
            device_id: file.device_id,
            settings: file.settings.normalized(),
            history: History::new(file.revisions)?,
            manual_rates: file.manual_rates,
        })
    }
}

/// How an import treats manual exchange rates, which have no history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateImport {
    /// Add the backup's rates; where both have one for the same currency and
    /// day, keep this device's (Merge).
    KeepLocal,
    /// Add the backup's rates; where both have one, take the backup's. This
    /// device's other rates stay (Replace, which loses nothing).
    PreferBackup,
    /// Exactly the backup's rates (replacing the whole history).
    Exact,
}

/// What importing manual rates does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateChanges {
    /// The manual rates afterwards.
    pub rates: Vec<Rate>,
    /// Rates added or changed.
    pub changed: usize,
    /// Rates on this device that are gone afterwards.
    pub removed: usize,
}

pub fn import_manual_rates(local: &[Rate], incoming: &[Rate], how: RateImport) -> RateChanges {
    let key = |r: &Rate| (r.currency(), r.date());
    let mut result: BTreeMap<(Currency, Date), Rate> = match how {
        RateImport::Exact => BTreeMap::new(),
        _ => local.iter().map(|r| (key(r), *r)).collect(),
    };
    for rate in incoming {
        if how != RateImport::KeepLocal || !result.contains_key(&key(rate)) {
            result.insert(key(rate), *rate);
        }
    }
    let before: BTreeMap<_, _> = local.iter().map(|r| (key(r), *r)).collect();
    RateChanges {
        changed: result
            .iter()
            .filter(|(k, r)| before.get(*k) != Some(*r))
            .count(),
        removed: before.keys().filter(|k| !result.contains_key(*k)).count(),
        rates: result.into_values().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::tests::Builder;
    use crate::subscription::tests::{monthly_plan, subscription};
    use jiff::civil::date;

    /// Backups in each format version, kept unchanged so that changes to the
    /// format are noticed: they must keep importing, and exporting the latest
    /// must give it back.
    const V1: &str = include_str!("../testdata/backup-v1.json");
    const V2: &str = include_str!("../testdata/backup-v2.json");
    const V3: &str = include_str!("../testdata/backup-v3.json");
    const V4: &str = include_str!("../testdata/backup-v4.json");

    fn sample() -> Backup {
        let mut b = Builder::new();
        let s = subscription(date(2026, 1, 15), monthly_plan(date(2026, 1, 15), 999));
        let r1 = b.create(&s);
        let r2 = b.edit(&r1, &s.renamed("Renamed").unwrap());
        let r3 = b.delete(&r2);
        Backup {
            exported_at: Timestamp::from_second(1_800_000_000).unwrap(),
            device_id: Uuid::from_u128(7),
            settings: Settings {
                reporting_currency: Currency::EUR,
                favorite_currencies: vec!["RON".parse().unwrap(), Currency::EUR],
            },
            history: History::new([r3, r1, r2]).unwrap(),
            manual_rates: vec![manual("RON", 1, "5.1")],
        }
    }

    #[test]
    fn round_trips() {
        let backup = sample();
        assert_eq!(Backup::from_json(&backup.to_json()).unwrap(), backup);
    }

    #[test]
    fn version_4_format_is_stable() {
        let backup = Backup::from_json(V4).unwrap();
        let lessons = backup
            .history
            .current()
            .find(|s| s.name() == "Language lessons")
            .unwrap();
        let plan = lessons.plans()[0].value;
        assert_eq!(plan.amount, 12000);
        assert_eq!(plan.range(), (10000, Some(14000)));
        let written: serde_json::Value = serde_json::from_str(&backup.to_json()).unwrap();
        let original: serde_json::Value = serde_json::from_str(V4).unwrap();
        assert_eq!(written, original);
    }

    /// An older file (version 1 or 2), exported again: the same data in the
    /// current version, with no favourites and no manual rates.
    fn upgraded(old: &str) -> serde_json::Value {
        let mut expected: serde_json::Value = serde_json::from_str(old).unwrap();
        expected["version"] = VERSION.into();
        expected["settings"]["favorite_currencies"] = serde_json::json!([]);
        expected["manual_rates"] = serde_json::json!([]);
        expected
    }

    #[test]
    fn older_versions_still_import() {
        let v3 = Backup::from_json(V3).unwrap();
        assert!(
            v3.history
                .current()
                .all(|s| s.plans().iter().all(|p| p.value.estimate.is_none()))
        );
        let mut expected: serde_json::Value = serde_json::from_str(V3).unwrap();
        expected["version"] = VERSION.into();
        let written: serde_json::Value = serde_json::from_str(&v3.to_json()).unwrap();
        assert_eq!(written, expected);

        let v2 = Backup::from_json(V2).unwrap();
        assert!(v2.history.current().any(|s| !s.notes().is_empty()));
        let written: serde_json::Value = serde_json::from_str(&v2.to_json()).unwrap();
        assert_eq!(written, upgraded(V2));

        let v1 = Backup::from_json(V1).unwrap();
        assert!(v1.history.current().all(|s| s.notes().is_empty()));
        let written: serde_json::Value = serde_json::from_str(&v1.to_json()).unwrap();
        assert_eq!(written, upgraded(V1));
    }

    #[test]
    fn checks_manual_rates() {
        let mut file: serde_json::Value = serde_json::from_str(V3).unwrap();
        file["manual_rates"][0]["source"] =
            serde_json::json!({"kind": "ecb", "fetched_at": "2026-10-01T08:00:00Z"});
        assert!(matches!(
            error(&file.to_string()),
            BackupError::NotManualRate(..)
        ));

        let mut file: serde_json::Value = serde_json::from_str(V3).unwrap();
        file["manual_rates"][1] = file["manual_rates"][0].clone();
        assert!(matches!(
            error(&file.to_string()),
            BackupError::DuplicateRate(..)
        ));

        let mut file: serde_json::Value = serde_json::from_str(V3).unwrap();
        file["manual_rates"][0]["per_eur"] = "-1".into();
        assert!(matches!(error(&file.to_string()), BackupError::Invalid(_)));
    }

    #[test]
    fn drops_duplicate_favorites() {
        let mut file: serde_json::Value = serde_json::from_str(V3).unwrap();
        file["settings"]["favorite_currencies"] = serde_json::json!(["RON", "EUR", "RON"]);
        let backup = Backup::from_json(&file.to_string()).unwrap();
        assert_eq!(backup.settings.favorite_currencies.len(), 2);
    }

    fn manual(currency: &str, day: i8, per_eur: &str) -> Rate {
        Rate::new(
            currency.parse().unwrap(),
            date(2026, 11, day),
            per_eur.parse().unwrap(),
            RateSource::Manual {
                entered_at: Timestamp::UNIX_EPOCH,
            },
        )
        .unwrap()
    }

    #[test]
    fn imports_manual_rates_by_mode() {
        let local = [manual("RON", 1, "5.0"), manual("MDL", 1, "19")];
        let incoming = [manual("RON", 1, "5.1"), manual("USD", 2, "1.1")];
        let per_eur = |c: &RateChanges, code: &str| {
            c.rates
                .iter()
                .find(|r| r.currency().code() == code)
                .map(|r| r.per_eur().to_string())
        };

        let merge = import_manual_rates(&local, &incoming, RateImport::KeepLocal);
        assert_eq!(per_eur(&merge, "RON").as_deref(), Some("5.0"));
        assert_eq!((merge.rates.len(), merge.changed, merge.removed), (3, 1, 0));

        let replace = import_manual_rates(&local, &incoming, RateImport::PreferBackup);
        assert_eq!(per_eur(&replace, "RON").as_deref(), Some("5.1"));
        assert_eq!(
            (replace.rates.len(), replace.changed, replace.removed),
            (3, 2, 0)
        );

        let exact = import_manual_rates(&local, &incoming, RateImport::Exact);
        assert_eq!(per_eur(&exact, "MDL"), None);
        assert_eq!((exact.rates.len(), exact.changed, exact.removed), (2, 2, 1));
    }

    fn error(json: &str) -> BackupError {
        Backup::from_json(json).unwrap_err()
    }

    fn with(field: &str, value: serde_json::Value) -> String {
        let mut file: serde_json::Value = serde_json::from_str(V1).unwrap();
        file[field] = value;
        file.to_string()
    }

    #[test]
    fn explains_what_is_wrong() {
        assert!(matches!(error("{ not json"), BackupError::NotJson(_)));
        assert!(matches!(error("[1, 2]"), BackupError::NotABackup));
        assert!(matches!(error(r#"{"name": "x"}"#), BackupError::NotABackup));
        assert!(matches!(
            error(&with("format", "something-else".into())),
            BackupError::NotABackup
        ));
        assert!(matches!(
            error(&with("version", 5.into())),
            BackupError::Newer(5)
        ));
        assert!(matches!(
            error(&with("version", 0.into())),
            BackupError::Unsupported(0)
        ));
        assert!(matches!(
            error(&with(
                "settings",
                serde_json::json!({"reporting_currency": "XYZ"})
            )),
            BackupError::Invalid(_)
        ));
    }

    #[test]
    fn rejects_invalid_subscriptions() {
        let mut file: serde_json::Value = serde_json::from_str(V1).unwrap();
        file["revisions"][0]["subscription"]["name"] = "  ".into();
        let message = error(&file.to_string()).to_string();
        assert!(message.contains("name must not be empty"), "{message}");
    }

    #[test]
    fn rejects_inconsistent_history() {
        let mut file: serde_json::Value = serde_json::from_str(V1).unwrap();
        // Drop the first revision, which the second one names as its parent.
        file["revisions"].as_array_mut().unwrap().remove(0);
        assert!(matches!(
            error(&file.to_string()),
            BackupError::History(HistoryError::MissingParent(..))
        ));
    }
}
