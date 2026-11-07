//! Keeps the live test accounts in `user/accounts.db`, under the directory Git ignores.
//!
//! An account with a `user_id` is registered. One without is only a name that can be
//! registered. A `pending_user_id` is an ID saved before registering, so that a lost
//! answer cannot lose the account. User IDs are credentials, so nothing prints them.
//!
//! Each change writes a JSON snapshot to `user/backups/`, readable without the database
//! engine. `AGENTS.md` describes recovery. Turso allows one process at a time on a file,
//! so examples open the store briefly, and never hold it during a match.

use std::path::{Path, PathBuf};

use miette::{IntoDiagnostic, Result, miette};
use serde::{Deserialize, Serialize};
use turso::{Builder, Connection, Database, Value};

const NEW_USER_ID_LENGTH: usize = 19;
const SNAPSHOT_SUFFIX_LENGTH: usize = 8;
const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS accounts (
    username TEXT PRIMARY KEY NOT NULL,
    user_id TEXT UNIQUE,
    pending_user_id TEXT UNIQUE,
    CHECK (user_id IS NULL OR pending_user_id IS NULL)
) STRICT";
const SELECT_ENTRIES: &str =
    "SELECT username, user_id, pending_user_id FROM accounts ORDER BY rowid";
const INSERT_ENTRY: &str =
    "INSERT INTO accounts (username, user_id, pending_user_id) VALUES (?1, ?2, ?3)";

/// One account, or one name still available to register.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountEntry {
    pub username: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_user_id: Option<String>,
}

/// The open accounts database, and where its snapshots go.
pub struct AccountStore {
    connection: Connection,
    backup_directory: PathBuf,
    _database: Database,
}

fn default_directory() -> PathBuf {
    [env!("CARGO_MANIFEST_DIR"), "..", "..", "user"]
        .iter()
        .collect()
}

impl AccountStore {
    /// Opens the store in `user/`.
    ///
    /// # Errors
    /// Returns an error if the database cannot be opened, for example because another
    /// process holds it.
    pub async fn open() -> Result<Self> {
        Self::open_at(&default_directory()).await
    }

    /// Opens the store in `directory`, creating it if needed.
    ///
    /// # Errors
    /// Returns an error if the database cannot be opened.
    pub async fn open_at(directory: &Path) -> Result<Self> {
        let backup_directory = directory.join("backups");
        std::fs::create_dir_all(&backup_directory).into_diagnostic()?;
        let path = directory.join("accounts.db");
        let path = path
            .to_str()
            .ok_or_else(|| miette!("the accounts path is not UTF-8"))?;
        let database = Builder::new_local(path).build().await.into_diagnostic()?;
        let connection = database.connect().into_diagnostic()?;
        connection.execute(SCHEMA, ()).await.into_diagnostic()?;
        Ok(Self {
            connection,
            backup_directory,
            _database: database,
        })
    }

    /// Reads every entry, in insertion order.
    ///
    /// # Errors
    /// Returns an error if the database cannot be read.
    pub async fn entries(&self) -> Result<Vec<AccountEntry>> {
        read_entries(&self.connection).await
    }

    /// Fills an empty store from `entries`, keeping their order and every ID, and writes
    /// a snapshot. This migrates a JSON file once, and restores any snapshot.
    ///
    /// It takes the store by value, because a failed commit leaves the connection with
    /// a deferred rollback, so the store must not be reused then.
    ///
    /// # Errors
    /// Returns an error, with the store unchanged, if it is not empty, if an entry is
    /// invalid, or if the rows read back differ from `entries`.
    pub async fn import(mut self, entries: &[AccountEntry]) -> Result<Self> {
        if !self.entries().await?.is_empty() {
            return Err(miette!("import needs an empty store"));
        }
        let transaction = self.connection.transaction().await.into_diagnostic()?;
        let filled = async {
            for entry in entries {
                transaction
                    .execute(
                        INSERT_ENTRY,
                        [
                            Value::Text(entry.username.clone()),
                            optional_text(entry.user_id.as_deref()),
                            optional_text(entry.pending_user_id.as_deref()),
                        ],
                    )
                    .await
                    .into_diagnostic()?;
            }
            if read_entries(&transaction).await? == entries {
                Ok(())
            } else {
                Err(miette!("the imported rows differ from the file"))
            }
        }
        .await;
        match filled {
            Ok(()) => transaction.commit().await.into_diagnostic()?,
            Err(error) => {
                transaction.rollback().await.into_diagnostic()?;
                return Err(error);
            }
        }
        self.write_backup().await?;
        Ok(self)
    }

    /// Returns the ID to register `username` under, saving a new one if it has none yet.
    /// A later attempt gets the same ID.
    ///
    /// # Errors
    /// Returns an error if the name is unknown or already registered.
    pub async fn reserve_user_id(&self, username: &str) -> Result<String> {
        let entries = self.entries().await?;
        let entry = entries
            .iter()
            .find(|entry| entry.username == username)
            .ok_or_else(|| miette!("{username:?} is not a name in the store"))?;
        if entry.user_id.is_some() {
            return Err(miette!("{username:?} is already registered"));
        }
        if let Some(pending) = &entry.pending_user_id {
            return Ok(pending.clone());
        }
        let user_id = random_alphanumeric(NEW_USER_ID_LENGTH)?;
        let changed = self
            .connection
            .execute(
                "UPDATE accounts SET pending_user_id = ?2
                 WHERE username = ?1 AND user_id IS NULL AND pending_user_id IS NULL",
                [username, user_id.as_str()],
            )
            .await
            .into_diagnostic()?;
        if changed != 1 {
            return Err(miette!("failed to reserve an ID for {username:?}"));
        }
        Ok(user_id)
    }

    /// Marks `username` as registered under its reserved ID.
    ///
    /// # Errors
    /// Returns an error, changing nothing, unless `user_id` is the reserved ID.
    pub async fn confirm_registration(&self, username: &str, user_id: &str) -> Result<()> {
        let changed = self
            .connection
            .execute(
                "UPDATE accounts SET user_id = pending_user_id, pending_user_id = NULL
                 WHERE username = ?1 AND pending_user_id = ?2",
                [username, user_id],
            )
            .await
            .into_diagnostic()?;
        if changed == 1 {
            Ok(())
        } else {
            Err(miette!("{username:?} has no matching reserved ID"))
        }
    }

    /// Writes the committed entries to a new snapshot, flushed to disk, and returns its
    /// path. It never replaces an existing snapshot.
    ///
    /// # Errors
    /// Returns an error if the entries cannot be read or the file cannot be written.
    pub async fn write_backup(&self) -> Result<PathBuf> {
        let entries = self.entries().await?;
        write_snapshot(&self.backup_directory, &entries)
    }
}

/// Reads the IDs of the registered accounts, in insertion order, and closes the store.
///
/// # Errors
/// Returns an error if the store cannot be opened or read.
pub async fn registered_user_ids() -> Result<Vec<String>> {
    Ok(AccountStore::open()
        .await?
        .entries()
        .await?
        .into_iter()
        .filter_map(|entry| entry.user_id)
        .collect())
}

/// Reads a JSON file of entries, such as a snapshot.
///
/// # Errors
/// Returns an error if the file is missing or invalid.
pub fn read_entries_file(path: &Path) -> Result<Vec<AccountEntry>> {
    let text = std::fs::read_to_string(path).into_diagnostic()?;
    serde_json::from_str(&text).into_diagnostic()
}

async fn read_entries(connection: &Connection) -> Result<Vec<AccountEntry>> {
    let mut rows = connection
        .query(SELECT_ENTRIES, ())
        .await
        .into_diagnostic()?;
    let mut entries = Vec::new();
    while let Some(row) = rows.next().await.into_diagnostic()? {
        let Value::Text(username) = row.get_value(0).into_diagnostic()? else {
            return Err(miette!("a stored username is not text"));
        };
        entries.push(AccountEntry {
            username,
            user_id: read_optional_text(row.get_value(1).into_diagnostic()?)?,
            pending_user_id: read_optional_text(row.get_value(2).into_diagnostic()?)?,
        });
    }
    Ok(entries)
}

fn optional_text(value: Option<&str>) -> Value {
    value.map_or(Value::Null, |text| Value::Text(text.to_owned()))
}

fn read_optional_text(value: Value) -> Result<Option<String>> {
    match value {
        Value::Null => Ok(None),
        Value::Text(text) => Ok(Some(text)),
        _ => Err(miette!("a stored ID is not text")),
    }
}

/// Generates text from the operating system's random source, because a user ID is a
/// credential, and a seeded generator can repeat across processes.
fn random_alphanumeric(length: usize) -> Result<String> {
    const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    // Rejecting bytes above this multiple of the alphabet's length avoids any bias.
    let limit = u8::MAX - u8::MAX % u8::try_from(ALPHABET.len()).expect("the alphabet is short");
    let mut text = String::with_capacity(length);
    while text.len() < length {
        let mut bytes = [0; 64];
        getrandom::fill(&mut bytes)
            .map_err(|error| miette!("failed to get randomness: {error}"))?;
        text.extend(
            bytes
                .iter()
                .filter(|&&byte| byte < limit)
                .map(|&byte| char::from(ALPHABET[usize::from(byte) % ALPHABET.len()]))
                .take(length - text.len()),
        );
    }
    Ok(text)
}

fn write_snapshot(directory: &Path, entries: &[AccountEntry]) -> Result<PathBuf> {
    use std::io::Write;

    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .into_diagnostic()?
        .as_millis();
    let suffix = random_alphanumeric(SNAPSHOT_SUFFIX_LENGTH)?;
    let name = format!("accounts-{millis}-{suffix}");
    let path = directory.join(format!("{name}.json"));
    if path.exists() {
        return Err(miette!("the snapshot {} already exists", path.display()));
    }
    let partial = directory.join(format!("{name}.partial"));
    let text = serde_json::to_string_pretty(entries).into_diagnostic()?;
    let mut file = std::fs::File::create_new(&partial).into_diagnostic()?;
    file.write_all(format!("{text}\n").as_bytes())
        .into_diagnostic()?;
    file.sync_all().into_diagnostic()?;
    drop(file);
    std::fs::rename(&partial, &path).into_diagnostic()?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A temporary directory, removed when dropped.
    struct TempDirectory(PathBuf);

    impl TempDirectory {
        fn new() -> Self {
            let suffix = random_alphanumeric(16).unwrap();
            let path = std::env::temp_dir().join(format!("shuai-{suffix}"));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDirectory {
        fn drop(&mut self) {
            // A leftover test directory holds no real data, so a failure here is harmless.
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn entry(username: &str, user_id: Option<&str>, pending: Option<&str>) -> AccountEntry {
        AccountEntry {
            username: username.to_owned(),
            user_id: user_id.map(str::to_owned),
            pending_user_id: pending.map(str::to_owned),
        }
    }

    fn sample() -> Vec<AccountEntry> {
        vec![
            entry("zebra", Some("id-z"), None),
            entry("pending", None, Some("id-p")),
            entry("available", None, None),
            entry("apple", Some("id-a"), None),
        ]
    }

    fn snapshots(directory: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(directory.join("backups"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect()
    }

    async fn filled_store(directory: &TempDirectory) -> AccountStore {
        AccountStore::open_at(&directory.0)
            .await
            .unwrap()
            .import(&sample())
            .await
            .unwrap()
    }

    #[test]
    fn random_text_has_the_length_and_alphabet_and_does_not_repeat() {
        let first = random_alphanumeric(NEW_USER_ID_LENGTH).unwrap();
        let second = random_alphanumeric(NEW_USER_ID_LENGTH).unwrap();
        assert_eq!(first.len(), NEW_USER_ID_LENGTH);
        assert!(first.chars().all(|c| c.is_ascii_alphanumeric()));
        assert_ne!(first, second);
    }

    #[tokio::test]
    async fn import_keeps_every_field_and_the_order_across_a_reopen() {
        let directory = TempDirectory::new();
        drop(filled_store(&directory).await);
        let reopened = AccountStore::open_at(&directory.0).await.unwrap();
        assert_eq!(reopened.entries().await.unwrap(), sample());
    }

    #[tokio::test]
    async fn import_writes_a_snapshot() {
        let directory = TempDirectory::new();
        drop(filled_store(&directory).await);
        let written = snapshots(&directory.0);
        assert_eq!(written.len(), 1);
        assert_eq!(read_entries_file(&written[0]).unwrap(), sample());
    }

    #[tokio::test]
    async fn import_refuses_a_store_that_has_entries() {
        let directory = TempDirectory::new();
        drop(filled_store(&directory).await);
        let reopened = AccountStore::open_at(&directory.0).await.unwrap();
        assert!(reopened.import(&sample()).await.is_err());
        let again = AccountStore::open_at(&directory.0).await.unwrap();
        assert_eq!(again.entries().await.unwrap(), sample());
    }

    #[tokio::test]
    async fn a_failed_import_leaves_nothing_behind() {
        let directory = TempDirectory::new();
        let store = AccountStore::open_at(&directory.0).await.unwrap();
        let duplicate_id = [
            entry("first", Some("same"), None),
            entry("second", Some("same"), None),
        ];
        assert!(store.import(&duplicate_id).await.is_err());
        let reopened = AccountStore::open_at(&directory.0).await.unwrap();
        assert_eq!(reopened.entries().await.unwrap(), []);
        assert_eq!(snapshots(&directory.0), [] as [PathBuf; 0]);
    }

    #[tokio::test]
    async fn an_entry_cannot_hold_both_kinds_of_id() {
        let directory = TempDirectory::new();
        let store = AccountStore::open_at(&directory.0).await.unwrap();
        let both = [entry("both", Some("id"), Some("other"))];
        assert!(store.import(&both).await.is_err());
    }

    #[tokio::test]
    async fn a_reserved_id_survives_a_reopen_and_is_reused() {
        let directory = TempDirectory::new();
        let store = filled_store(&directory).await;
        let reserved = store.reserve_user_id("available").await.unwrap();
        drop(store);
        let reopened = AccountStore::open_at(&directory.0).await.unwrap();
        assert_eq!(
            reopened.reserve_user_id("available").await.unwrap(),
            reserved
        );
        assert_eq!(reopened.reserve_user_id("pending").await.unwrap(), "id-p");
    }

    #[tokio::test]
    async fn registered_and_unknown_names_cannot_be_reserved() {
        let directory = TempDirectory::new();
        let store = filled_store(&directory).await;
        store.reserve_user_id("apple").await.unwrap_err();
        store.reserve_user_id("nobody").await.unwrap_err();
    }

    #[tokio::test]
    async fn confirming_needs_the_reserved_id() {
        let directory = TempDirectory::new();
        let store = filled_store(&directory).await;
        assert!(
            store
                .confirm_registration("pending", "wrong")
                .await
                .is_err()
        );
        assert_eq!(store.entries().await.unwrap(), sample());
        store.confirm_registration("pending", "id-p").await.unwrap();
        let entries = store.entries().await.unwrap();
        assert_eq!(entries[1], entry("pending", Some("id-p"), None));
        store
            .confirm_registration("pending", "id-p")
            .await
            .unwrap_err();
    }

    #[tokio::test]
    async fn a_snapshot_restores_into_a_new_store() {
        let directory = TempDirectory::new();
        let store = filled_store(&directory).await;
        let first = store.write_backup().await.unwrap();
        let second = store.write_backup().await.unwrap();
        assert_ne!(first, second, "a snapshot never replaces another");
        assert!(
            snapshots(&directory.0)
                .iter()
                .all(|path| path.extension().unwrap() == "json")
        );

        let restored_directory = TempDirectory::new();
        let restored = AccountStore::open_at(&restored_directory.0)
            .await
            .unwrap()
            .import(&read_entries_file(&first).unwrap())
            .await
            .unwrap();
        assert_eq!(restored.entries().await.unwrap(), sample());
    }
}
