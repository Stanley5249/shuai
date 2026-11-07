//! Reads the live test accounts from `user/user.json`, which Git ignores.
//!
//! An entry with a `user_id` is a registered account. An entry without one is only a
//! name that can be registered. User IDs are credentials, so nothing prints them.

use std::path::PathBuf;

use miette::{IntoDiagnostic, Result, WrapErr};
use serde::{Deserialize, Serialize};

/// One account, or one name still available to register.
#[derive(Debug, Serialize, Deserialize)]
pub struct AccountEntry {
    pub username: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
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
    let text = serde_json::to_string_pretty(entries).into_diagnostic()?;
    std::fs::write(accounts_path(), format!("{text}\n")).into_diagnostic()
}
