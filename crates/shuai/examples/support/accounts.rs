//! Reads the live test accounts from `user/user.json`, which Git ignores.
//!
//! An entry with a `user_id` is a registered account. An entry without one is only a
//! name that can be registered. A `pending_user_id` is an ID saved before registering,
//! so that a lost answer cannot lose the account. User IDs are credentials, so nothing
//! prints them.

use std::path::{Path, PathBuf};

use miette::{IntoDiagnostic, Result, WrapErr};
use serde::{Deserialize, Serialize};

/// One account, or one name still available to register.
#[derive(Debug, Serialize, Deserialize)]
pub struct AccountEntry {
    pub username: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_user_id: Option<String>,
}

fn accounts_path() -> PathBuf {
    [env!("CARGO_MANIFEST_DIR"), "..", "..", "user", "user.json"]
        .iter()
        .collect()
}

/// Reads every entry.
///
/// # Errors
/// Returns an error if the file is missing or invalid.
pub fn load_entries() -> Result<Vec<AccountEntry>> {
    let path = accounts_path();
    let text = std::fs::read_to_string(&path)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&text).into_diagnostic()
}

/// Reads the user IDs of the registered accounts, in file order.
///
/// # Errors
/// Returns an error if the file is missing or invalid.
pub fn registered_user_ids() -> Result<Vec<String>> {
    Ok(load_entries()?
        .into_iter()
        .filter_map(|entry| entry.user_id)
        .collect())
}

/// Writes every entry back, in the file's format.
///
/// # Errors
/// Returns an error if the file cannot be written.
pub fn save_entries(entries: &[AccountEntry]) -> Result<()> {
    write_entries(&accounts_path(), entries)
}

/// Replaces the file only after its new contents are complete, so a failed write cannot
/// destroy the credentials already saved.
fn write_entries(path: &Path, entries: &[AccountEntry]) -> Result<()> {
    let text = serde_json::to_string_pretty(entries).into_diagnostic()?;
    let replacement = path.with_extension("json.new");
    std::fs::write(&replacement, format!("{text}\n")).into_diagnostic()?;
    std::fs::rename(&replacement, path).into_diagnostic()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writing_replaces_the_file_and_keeps_every_field() {
        let directory = std::env::temp_dir().join(format!("shuai-{:016x}", fastrand::u64(..)));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("user.json");
        std::fs::write(&path, "old contents").unwrap();
        let entries = [
            AccountEntry {
                username: "registered".to_owned(),
                user_id: Some("id".to_owned()),
                pending_user_id: None,
            },
            AccountEntry {
                username: "pending".to_owned(),
                user_id: None,
                pending_user_id: Some("new id".to_owned()),
            },
            AccountEntry {
                username: "available".to_owned(),
                user_id: None,
                pending_user_id: None,
            },
        ];
        write_entries(&path, &entries).unwrap();
        let read: Vec<AccountEntry> =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(read[0].user_id.as_deref(), Some("id"));
        assert_eq!(read[1].pending_user_id.as_deref(), Some("new id"));
        assert!(read[1].user_id.is_none());
        assert!(read[2].user_id.is_none() && read[2].pending_user_id.is_none());
        assert!(!path.with_extension("json.new").exists());
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&directory).unwrap();
    }
}
