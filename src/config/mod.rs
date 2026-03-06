//! Configuration loading, validation, and persistence.
//!
//! Manages the `~/.beezle/config.toml` file and provides typed access
//! to all configuration sections. Creates default config and directory
//! structure on first run.

mod onboard;

pub use onboard::run_onboarding;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Top-level application configuration, serialized as TOML.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppConfig {
    /// Agent behavior settings.
    pub agent: AgentConfig,
    /// LLM provider credentials and preferences.
    pub providers: ProvidersConfig,
    /// Shell execution safety rules.
    pub shell: ShellConfig,
}

/// Agent identity and behavioral limits.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentConfig {
    /// Display name for the agent.
    pub name: String,
    /// Maximum agent loop iterations per turn.
    pub max_iterations: usize,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            name: "beezle".into(),
            max_iterations: 20,
        }
    }
}

/// Provider configuration container.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ProvidersConfig {
    /// Anthropic Claude provider settings.
    pub anthropic: Option<AnthropicConfig>,
    /// Local Ollama provider settings.
    pub ollama: Option<OllamaConfig>,
}

/// Anthropic API configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnthropicConfig {
    /// Name of the environment variable holding the API key.
    pub api_key_env: String,
    /// Default model identifier.
    pub model: String,
}

impl Default for AnthropicConfig {
    fn default() -> Self {
        Self {
            api_key_env: "ANTHROPIC_API_KEY".into(),
            model: "claude-sonnet-4-20250514".into(),
        }
    }
}

/// Ollama (local) provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OllamaConfig {
    /// Base URL for the Ollama API.
    pub base_url: String,
    /// Default model name.
    pub model: String,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434".into(),
            model: "qwen2.5:14b".into(),
        }
    }
}

/// Shell command execution safety settings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShellConfig {
    /// Directories the agent is allowed to operate in.
    pub allowed_dirs: Vec<String>,
    /// Shell commands that are explicitly blocked.
    pub blocked_commands: Vec<String>,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            allowed_dirs: vec!["~/.beezle".into()],
            blocked_commands: Vec::new(),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            agent: AgentConfig::default(),
            providers: ProvidersConfig {
                anthropic: Some(AnthropicConfig::default()),
                ollama: None,
            },
            shell: ShellConfig::default(),
        }
    }
}

/// Errors that can occur during configuration operations.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Failed to determine the user's home directory.
    #[error("could not determine home directory")]
    NoHomeDir,
    /// An I/O error occurred reading or writing config files.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    /// The TOML content could not be deserialized.
    #[error("failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),
    /// The config struct could not be serialized to TOML.
    #[error("failed to serialize config: {0}")]
    Serialize(#[from] toml::ser::Error),
}

/// Returns the default beezle home directory (`~/.beezle`).
///
/// # Errors
///
/// Returns `ConfigError::NoHomeDir` if the home directory cannot be determined.
pub fn beezle_home() -> Result<PathBuf, ConfigError> {
    dirs::home_dir()
        .map(|h| h.join(".beezle"))
        .ok_or(ConfigError::NoHomeDir)
}

/// Returns the default config file path (`~/.beezle/config.toml`).
///
/// # Errors
///
/// Returns `ConfigError::NoHomeDir` if the home directory cannot be determined.
pub fn default_config_path() -> Result<PathBuf, ConfigError> {
    beezle_home().map(|h| h.join("config.toml"))
}

/// Creates the `~/.beezle/` directory structure if it doesn't exist.
///
/// Creates the following subdirectories:
/// - `~/.beezle/sessions/`
/// - `~/.beezle/memory/`
/// - `~/.beezle/skills/`
///
/// # Errors
///
/// Returns `ConfigError::Io` if directory creation fails.
pub fn ensure_dirs() -> Result<PathBuf, ConfigError> {
    let home = beezle_home()?;
    for sub in &["sessions", "memory", "skills"] {
        std::fs::create_dir_all(home.join(sub))?;
    }
    Ok(home)
}

/// Loads configuration from the given path, or the default path if `None`.
///
/// If the config file does not exist, writes a default config and returns it.
///
/// # Arguments
///
/// * `path` - Optional explicit path to the config file. Falls back to
///   `~/.beezle/config.toml` if `None`.
///
/// # Errors
///
/// Returns `ConfigError` on I/O failures, parse errors, or if the home
/// directory cannot be determined.
pub fn load_config(path: Option<&Path>) -> Result<AppConfig, ConfigError> {
    let config_path = match path {
        Some(p) => p.to_path_buf(),
        None => default_config_path()?,
    };

    if !config_path.exists() {
        let default = AppConfig::default();
        save_config(&default, &config_path)?;
        return Ok(default);
    }

    let content = std::fs::read_to_string(&config_path)?;
    let config: AppConfig = toml::from_str(&content)?;
    Ok(config)
}

/// Writes a configuration to disk as TOML.
///
/// Creates parent directories if they don't exist.
///
/// # Arguments
///
/// * `config` - The configuration to persist.
/// * `path` - File path to write the TOML to.
///
/// # Errors
///
/// Returns `ConfigError` on I/O or serialization failures.
pub fn save_config(config: &AppConfig, path: &Path) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let toml_string = toml::to_string_pretty(config)?;
    std::fs::write(path, toml_string)?;
    Ok(())
}

/// Returns `true` if the config is ready to use (has a resolvable API key
/// or is configured for a local-only provider like Ollama).
///
/// # Arguments
///
/// * `config` - The configuration to check.
pub fn is_config_complete(config: &AppConfig) -> bool {
    // If Anthropic is configured, check that the env var is set
    if let Some(ref anthropic) = config.providers.anthropic
        && std::env::var(&anthropic.api_key_env).is_ok()
    {
        return true;
    }
    // Ollama doesn't need an API key
    config.providers.ollama.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // Helper: create a temp dir and return the config path inside it.
    fn temp_config_path() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        (dir, path)
    }

    #[test]
    fn default_config_has_anthropic_provider() {
        let config = AppConfig::default();
        assert!(config.providers.anthropic.is_some());
        assert!(config.providers.ollama.is_none());
    }

    #[test]
    fn default_config_roundtrips_through_toml() {
        let config = AppConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: AppConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(config, parsed);
    }

    #[test]
    fn load_config_creates_default_when_missing() {
        let (_dir, path) = temp_config_path();
        assert!(!path.exists());

        let config = load_config(Some(&path)).unwrap();
        assert!(path.exists());
        assert_eq!(config, AppConfig::default());
    }

    #[test]
    fn load_config_reads_existing_file() {
        let (_dir, path) = temp_config_path();

        let mut custom = AppConfig::default();
        custom.agent.name = "custom-agent".into();
        custom.agent.max_iterations = 42;
        save_config(&custom, &path).unwrap();

        let loaded = load_config(Some(&path)).unwrap();
        assert_eq!(loaded.agent.name, "custom-agent");
        assert_eq!(loaded.agent.max_iterations, 42);
    }

    #[test]
    fn save_config_creates_parent_dirs() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nested").join("deep").join("config.toml");

        let config = AppConfig::default();
        save_config(&config, &path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn is_config_complete_returns_false_without_key() {
        // Use a unique env var name that definitely doesn't exist.
        let mut config = AppConfig::default();
        config.providers.anthropic = Some(AnthropicConfig {
            api_key_env: "BEEZLE_TEST_NONEXISTENT_KEY_12345".into(),
            ..AnthropicConfig::default()
        });
        config.providers.ollama = None;
        assert!(!is_config_complete(&config));
    }

    #[test]
    fn is_config_complete_returns_true_with_ollama() {
        let mut config = AppConfig::default();
        config.providers.anthropic = None;
        config.providers.ollama = Some(OllamaConfig::default());
        assert!(is_config_complete(&config));
    }

    #[test]
    fn partial_toml_uses_defaults_for_missing_fields() {
        let toml_str = r#"
[agent]
name = "minimal"
max_iterations = 5

[providers]

[shell]
allowed_dirs = []
blocked_commands = []
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.agent.name, "minimal");
        assert_eq!(config.agent.max_iterations, 5);
        assert!(config.providers.anthropic.is_none());
    }

    #[test]
    fn config_error_on_invalid_toml() {
        let (_dir, path) = temp_config_path();
        fs::write(&path, "this is not valid toml {{{{").unwrap();

        let result = load_config(Some(&path));
        assert!(result.is_err());
    }

    #[test]
    fn ensure_dirs_creates_subdirectories() {
        // This test uses the real home dir; skip if we can't determine it.
        // We test the logic by checking ensure_dirs returns Ok.
        // The actual directories may already exist, which is fine.
        let result = ensure_dirs();
        assert!(result.is_ok());
    }
}
