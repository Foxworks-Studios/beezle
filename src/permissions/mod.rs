//! Permission types and pattern matching for the Beezle permissions system.
//!
//! Provides the foundational types (`PermissionRule`, `ToolCategory`,
//! `PermissionVerdict`, `PermissionResponse`), pure functions
//! (`parse_rule`, `pattern_matches`), and the [`PermissionPolicy`] that
//! loads three-tier settings and evaluates tool invocations.

pub mod guard;
pub mod hooks;

use std::path::Path;

use serde::{Deserialize, Serialize};

/// A parsed permission rule like `Bash(cargo test:*)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionRule {
    /// Tool name (e.g. "Bash", "Read", "Write").
    pub tool: String,
    /// Pattern to match against the tool's primary argument.
    pub pattern: String,
}

/// Tool category for default permission policies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCategory {
    /// Read-only operations (read_file, list_files, search).
    Read,
    /// Write operations (write_file, edit_file).
    Write,
    /// Shell command execution (bash).
    Execute,
    /// Network operations (web_fetch, web_search).
    Network,
    /// Internal agent operations (memory, subagents) — allowed unless explicitly denied.
    Internal,
}

/// Result of checking a tool invocation against the permission policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionVerdict {
    /// The invocation is explicitly allowed.
    Allow,
    /// The invocation is explicitly denied.
    Deny,
    /// No rule matched; the user should be prompted.
    Ask,
}

/// The user's response to a permission prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionResponse {
    /// Allow this single invocation.
    Yes,
    /// Deny this invocation.
    No,
    /// Allow and remember for the rest of the session.
    Always,
}

/// Errors that can occur in the permissions system.
#[derive(Debug, thiserror::Error)]
pub enum PermissionError {
    /// A rule string could not be parsed (missing parentheses, etc.).
    #[error("invalid permission rule: {0}")]
    InvalidRule(String),
}

/// Deserialized inner permissions block from a settings file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionSettingsInner {
    /// Rules that allow tool invocations.
    #[serde(default)]
    pub allow: Vec<String>,
    /// Rules that deny tool invocations.
    #[serde(default)]
    pub deny: Vec<String>,
}

/// Deserialized settings file (top-level JSON structure).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionSettings {
    /// The permissions block (optional; may be absent in a settings file).
    #[serde(default)]
    pub permissions: Option<PermissionSettingsInner>,
}

/// The merged permission policy from all settings tiers.
///
/// Created via [`PermissionPolicy::load`], which reads and merges the
/// three-tier settings files (global, project, local). The [`check`]
/// method evaluates a tool invocation against the merged policy.
///
/// [`check`]: PermissionPolicy::check
#[derive(Debug, Clone)]
pub struct PermissionPolicy {
    /// Merged allow rules from all tiers.
    pub allow: Vec<PermissionRule>,
    /// Merged deny rules from all tiers.
    pub deny: Vec<PermissionRule>,
    /// Session-scoped grants from interactive "Always" responses.
    pub session_grants: Vec<PermissionRule>,
}

impl PermissionPolicy {
    /// Load and merge permission settings from all three tiers.
    ///
    /// Missing files are silently ignored. Malformed JSON produces a
    /// `tracing::warn!` and the tier is skipped.
    pub fn load(cwd: &Path) -> Self {
        let home_beezle = dirs::home_dir()
            .map(|h| h.join(".beezle"))
            .unwrap_or_default();
        Self::load_with_home(cwd, &home_beezle)
    }

    /// Load with an explicit home-beezle directory (for testing).
    pub fn load_with_home(cwd: &Path, home_beezle: &Path) -> Self {
        let mut allow = Vec::new();
        let mut deny = Vec::new();

        let paths = [
            home_beezle.join("settings.json"),
            cwd.join(".beezle/settings.json"),
            cwd.join(".beezle/local.settings.json"),
        ];

        for path in &paths {
            if let Some(settings) = Self::read_settings(path)
                && let Some(perms) = settings.permissions
            {
                for rule_str in &perms.allow {
                    match parse_rule(rule_str) {
                        Ok(rule) => allow.push(rule),
                        Err(e) => tracing::warn!("skipping invalid allow rule: {e}"),
                    }
                }
                for rule_str in &perms.deny {
                    match parse_rule(rule_str) {
                        Ok(rule) => deny.push(rule),
                        Err(e) => tracing::warn!("skipping invalid deny rule: {e}"),
                    }
                }
            }
        }

        Self {
            allow,
            deny,
            session_grants: Vec::new(),
        }
    }

    /// Read and deserialize a single settings file.
    ///
    /// Returns `None` if the file does not exist or contains malformed JSON.
    fn read_settings(path: &Path) -> Option<PermissionSettings> {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return None,
        };
        match serde_json::from_str::<PermissionSettings>(&content) {
            Ok(s) => Some(s),
            Err(e) => {
                tracing::warn!("malformed settings file {}: {e}", path.display());
                None
            }
        }
    }

    /// Check whether a tool invocation is allowed.
    ///
    /// Resolution order (first match wins):
    /// 1. Session grants — if a session grant matches, return `Allow`.
    /// 2. Deny rules — if any deny rule matches, return `Deny`.
    /// 3. Allow rules — if any allow rule matches, return `Allow`.
    /// 4. Category defaults — hardcoded fallback by tool category.
    pub fn check(&self, tool_name: &str, args: &serde_json::Value) -> PermissionVerdict {
        let primary_arg = extract_primary_arg(tool_name, args);

        // 1. Session grants (highest precedence).
        for grant in &self.session_grants {
            if rule_matches(&grant.tool, &grant.pattern, tool_name, &primary_arg) {
                return PermissionVerdict::Allow;
            }
        }

        // 2. Deny rules.
        for rule in &self.deny {
            if rule_matches(&rule.tool, &rule.pattern, tool_name, &primary_arg) {
                return PermissionVerdict::Deny;
            }
        }

        // 3. Allow rules.
        for rule in &self.allow {
            if rule_matches(&rule.tool, &rule.pattern, tool_name, &primary_arg) {
                return PermissionVerdict::Allow;
            }
        }

        // 4. Category defaults.
        Self::category_default(tool_name)
    }

    /// Add a session-scoped grant (from interactive "Always" response).
    pub fn grant_session(&mut self, tool_name: &str, args: &serde_json::Value) {
        let primary_arg = extract_primary_arg(tool_name, args);
        self.session_grants.push(PermissionRule {
            tool: tool_name.to_string(),
            pattern: primary_arg,
        });
    }

    /// Categorize a tool name into a [`ToolCategory`].
    pub fn categorize(tool_name: &str) -> ToolCategory {
        match tool_name {
            "read_file" | "list_files" | "search" => ToolCategory::Read,
            "write_file" | "edit_file" => ToolCategory::Write,
            "bash" => ToolCategory::Execute,
            "web_fetch" | "web_search" => ToolCategory::Network,
            "memory_read" | "memory_write" | "spawn_agent" => ToolCategory::Internal,
            // Default unknown tools to Execute (safest — requires prompting).
            _ => ToolCategory::Execute,
        }
    }

    /// Return the default verdict for a tool based on its category.
    fn category_default(tool_name: &str) -> PermissionVerdict {
        match Self::categorize(tool_name) {
            ToolCategory::Read | ToolCategory::Internal => PermissionVerdict::Allow,
            ToolCategory::Write | ToolCategory::Execute | ToolCategory::Network => {
                PermissionVerdict::Ask
            }
        }
    }
}

/// Extract the primary argument from tool args for pattern matching.
///
/// - `bash`: uses the `"command"` field.
/// - `read_file`, `write_file`, `edit_file`: uses the `"file_path"` field.
/// - `web_fetch`, `web_search`: uses the `"url"` field.
/// - Other tools: serializes the entire args object.
fn extract_primary_arg(tool_name: &str, args: &serde_json::Value) -> String {
    match tool_name {
        "bash" => args
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        "read_file" | "write_file" | "edit_file" => args
            .get("file_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        "web_fetch" | "web_search" => args
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        _ => serde_json::to_string(args).unwrap_or_default(),
    }
}

/// Check if a rule matches a tool invocation.
///
/// Tool names are compared case-insensitively. The pattern is checked
/// against the primary argument using [`pattern_matches`].
fn rule_matches(rule_tool: &str, rule_pattern: &str, tool_name: &str, primary_arg: &str) -> bool {
    if !rule_tool.eq_ignore_ascii_case(tool_name) {
        return false;
    }
    pattern_matches(rule_pattern, primary_arg)
}

/// Parse a rule string like `"Bash(cargo test:*)"` into a [`PermissionRule`].
///
/// The expected format is `ToolName(pattern)`. An empty pattern (e.g.
/// `"Read()"`) is valid. Missing parentheses produce an error.
///
/// # Errors
///
/// Returns [`PermissionError::InvalidRule`] if the rule string does not
/// contain matching parentheses.
pub fn parse_rule(rule: &str) -> Result<PermissionRule, PermissionError> {
    let open = rule
        .find('(')
        .ok_or_else(|| PermissionError::InvalidRule(rule.to_string()))?;
    if !rule.ends_with(')') {
        return Err(PermissionError::InvalidRule(rule.to_string()));
    }
    let tool = &rule[..open];
    if tool.is_empty() {
        return Err(PermissionError::InvalidRule(rule.to_string()));
    }
    let pattern = &rule[open + 1..rule.len() - 1];
    Ok(PermissionRule {
        tool: tool.to_string(),
        pattern: pattern.to_string(),
    })
}

/// Check whether a pattern matches a given value.
///
/// Supported pattern syntax:
/// - `*` — matches anything (bare wildcard).
/// - `:*` suffix — prefix match (e.g. `"cargo test:*"` matches any string
///   starting with `"cargo test"`).
/// - `domain:X` — matches if the value contains the domain `X`.
/// - `**` in a path — recursive glob (matches any number of path segments).
/// - `*` in a path — matches a single path segment (no `/`).
/// - Exact match as fallback.
pub fn pattern_matches(pattern: &str, value: &str) -> bool {
    // Bare wildcard matches everything.
    if pattern == "*" {
        return true;
    }

    // `:*` suffix means prefix match on everything before `:*`.
    if let Some(prefix) = pattern.strip_suffix(":*") {
        // Also handle `domain:X` being a prefix pattern — but only if
        // the prefix itself doesn't start with `domain:`.
        return value.starts_with(prefix);
    }

    // `domain:X` — check if value contains the domain.
    if let Some(domain) = pattern.strip_prefix("domain:") {
        return value.contains(domain);
    }

    // Glob matching for path patterns containing `*` or `**`.
    if pattern.contains('*') {
        return glob_matches(pattern, value);
    }

    // Exact match.
    pattern == value
}

/// Simple glob matcher supporting `*` (single segment) and `**` (recursive).
///
/// `*` matches any characters except `/`.
/// `**` matches any characters including `/`.
fn glob_matches(pattern: &str, value: &str) -> bool {
    // Convert the glob pattern to a simple regex-like matcher.
    // We process the pattern character by character.
    let mut regex_str = String::with_capacity(pattern.len() * 2);
    regex_str.push('^');

    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '*' && i + 1 < chars.len() && chars[i + 1] == '*' {
            // `**` matches everything including `/`.
            regex_str.push_str(".*");
            i += 2;
            // Skip a trailing `/` after `**` if present.
            if i < chars.len() && chars[i] == '/' {
                // `**/` — the `.*` already covers the slash.
                i += 1;
            }
        } else if chars[i] == '*' {
            // `*` matches everything except `/`.
            regex_str.push_str("[^/]*");
            i += 1;
        } else {
            // Escape regex metacharacters.
            let c = chars[i];
            if "\\^$.|+?()[]{}".contains(c) {
                regex_str.push('\\');
            }
            regex_str.push(c);
            i += 1;
        }
    }
    regex_str.push('$');

    // Use a simple hand-rolled match instead of pulling in regex crate.
    simple_regex_match(&regex_str, value)
}

/// Minimal regex-like matcher that only supports:
/// - `^` / `$` anchors
/// - `.*` (match anything)
/// - `[^/]*` (match anything except `/`)
/// - Literal characters (with `\` escaping)
///
/// This avoids adding a `regex` dependency for simple glob patterns.
fn simple_regex_match(pattern: &str, value: &str) -> bool {
    // Strip anchors — we always do full match.
    let pat = pattern
        .strip_prefix('^')
        .unwrap_or(pattern)
        .strip_suffix('$')
        .unwrap_or(pattern);

    match_recursive(pat, value)
}

/// Recursive matcher for the simplified regex pattern.
fn match_recursive(pattern: &str, value: &str) -> bool {
    if pattern.is_empty() {
        return value.is_empty();
    }

    // `.*` — match any amount of any characters.
    if let Some(rest_pat) = pattern.strip_prefix(".*") {
        // Try matching rest_pat against every suffix of value.
        for i in 0..=value.len() {
            if match_recursive(rest_pat, &value[i..]) {
                return true;
            }
        }
        return false;
    }

    // `[^/]*` — match any amount of non-slash characters.
    if let Some(rest_pat) = pattern.strip_prefix("[^/]*") {
        // Find how many non-slash chars we can consume.
        let max_consume = value.find('/').unwrap_or(value.len());
        for i in 0..=max_consume {
            if match_recursive(rest_pat, &value[i..]) {
                return true;
            }
        }
        return false;
    }

    // Escaped character.
    if let Some(rest_pat) = pattern.strip_prefix('\\') {
        if rest_pat.is_empty() {
            return false;
        }
        let expected = rest_pat.as_bytes()[0];
        if value.is_empty() || value.as_bytes()[0] != expected {
            return false;
        }
        return match_recursive(&rest_pat[1..], &value[1..]);
    }

    // Literal character match.
    if value.is_empty() {
        return false;
    }
    let pat_char = pattern.as_bytes()[0];
    let val_char = value.as_bytes()[0];
    if pat_char != val_char {
        return false;
    }
    match_recursive(&pattern[1..], &value[1..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // ── parse_rule tests ──────────────────────────────────────────

    #[test]
    fn parse_rule_bash_with_pattern() {
        let rule = parse_rule("Bash(cargo test:*)").unwrap();
        assert_eq!(rule.tool, "Bash");
        assert_eq!(rule.pattern, "cargo test:*");
    }

    #[test]
    fn parse_rule_empty_pattern() {
        let rule = parse_rule("Read()").unwrap();
        assert_eq!(rule.tool, "Read");
        assert_eq!(rule.pattern, "");
    }

    #[test]
    fn parse_rule_missing_parens() {
        let result = parse_rule("NoParen");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("invalid permission rule"));
    }

    // ── pattern_matches tests ─────────────────────────────────────

    #[test]
    fn prefix_match_with_colon_star() {
        assert!(pattern_matches("cargo test:*", "cargo test --release"));
    }

    #[test]
    fn prefix_match_no_match() {
        assert!(!pattern_matches("cargo test:*", "cargo fmt"));
    }

    #[test]
    fn recursive_glob_matches() {
        assert!(pattern_matches("/src/**", "/src/main.rs"));
    }

    #[test]
    fn recursive_glob_no_match() {
        assert!(!pattern_matches("/src/**", "/tests/foo.rs"));
    }

    #[test]
    fn single_segment_glob_matches() {
        assert!(pattern_matches("/src/*.rs", "/src/main.rs"));
    }

    #[test]
    fn single_segment_glob_no_nested() {
        assert!(!pattern_matches("/src/*.rs", "/src/nested/main.rs"));
    }

    #[test]
    fn domain_match() {
        assert!(pattern_matches("domain:docs.rs", "https://docs.rs/tokio"));
    }

    #[test]
    fn domain_no_match() {
        assert!(!pattern_matches(
            "domain:docs.rs",
            "https://crates.io/tokio"
        ));
    }

    #[test]
    fn bare_wildcard_matches_anything() {
        assert!(pattern_matches("*", "anything"));
    }

    // ── PermissionPolicy tests ───────────────────────────────────

    /// Helper to create a settings JSON file with allow/deny lists.
    fn write_settings(dir: &std::path::Path, filename: &str, allow: &[&str], deny: &[&str]) {
        let settings = PermissionSettings {
            permissions: Some(PermissionSettingsInner {
                allow: allow.iter().map(|s| s.to_string()).collect(),
                deny: deny.iter().map(|s| s.to_string()).collect(),
            }),
        };
        let json = serde_json::to_string_pretty(&settings).unwrap();
        let path = dir.join(filename);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(json.as_bytes()).unwrap();
    }

    #[test]
    fn load_missing_files_returns_empty_policy() {
        let tmp = tempfile::TempDir::new().unwrap();
        let policy = PermissionPolicy::load(tmp.path());
        assert!(policy.allow.is_empty());
        assert!(policy.deny.is_empty());
    }

    #[test]
    fn load_malformed_json_skips_tier() {
        let tmp = tempfile::TempDir::new().unwrap();
        let beezle_dir = tmp.path().join(".beezle");
        std::fs::create_dir_all(&beezle_dir).unwrap();
        std::fs::write(beezle_dir.join("settings.json"), "NOT JSON!!!").unwrap();
        let policy = PermissionPolicy::load(tmp.path());
        // Should not panic, just skip the malformed file.
        assert!(policy.allow.is_empty());
        assert!(policy.deny.is_empty());
    }

    #[test]
    fn three_tiers_merge_correctly() {
        let tmp = tempfile::TempDir::new().unwrap();

        // Global tier: ~/.beezle/settings.json
        let home_beezle = tmp.path().join("home_beezle");
        std::fs::create_dir_all(&home_beezle).unwrap();
        write_settings(&home_beezle, "settings.json", &["Read(/src/**)"], &[]);

        // Project tier: .beezle/settings.json
        let project_beezle = tmp.path().join(".beezle");
        std::fs::create_dir_all(&project_beezle).unwrap();
        write_settings(
            tmp.path(),
            ".beezle/settings.json",
            &["Bash(cargo test:*)"],
            &["Bash(rm -rf:*)"],
        );

        // Local tier: .beezle/local.settings.json
        write_settings(
            tmp.path(),
            ".beezle/local.settings.json",
            &["Bash(cargo build:*)"],
            &["Bash(git push --force:*)"],
        );

        let policy = PermissionPolicy::load_with_home(tmp.path(), &home_beezle);

        // All allow rules from all three tiers are present.
        assert_eq!(policy.allow.len(), 3);
        assert_eq!(policy.deny.len(), 2);

        // Verify rules from each tier are active.
        let allow_patterns: Vec<&str> = policy.allow.iter().map(|r| r.pattern.as_str()).collect();
        assert!(allow_patterns.contains(&"/src/**"));
        assert!(allow_patterns.contains(&"cargo test:*"));
        assert!(allow_patterns.contains(&"cargo build:*"));

        let deny_patterns: Vec<&str> = policy.deny.iter().map(|r| r.pattern.as_str()).collect();
        assert!(deny_patterns.contains(&"rm -rf:*"));
        assert!(deny_patterns.contains(&"git push --force:*"));
    }

    #[test]
    fn check_allow_when_allow_rule_matches() {
        let policy = PermissionPolicy {
            allow: vec![PermissionRule {
                tool: "bash".to_string(),
                pattern: "cargo test:*".to_string(),
            }],
            deny: vec![],
            session_grants: vec![],
        };
        let args = serde_json::json!({"command": "cargo test --release"});
        assert_eq!(policy.check("bash", &args), PermissionVerdict::Allow);
    }

    #[test]
    fn check_deny_overrides_allow() {
        let policy = PermissionPolicy {
            allow: vec![PermissionRule {
                tool: "bash".to_string(),
                pattern: "*".to_string(),
            }],
            deny: vec![PermissionRule {
                tool: "bash".to_string(),
                pattern: "rm -rf:*".to_string(),
            }],
            session_grants: vec![],
        };
        let args = serde_json::json!({"command": "rm -rf /"});
        assert_eq!(policy.check("bash", &args), PermissionVerdict::Deny);
    }

    #[test]
    fn check_ask_when_no_rule_matches() {
        let policy = PermissionPolicy {
            allow: vec![],
            deny: vec![],
            session_grants: vec![],
        };
        let args = serde_json::json!({"command": "cargo test"});
        assert_eq!(policy.check("bash", &args), PermissionVerdict::Ask);
    }

    #[test]
    fn check_read_defaults_to_allow() {
        let policy = PermissionPolicy {
            allow: vec![],
            deny: vec![],
            session_grants: vec![],
        };
        let args = serde_json::json!({"file_path": "/src/main.rs"});
        assert_eq!(policy.check("read_file", &args), PermissionVerdict::Allow);
    }

    #[test]
    fn internal_tools_default_to_allow() {
        let policy = PermissionPolicy {
            allow: vec![],
            deny: vec![],
            session_grants: vec![],
        };
        // Internal tools should be allowed by default without any rules.
        let args = serde_json::json!({"key": "foo"});
        assert_eq!(policy.check("memory_read", &args), PermissionVerdict::Allow);
        assert_eq!(
            policy.check("memory_write", &args),
            PermissionVerdict::Allow
        );
        assert_eq!(
            policy.check("spawn_agent", &args),
            PermissionVerdict::Allow
        );
    }

    #[test]
    fn internal_tools_can_be_denied() {
        let policy = PermissionPolicy {
            allow: vec![],
            deny: vec![PermissionRule {
                tool: "spawn_agent".to_string(),
                pattern: "*".to_string(),
            }],
            session_grants: vec![],
        };
        let args = serde_json::json!({"name": "some-agent"});
        assert_eq!(
            policy.check("spawn_agent", &args),
            PermissionVerdict::Deny
        );
    }

    #[test]
    fn session_grant_survives_multiple_checks() {
        let mut policy = PermissionPolicy {
            allow: vec![],
            deny: vec![],
            session_grants: vec![],
        };
        let args = serde_json::json!({"command": "cargo test --release"});

        // Before grant, should Ask.
        assert_eq!(policy.check("bash", &args), PermissionVerdict::Ask);

        // Grant session permission.
        policy.grant_session("bash", &args);

        // After grant, should Allow on repeated checks.
        assert_eq!(policy.check("bash", &args), PermissionVerdict::Allow);
        assert_eq!(policy.check("bash", &args), PermissionVerdict::Allow);
        assert_eq!(policy.check("bash", &args), PermissionVerdict::Allow);
    }
}
