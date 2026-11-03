//! Backup files: the whole change history and the settings, as versioned JSON.
//!
//! The current state isn't stored separately: it is the latest revision of each
//! subscription. See docs/DECISIONS.md, "Backups".

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::error::Category;
use thiserror::Error;
use uuid::Uuid;

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
pub const VERSION: u64 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub reporting_currency: Currency,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backup {
    pub exported_at: Timestamp,
    /// The device that made the backup.
    pub device_id: Uuid,
    pub settings: Settings,
    pub history: History,
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
}

#[derive(Deserialize)]
struct File {
    exported_at: Timestamp,
    device_id: Uuid,
    settings: Settings,
    revisions: Vec<Revision>,
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
            1 | 2 => serde_json::from_str(json).map_err(BackupError::Invalid)?,
            v if v > VERSION => return Err(BackupError::Newer(v)),
            v => return Err(BackupError::Unsupported(v)),
        };
        Ok(Backup {
            exported_at: file.exported_at,
            device_id: file.device_id,
            settings: file.settings,
            history: History::new(file.revisions)?,
        })
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
            },
            history: History::new([r3, r1, r2]).unwrap(),
        }
    }

    #[test]
    fn round_trips() {
        let backup = sample();
        assert_eq!(Backup::from_json(&backup.to_json()).unwrap(), backup);
    }

    #[test]
    fn version_2_format_is_stable() {
        let backup = Backup::from_json(V2).unwrap();
        assert_eq!(backup.history.revisions().len(), 7);
        assert_eq!(backup.history.current().count(), 2);
        assert!(
            backup
                .history
                .current()
                .any(|s| s.notes().starts_with("Family plan"))
        );
        let written: serde_json::Value = serde_json::from_str(&backup.to_json()).unwrap();
        let original: serde_json::Value = serde_json::from_str(V2).unwrap();
        assert_eq!(written, original);
    }

    #[test]
    fn version_1_imports_without_notes() {
        let backup = Backup::from_json(V1).unwrap();
        assert!(backup.history.current().all(|s| s.notes().is_empty()));
        // Exported again, it is the same history in the current version.
        let mut expected: serde_json::Value = serde_json::from_str(V1).unwrap();
        expected["version"] = VERSION.into();
        let written: serde_json::Value = serde_json::from_str(&backup.to_json()).unwrap();
        assert_eq!(written, expected);
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
            error(&with("version", 3.into())),
            BackupError::Newer(3)
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
