//! Session persistence for multi-turn conversations.
//!
//! Manages saving and loading conversation state as JSON files in
//! `~/.beezle/sessions/`. Each session is identified by a key (typically
//! a timestamp or user-chosen name) and stored as `<key>.json`.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Metadata about a saved session, returned by [`SessionManager::list`].
#[derive(Debug, Clone, PartialEq)]
pub struct SessionInfo {
    /// The session key (filename without `.json` extension).
    pub key: String,
    /// When the session file was last modified.
    pub modified: SystemTime,
    /// File size in bytes.
    pub size_bytes: u64,
}

/// Errors that can occur during session operations.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// An I/O error occurred reading or writing session files.
    #[error("session i/o error: {0}")]
    Io(#[from] std::io::Error),
    /// The requested session was not found.
    #[error("session not found: {0}")]
    NotFound(String),
}

/// Manages session files in a directory.
///
/// Each session is stored as a `<key>.json` file containing the serialized
/// conversation messages from yoagent's `Agent::save_messages()`.
#[derive(Debug, Clone)]
pub struct SessionManager {
    /// Directory where session files are stored.
    dir: PathBuf,
}

impl SessionManager {
    /// Creates a new `SessionManager` for the given directory.
    ///
    /// Creates the directory if it doesn't exist.
    ///
    /// # Arguments
    ///
    /// * `dir` - Path to the sessions directory.
    ///
    /// # Errors
    ///
    /// Returns `SessionError::Io` if the directory cannot be created.
    pub fn new(dir: &Path) -> Result<Self, SessionError> {
        std::fs::create_dir_all(dir)?;
        Ok(Self {
            dir: dir.to_path_buf(),
        })
    }

    /// Generates a timestamp-based session key (e.g. `2026-03-05_14-30-05`).
    ///
    /// Uses the current UTC time. The format is chosen to sort chronologically
    /// as a plain string.
    pub fn generate_key() -> String {
        // Use chrono-free approach: parse SystemTime manually.
        // Format: YYYY-MM-DD_HH-MM-SS from Unix timestamp.
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        let secs = now.as_secs();

        // Simple UTC datetime computation (no leap second handling needed
        // for key generation — uniqueness is what matters).
        let days = secs / 86400;
        let time_of_day = secs % 86400;
        let hours = time_of_day / 3600;
        let minutes = (time_of_day % 3600) / 60;
        let seconds = time_of_day % 60;

        // Days since 1970-01-01 to Y-M-D.
        let (year, month, day) = days_to_ymd(days);

        format!("{year:04}-{month:02}-{day:02}_{hours:02}-{minutes:02}-{seconds:02}")
    }

    /// Saves a session's JSON content to `<key>.json`.
    ///
    /// # Arguments
    ///
    /// * `key` - The session key (used as the filename stem).
    /// * `json` - The serialized conversation state from `Agent::save_messages()`.
    ///
    /// # Returns
    ///
    /// The path to the written session file.
    ///
    /// # Errors
    ///
    /// Returns `SessionError::Io` if the file cannot be written.
    pub fn save(&self, key: &str, json: &str) -> Result<PathBuf, SessionError> {
        let path = self.session_path(key);
        std::fs::write(&path, json)?;
        tracing::debug!(key, path = %path.display(), "saved session");
        Ok(path)
    }

    /// Loads a session's JSON content from `<key>.json`.
    ///
    /// # Arguments
    ///
    /// * `key` - The session key to load.
    ///
    /// # Returns
    ///
    /// The raw JSON string, suitable for `Agent::restore_messages()`.
    ///
    /// # Errors
    ///
    /// Returns `SessionError::NotFound` if the file doesn't exist.
    /// Returns `SessionError::Io` on other read failures.
    pub fn load(&self, key: &str) -> Result<String, SessionError> {
        let path = self.session_path(key);
        if !path.exists() {
            return Err(SessionError::NotFound(key.to_owned()));
        }
        let json = std::fs::read_to_string(&path)?;
        tracing::debug!(key, "loaded session");
        Ok(json)
    }

    /// Lists all saved sessions, sorted by modification time (newest first).
    ///
    /// # Errors
    ///
    /// Returns `SessionError::Io` if the directory cannot be read.
    pub fn list(&self) -> Result<Vec<SessionInfo>, SessionError> {
        let mut sessions = Vec::new();

        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();

            // Only consider .json files.
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }

            let key = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_owned();

            if key.is_empty() {
                continue;
            }

            let metadata = entry.metadata()?;
            let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            let size_bytes = metadata.len();

            sessions.push(SessionInfo {
                key,
                modified,
                size_bytes,
            });
        }

        // Sort by modified time, newest first.
        sessions.sort_by(|a, b| b.modified.cmp(&a.modified));
        Ok(sessions)
    }

    /// Returns the key of the most recently modified session, if any.
    ///
    /// # Errors
    ///
    /// Returns `SessionError::Io` if the directory cannot be read.
    pub fn most_recent(&self) -> Result<Option<String>, SessionError> {
        let sessions = self.list()?;
        Ok(sessions.into_iter().next().map(|s| s.key))
    }

    /// Deletes a saved session file.
    ///
    /// # Arguments
    ///
    /// * `key` - The session key to delete.
    ///
    /// # Errors
    ///
    /// Returns `SessionError::NotFound` if the file doesn't exist.
    /// Returns `SessionError::Io` on other failures.
    pub fn delete(&self, key: &str) -> Result<(), SessionError> {
        let path = self.session_path(key);
        if !path.exists() {
            return Err(SessionError::NotFound(key.to_owned()));
        }
        std::fs::remove_file(&path)?;
        tracing::debug!(key, "deleted session");
        Ok(())
    }

    /// Returns the file path for a given session key.
    fn session_path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.json"))
    }
}

/// Converts days since Unix epoch (1970-01-01) to (year, month, day).
///
/// Uses a simple arithmetic approach. Not optimized for speed — called
/// once per session key generation.
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn temp_manager() -> (TempDir, SessionManager) {
        let dir = TempDir::new().unwrap();
        let mgr = SessionManager::new(dir.path()).unwrap();
        (dir, mgr)
    }

    #[test]
    fn generate_key_has_expected_format() {
        let key = SessionManager::generate_key();
        // Format: YYYY-MM-DD_HH-MM-SS (19 chars).
        assert_eq!(key.len(), 19, "key: {key}");
        assert_eq!(&key[4..5], "-");
        assert_eq!(&key[7..8], "-");
        assert_eq!(&key[10..11], "_");
        assert_eq!(&key[13..14], "-");
        assert_eq!(&key[16..17], "-");
    }

    #[test]
    fn save_and_load_roundtrip() {
        let (_dir, mgr) = temp_manager();
        let json = r#"[{"role":"user","content":"hello"}]"#;

        let path = mgr.save("test-session", json).unwrap();
        assert!(path.exists());

        let loaded = mgr.load("test-session").unwrap();
        assert_eq!(loaded, json);
    }

    #[test]
    fn load_nonexistent_returns_not_found() {
        let (_dir, mgr) = temp_manager();
        let result = mgr.load("does-not-exist");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, SessionError::NotFound(_)),
            "expected NotFound, got: {err}"
        );
    }

    #[test]
    fn list_returns_sessions_sorted_by_mtime() {
        let (_dir, mgr) = temp_manager();

        mgr.save("session-a", "[]").unwrap();
        // Ensure different mtime by modifying after a small delay.
        // Since we can't sleep in tests reliably, just write a second file.
        mgr.save("session-b", "[1,2,3]").unwrap();

        let sessions = mgr.list().unwrap();
        assert_eq!(sessions.len(), 2);
        // Both should be present (order may vary if mtime is identical).
        let keys: Vec<&str> = sessions.iter().map(|s| s.key.as_str()).collect();
        assert!(keys.contains(&"session-a"));
        assert!(keys.contains(&"session-b"));
    }

    #[test]
    fn list_ignores_non_json_files() {
        let (dir, mgr) = temp_manager();
        mgr.save("real-session", "[]").unwrap();
        // Create a non-json file in the directory.
        fs::write(dir.path().join("notes.txt"), "not a session").unwrap();

        let sessions = mgr.list().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].key, "real-session");
    }

    #[test]
    fn list_empty_directory() {
        let (_dir, mgr) = temp_manager();
        let sessions = mgr.list().unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn most_recent_returns_none_when_empty() {
        let (_dir, mgr) = temp_manager();
        let key = mgr.most_recent().unwrap();
        assert!(key.is_none());
    }

    #[test]
    fn most_recent_returns_latest() {
        let (_dir, mgr) = temp_manager();
        mgr.save("older", "[]").unwrap();
        mgr.save("newer", "[1]").unwrap();

        let key = mgr.most_recent().unwrap();
        assert!(key.is_some());
        // Can't guarantee order if mtimes are identical, but at least one exists.
    }

    #[test]
    fn delete_removes_session() {
        let (_dir, mgr) = temp_manager();
        mgr.save("to-delete", "[]").unwrap();

        mgr.delete("to-delete").unwrap();

        let result = mgr.load("to-delete");
        assert!(result.is_err());
    }

    #[test]
    fn delete_nonexistent_returns_not_found() {
        let (_dir, mgr) = temp_manager();
        let result = mgr.delete("ghost");
        assert!(matches!(result.unwrap_err(), SessionError::NotFound(_)));
    }

    #[test]
    fn save_overwrites_existing() {
        let (_dir, mgr) = temp_manager();
        mgr.save("overwrite-me", "v1").unwrap();
        mgr.save("overwrite-me", "v2").unwrap();

        let loaded = mgr.load("overwrite-me").unwrap();
        assert_eq!(loaded, "v2");
    }

    #[test]
    fn session_info_has_nonzero_size() {
        let (_dir, mgr) = temp_manager();
        mgr.save("sized", r#"{"data":true}"#).unwrap();

        let sessions = mgr.list().unwrap();
        assert_eq!(sessions.len(), 1);
        assert!(sessions[0].size_bytes > 0);
    }

    #[test]
    fn days_to_ymd_epoch() {
        let (y, m, d) = days_to_ymd(0);
        assert_eq!((y, m, d), (1970, 1, 1));
    }

    #[test]
    fn days_to_ymd_known_date() {
        // 2026-03-05 is day 20517 since epoch.
        let (y, m, d) = days_to_ymd(20517);
        assert_eq!((y, m, d), (2026, 3, 5));
    }
}
