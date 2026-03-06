//! Persistent memory system for the Beezle agent.
//!
//! Provides two-tier markdown-based memory:
//! - **Long-term** (`MEMORY.md`): Stable facts, preferences, and patterns.
//! - **Daily notes** (`YYYY-MM-DD.md`): Timestamped entries for a given day.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Local, NaiveDate};

/// Errors that can occur during memory operations.
#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    /// An I/O error occurred while reading or writing memory files.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The user's home directory could not be determined.
    #[error("home directory not found")]
    HomeNotFound,
}

/// Abstraction over system time for testability.
///
/// Implement this trait to inject a fake clock in tests or use
/// [`SystemClock`] for production code.
pub trait Clock: Send + Sync {
    /// Returns the current local date and time.
    fn now(&self) -> DateTime<Local>;
}

/// Production clock that delegates to [`chrono::Local::now()`].
#[derive(Debug, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Local> {
        Local::now()
    }
}

/// File-backed persistent memory store.
///
/// Manages long-term memory (`MEMORY.md`) and daily notes (`YYYY-MM-DD.md`)
/// within a configurable directory. The directory and files are created lazily
/// on first write, not on construction.
pub struct MemoryStore {
    /// Root directory for all memory files.
    memory_dir: PathBuf,
    /// Injectable clock for determining the current time.
    clock: Arc<dyn Clock>,
}

impl MemoryStore {
    /// Creates a new `MemoryStore` without touching the filesystem.
    ///
    /// # Arguments
    ///
    /// * `memory_dir` - Path to the directory where memory files are stored.
    /// * `clock` - Clock implementation for determining the current time.
    pub fn new(memory_dir: impl Into<PathBuf>, clock: Arc<dyn Clock>) -> Self {
        Self {
            memory_dir: memory_dir.into(),
            clock,
        }
    }

    /// Returns the root path where memory files are stored.
    pub fn memory_dir(&self) -> &Path {
        &self.memory_dir
    }

    /// Returns today's date according to the store's clock.
    pub fn today(&self) -> NaiveDate {
        self.clock.now().date_naive()
    }

    /// Reads the contents of `MEMORY.md` (long-term memory).
    ///
    /// Returns an empty string if the file does not exist yet.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError::Io`] if the file exists but cannot be read.
    pub fn read_long_term(&self) -> Result<String, MemoryError> {
        let path = self.long_term_path();
        match std::fs::read_to_string(&path) {
            Ok(content) => Ok(content),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(e.into()),
        }
    }

    /// Reads the contents of a daily notes file for the given date.
    ///
    /// The file is named `YYYY-MM-DD.md`. Returns an empty string if the
    /// file does not exist.
    ///
    /// # Arguments
    ///
    /// * `date` - The date whose daily notes to read.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError::Io`] if the file exists but cannot be read.
    pub fn read_daily(&self, date: NaiveDate) -> Result<String, MemoryError> {
        let path = self.daily_path(date);
        match std::fs::read_to_string(&path) {
            Ok(content) => Ok(content),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(e.into()),
        }
    }

    /// Appends a timestamped entry to today's daily notes file.
    ///
    /// Creates the file (and parent directory) if they don't exist.
    /// Each entry is formatted as `\n## HH:MM\n{text}\n`.
    ///
    /// # Arguments
    ///
    /// * `text` - The content to append.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError::Io`] if the file cannot be written.
    pub fn append_daily(&self, text: &str) -> Result<(), MemoryError> {
        self.ensure_dir()?;
        let now = self.clock.now();
        let date = now.date_naive();
        let time_str = now.format("%H:%M");
        let path = self.daily_path(date);

        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        write!(file, "\n## {time_str}\n{text}\n")?;
        Ok(())
    }

    /// Atomically replaces the contents of `MEMORY.md`.
    ///
    /// Writes to a temporary file first, then renames it to ensure
    /// atomicity. Creates the directory and file if they don't exist.
    ///
    /// # Arguments
    ///
    /// * `content` - The new content for long-term memory.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError::Io`] if the file cannot be written.
    pub fn write_long_term(&self, content: &str) -> Result<(), MemoryError> {
        self.ensure_dir()?;
        let target = self.long_term_path();
        let tmp = self.memory_dir.join("MEMORY.md.tmp");
        std::fs::write(&tmp, content)?;
        std::fs::rename(&tmp, &target)?;
        Ok(())
    }

    /// Ensures the memory directory exists, creating it if needed.
    fn ensure_dir(&self) -> Result<(), MemoryError> {
        std::fs::create_dir_all(&self.memory_dir)?;
        Ok(())
    }

    /// Returns the path to the long-term memory file.
    fn long_term_path(&self) -> PathBuf {
        self.memory_dir.join("MEMORY.md")
    }

    /// Returns the path to a daily notes file for the given date.
    fn daily_path(&self, date: NaiveDate) -> PathBuf {
        self.memory_dir.join(format!("{}.md", date))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use chrono::TimeZone;
    use tempfile::TempDir;

    /// Test clock that always returns a fixed time.
    #[derive(Debug, Clone)]
    pub struct FakeClock(pub DateTime<Local>);

    impl Clock for FakeClock {
        fn now(&self) -> DateTime<Local> {
            self.0
        }
    }

    /// Helper: creates a `MemoryStore` backed by a temp directory with a fake clock.
    fn test_store(time: DateTime<Local>) -> (MemoryStore, TempDir) {
        let dir = TempDir::new().expect("failed to create temp dir");
        let store = MemoryStore::new(dir.path().join("memory"), Arc::new(FakeClock(time)));
        (store, dir)
    }

    /// Helper: returns 2026-03-05T14:30:00 in local time.
    fn fixed_time() -> DateTime<Local> {
        Local
            .with_ymd_and_hms(2026, 3, 5, 14, 30, 0)
            .single()
            .expect("invalid fixed time")
    }

    #[test]
    fn read_long_term_returns_empty_on_fresh_dir() {
        let (store, _dir) = test_store(fixed_time());
        let content = store.read_long_term().expect("read_long_term failed");
        assert_eq!(content, "");
    }

    #[test]
    fn write_then_read_long_term() {
        let (store, _dir) = test_store(fixed_time());
        store
            .write_long_term("facts")
            .expect("write_long_term failed");
        let content = store.read_long_term().expect("read_long_term failed");
        assert_eq!(content, "facts");
    }

    #[test]
    fn append_daily_creates_file_with_timestamp() {
        let (store, _dir) = test_store(fixed_time());
        store.append_daily("hello").expect("append_daily failed");

        let date = fixed_time().date_naive();
        let content = store.read_daily(date).expect("read_daily failed");
        assert!(content.contains("## 14:30"), "missing timestamp header");
        assert!(content.contains("hello"), "missing appended text");

        // Verify the file exists at the expected path.
        let path = store.daily_path(date);
        assert!(path.exists(), "daily file should exist at {path:?}");
    }

    #[test]
    fn append_daily_twice_contains_both_sections() {
        let time1 = Local
            .with_ymd_and_hms(2026, 3, 5, 14, 30, 0)
            .single()
            .expect("invalid time");
        let time2 = Local
            .with_ymd_and_hms(2026, 3, 5, 15, 45, 0)
            .single()
            .expect("invalid time");

        let dir = TempDir::new().expect("failed to create temp dir");
        let mem_dir = dir.path().join("memory");

        // First append at 14:30.
        let store1 = MemoryStore::new(&mem_dir, Arc::new(FakeClock(time1)));
        store1.append_daily("first").expect("first append failed");

        // Second append at 15:45 (new store instance, different clock).
        let store2 = MemoryStore::new(&mem_dir, Arc::new(FakeClock(time2)));
        store2.append_daily("second").expect("second append failed");

        let date = time1.date_naive();
        let content = store1.read_daily(date).expect("read_daily failed");

        // Both sections should appear in order.
        let pos1 = content.find("## 14:30").expect("missing first timestamp");
        let pos2 = content.find("## 15:45").expect("missing second timestamp");
        assert!(pos1 < pos2, "first entry should appear before second");
        assert!(content.contains("first"), "missing first text");
        assert!(content.contains("second"), "missing second text");
    }

    #[test]
    fn read_daily_returns_empty_for_missing_date() {
        let (store, _dir) = test_store(fixed_time());
        let date = NaiveDate::from_ymd_opt(2025, 1, 1).expect("invalid date");
        let content = store.read_daily(date).expect("read_daily failed");
        assert_eq!(content, "");
    }

    #[test]
    fn long_term_memory_persists_across_store_instances() {
        let dir = TempDir::new().expect("failed to create temp dir");
        let mem_dir = dir.path().join("memory");
        let time = fixed_time();

        // Write with one store instance, then drop it.
        {
            let store = MemoryStore::new(&mem_dir, Arc::new(FakeClock(time)));
            store
                .write_long_term("persistent data")
                .expect("write_long_term failed");
        }
        // The first store is dropped here.

        // Create a brand-new store pointing at the same directory and read back.
        let store2 = MemoryStore::new(&mem_dir, Arc::new(FakeClock(time)));
        let content = store2.read_long_term().expect("read_long_term failed");
        assert_eq!(content, "persistent data");
    }
}
