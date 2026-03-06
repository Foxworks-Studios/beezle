//! Project context discovery and loading.
//!
//! Walks up from the current working directory to find project instruction
//! files (e.g. `CLAUDE.md`, `BEEZLE.md`) and injects their contents into
//! the agent's system prompt.

use std::fmt::Write;
use std::path::{Path, PathBuf};

/// File names to search for, in priority order.
/// Earlier entries appear first in the assembled context.
const CONTEXT_FILES: &[&str] = &["CLAUDE.md", "BEEZLE.md", ".beezle/instructions.md"];

/// Default maximum character count for the assembled context block.
pub const DEFAULT_MAX_CHARS: usize = 8000;

/// Discovers project context files by walking up from `start_dir` to the
/// filesystem root.
///
/// For each ancestor directory (including `start_dir` itself), checks for
/// the presence of each file in [`CONTEXT_FILES`] order. All matches are
/// returned, with files closer to `start_dir` and higher in the priority
/// list appearing first.
///
/// # Arguments
///
/// * `start_dir` - The directory to begin searching from (typically CWD).
///
/// # Returns
///
/// A vector of absolute paths to discovered context files, deduplicated.
pub fn discover_context_files(start_dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut dir = Some(start_dir);

    while let Some(current) = dir {
        for name in CONTEXT_FILES {
            let candidate = current.join(name);
            if candidate.is_file() {
                // Deduplicate by canonical path to handle symlinks.
                let key = candidate
                    .canonicalize()
                    .unwrap_or_else(|_| candidate.clone());
                if seen.insert(key) {
                    found.push(candidate);
                }
            }
        }
        dir = current.parent();
    }

    found
}

/// Loads and assembles project context from discovered files.
///
/// Reads each discovered file, prepends a header with the file path,
/// and concatenates them. If the total exceeds `max_chars`, the content
/// is truncated with a notice. The result is wrapped in
/// `<project-context>` delimiters.
///
/// Returns an empty string if no context files are found, so it can be
/// safely prepended to any system prompt.
///
/// # Arguments
///
/// * `start_dir` - The directory to begin searching from.
/// * `max_chars` - Maximum character count before truncation.
///
/// # Returns
///
/// The assembled context string, or empty if no files found.
pub fn load_project_context(start_dir: &Path, max_chars: usize) -> String {
    let files = discover_context_files(start_dir);
    if files.is_empty() {
        return String::new();
    }

    let mut body = String::new();

    for path in &files {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("failed to read context file {}: {e}", path.display());
                continue;
            }
        };
        if content.is_empty() {
            continue;
        }
        // Separate entries with a blank line.
        if !body.is_empty() {
            body.push('\n');
        }
        let _ = writeln!(body, "# Source: {}", path.display());
        body.push_str(&content);
        // Ensure trailing newline.
        if !body.ends_with('\n') {
            body.push('\n');
        }
    }

    if body.is_empty() {
        return String::new();
    }

    // Truncate if over limit (before wrapping in delimiters).
    if body.len() > max_chars {
        body.truncate(max_chars);
        // Avoid cutting mid-line: find last newline within the truncated range.
        if let Some(last_nl) = body.rfind('\n') {
            body.truncate(last_nl + 1);
        }
        body.push_str("\n[... context truncated ...]\n");
    }

    format!("<project-context>\n{body}</project-context>\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Create a temp directory tree and return the root TempDir handle.
    fn setup_dir_with_files(files: &[(&str, &str)]) -> TempDir {
        let dir = TempDir::new().unwrap();
        for (name, content) in files {
            let path = dir.path().join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, content).unwrap();
        }
        dir
    }

    #[test]
    fn discover_finds_claude_md_in_cwd() {
        let dir = setup_dir_with_files(&[("CLAUDE.md", "instructions here")]);
        let found = discover_context_files(dir.path());
        assert_eq!(found.len(), 1);
        assert!(found[0].ends_with("CLAUDE.md"));
    }

    #[test]
    fn discover_finds_beezle_md_in_cwd() {
        let dir = setup_dir_with_files(&[("BEEZLE.md", "beezle stuff")]);
        let found = discover_context_files(dir.path());
        assert_eq!(found.len(), 1);
        assert!(found[0].ends_with("BEEZLE.md"));
    }

    #[test]
    fn discover_finds_dotbeezle_instructions() {
        let dir = setup_dir_with_files(&[(".beezle/instructions.md", "custom instructions")]);
        let found = discover_context_files(dir.path());
        assert_eq!(found.len(), 1);
        assert!(found[0].ends_with(".beezle/instructions.md"));
    }

    #[test]
    fn discover_returns_empty_when_no_files() {
        let dir = TempDir::new().unwrap();
        let found = discover_context_files(dir.path());
        assert!(found.is_empty());
    }

    #[test]
    fn discover_finds_multiple_files_in_priority_order() {
        let dir = setup_dir_with_files(&[("CLAUDE.md", "claude"), ("BEEZLE.md", "beezle")]);
        let found = discover_context_files(dir.path());
        assert_eq!(found.len(), 2);
        assert!(found[0].ends_with("CLAUDE.md"));
        assert!(found[1].ends_with("BEEZLE.md"));
    }

    #[test]
    fn discover_walks_up_to_parent() {
        let dir = setup_dir_with_files(&[("CLAUDE.md", "root context")]);
        let subdir = dir.path().join("sub").join("deep");
        fs::create_dir_all(&subdir).unwrap();

        let found = discover_context_files(&subdir);
        assert!(
            found.iter().any(|p| p.ends_with("CLAUDE.md")),
            "should find CLAUDE.md in ancestor: {found:?}"
        );
    }

    #[test]
    fn discover_finds_files_at_multiple_levels() {
        let dir = setup_dir_with_files(&[("CLAUDE.md", "root")]);
        let subdir = dir.path().join("sub");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("BEEZLE.md"), "sub-level").unwrap();

        let found = discover_context_files(&subdir);
        // Should find BEEZLE.md in subdir and CLAUDE.md in parent.
        assert_eq!(found.len(), 2);
        assert!(found[0].ends_with("BEEZLE.md"), "subdir file first");
        assert!(found[1].ends_with("CLAUDE.md"), "parent file second");
    }

    #[test]
    fn load_returns_empty_when_no_files() {
        let dir = TempDir::new().unwrap();
        let result = load_project_context(dir.path(), DEFAULT_MAX_CHARS);
        assert!(result.is_empty());
    }

    #[test]
    fn load_wraps_in_delimiters() {
        let dir = setup_dir_with_files(&[("CLAUDE.md", "hello world")]);
        let result = load_project_context(dir.path(), DEFAULT_MAX_CHARS);
        assert!(result.starts_with("<project-context>\n"));
        assert!(result.ends_with("</project-context>\n"));
        assert!(result.contains("hello world"));
    }

    #[test]
    fn load_includes_source_headers() {
        let dir = setup_dir_with_files(&[("CLAUDE.md", "content")]);
        let result = load_project_context(dir.path(), DEFAULT_MAX_CHARS);
        assert!(result.contains("# Source:"));
        assert!(result.contains("CLAUDE.md"));
    }

    #[test]
    fn load_concatenates_multiple_files() {
        let dir = setup_dir_with_files(&[("CLAUDE.md", "first"), ("BEEZLE.md", "second")]);
        let result = load_project_context(dir.path(), DEFAULT_MAX_CHARS);
        let first_pos = result.find("first").unwrap();
        let second_pos = result.find("second").unwrap();
        assert!(
            first_pos < second_pos,
            "CLAUDE.md content should appear before BEEZLE.md"
        );
    }

    #[test]
    fn load_truncates_at_max_chars() {
        // Create content that exceeds the limit.
        let long_content = "x\n".repeat(100);
        let dir = setup_dir_with_files(&[("CLAUDE.md", &long_content)]);
        let result = load_project_context(dir.path(), 50);
        assert!(result.contains("[... context truncated ...]"));
        // The total should be bounded (delimiters + truncated content + notice).
        // The body inside delimiters should be <= 50 chars + notice line.
    }

    #[test]
    fn load_skips_empty_files() {
        let dir = setup_dir_with_files(&[("CLAUDE.md", ""), ("BEEZLE.md", "real content")]);
        let result = load_project_context(dir.path(), DEFAULT_MAX_CHARS);
        assert!(
            !result.contains("CLAUDE.md"),
            "empty file should be skipped"
        );
        assert!(result.contains("real content"));
    }
}
