//! Interactive onboarding flow for first-run configuration.
//!
//! When no config exists or the config is incomplete (no API key resolvable,
//! no local provider), this module walks the user through setting up their
//! preferred provider and model.

use std::io::{BufRead, Write};
use std::path::Path;

use crate::config::{
    AnthropicConfig, AppConfig, ConfigError, OllamaConfig, ProvidersConfig, save_config,
};

/// The provider the user selected during onboarding.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ProviderChoice {
    Anthropic,
    Ollama,
}

/// Runs the interactive onboarding flow, prompting the user to configure
/// their LLM provider and model.
///
/// Reads from `input` and writes prompts to `output` so the flow is testable
/// without a real terminal.
///
/// # Arguments
///
/// * `config` - The current (possibly default/incomplete) configuration to update.
/// * `config_path` - Where to save the completed config.
/// * `input` - Reader for user input (stdin in production, buffer in tests).
/// * `output` - Writer for prompts (stdout in production, buffer in tests).
///
/// # Returns
///
/// The updated configuration after onboarding completes.
///
/// # Errors
///
/// Returns `ConfigError` if saving the config fails.
pub fn run_onboarding<R: BufRead, W: Write>(
    mut config: AppConfig,
    config_path: &Path,
    input: &mut R,
    output: &mut W,
) -> Result<AppConfig, ConfigError> {
    writeln!(output).ok();
    writeln!(output, "  Welcome to beezle! Let's get you set up.").ok();
    writeln!(output).ok();

    let provider = prompt_provider_choice(input, output);
    match provider {
        ProviderChoice::Anthropic => {
            let api_key_env = prompt_api_key_env(input, output);
            let model = prompt_model(input, output, "claude-sonnet-4-20250514");
            config.providers = ProvidersConfig {
                anthropic: Some(AnthropicConfig {
                    api_key_env,
                    model,
                    ..AnthropicConfig::default()
                }),
                ollama: None,
            };
        }
        ProviderChoice::Ollama => {
            let base_url =
                prompt_line(input, output, "  Ollama base URL", "http://localhost:11434");
            let model = prompt_model(input, output, "qwen2.5:14b");
            config.agent.default_provider = "ollama".into();
            config.providers = ProvidersConfig {
                anthropic: None,
                ollama: Some(OllamaConfig { base_url, model }),
            };
        }
    }

    save_config(&config, config_path)?;

    writeln!(output).ok();
    writeln!(output, "  Config saved to {}", config_path.display()).ok();
    writeln!(output, "  You're all set! Starting beezle...").ok();
    writeln!(output).ok();

    Ok(config)
}

/// Prompts the user to choose a provider. Returns `Anthropic` by default.
fn prompt_provider_choice<R: BufRead, W: Write>(input: &mut R, output: &mut W) -> ProviderChoice {
    writeln!(output, "  Which LLM provider would you like to use?").ok();
    writeln!(output).ok();
    writeln!(output, "    [1] Anthropic Claude (cloud, API key)").ok();
    writeln!(output, "    [2] Ollama (local, no API key needed)").ok();
    writeln!(output).ok();
    write!(output, "  Choice [1]: ").ok();
    output.flush().ok();

    let mut line = String::new();
    input.read_line(&mut line).ok();
    match line.trim() {
        "2" => ProviderChoice::Ollama,
        _ => ProviderChoice::Anthropic,
    }
}

/// Prompts for the environment variable name holding the Anthropic API key.
/// Warns if the variable is not currently set.
fn prompt_api_key_env<R: BufRead, W: Write>(input: &mut R, output: &mut W) -> String {
    let env_name = prompt_line(input, output, "  API key env var name", "ANTHROPIC_API_KEY");

    if std::env::var(&env_name).is_err() {
        writeln!(
            output,
            "  (warning: ${} is not set in your environment)",
            env_name
        )
        .ok();
    }

    env_name
}

/// Prompts the user for a model name with a default fallback.
fn prompt_model<R: BufRead, W: Write>(input: &mut R, output: &mut W, default: &str) -> String {
    prompt_line(input, output, "  Model", default)
}

/// Generic prompt that shows a label and default, returns the user's input
/// or the default if they press Enter.
fn prompt_line<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    label: &str,
    default: &str,
) -> String {
    write!(output, "{} [{}]: ", label, default).ok();
    output.flush().ok();

    let mut line = String::new();
    input.read_line(&mut line).ok();
    let trimmed = line.trim();
    if trimmed.is_empty() {
        default.to_owned()
    } else {
        trimmed.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tempfile::TempDir;

    /// Simulate user input by providing lines separated by newlines.
    fn fake_input(lines: &str) -> Cursor<Vec<u8>> {
        Cursor::new(lines.as_bytes().to_vec())
    }

    fn temp_config_path() -> (TempDir, std::path::PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        (dir, path)
    }

    #[test]
    fn onboarding_anthropic_with_defaults() {
        let (_dir, path) = temp_config_path();
        let config = AppConfig::default();

        // User presses Enter for all prompts (accepts defaults):
        // Choice: 1 (default), API key env: ANTHROPIC_API_KEY, Model: default
        let mut input = fake_input("\n\n\n");
        let mut output = Vec::new();

        let result = run_onboarding(config, &path, &mut input, &mut output).unwrap();

        assert!(result.providers.anthropic.is_some());
        let anthropic = result.providers.anthropic.unwrap();
        assert_eq!(anthropic.api_key_env, "ANTHROPIC_API_KEY");
        assert_eq!(anthropic.model, "claude-sonnet-4-20250514");
        assert!(result.providers.ollama.is_none());
        assert!(path.exists());
    }

    #[test]
    fn onboarding_anthropic_with_custom_values() {
        let (_dir, path) = temp_config_path();
        let config = AppConfig::default();

        // User selects Anthropic, custom env var, custom model
        let mut input = fake_input("1\nMY_CLAUDE_KEY\nclaude-opus-4-20250514\n");
        let mut output = Vec::new();

        let result = run_onboarding(config, &path, &mut input, &mut output).unwrap();

        let anthropic = result.providers.anthropic.unwrap();
        assert_eq!(anthropic.api_key_env, "MY_CLAUDE_KEY");
        assert_eq!(anthropic.model, "claude-opus-4-20250514");
    }

    #[test]
    fn onboarding_ollama_with_defaults() {
        let (_dir, path) = temp_config_path();
        let config = AppConfig::default();

        // User selects Ollama, then accepts default base_url and model
        let mut input = fake_input("2\n\n\n");
        let mut output = Vec::new();

        let result = run_onboarding(config, &path, &mut input, &mut output).unwrap();

        assert_eq!(result.agent.default_provider, "ollama");
        assert!(result.providers.anthropic.is_none());
        assert!(result.providers.ollama.is_some());
        let ollama = result.providers.ollama.unwrap();
        assert_eq!(ollama.base_url, "http://localhost:11434");
        assert_eq!(ollama.model, "qwen2.5:14b");
    }

    #[test]
    fn onboarding_ollama_with_custom_values() {
        let (_dir, path) = temp_config_path();
        let config = AppConfig::default();

        // User selects Ollama with custom URL and model
        let mut input = fake_input("2\nhttp://gpu-box:11434\nllama3:70b\n");
        let mut output = Vec::new();

        let result = run_onboarding(config, &path, &mut input, &mut output).unwrap();

        let ollama = result.providers.ollama.unwrap();
        assert_eq!(ollama.base_url, "http://gpu-box:11434");
        assert_eq!(ollama.model, "llama3:70b");
    }

    #[test]
    fn onboarding_saves_config_to_disk() {
        let (_dir, path) = temp_config_path();
        let config = AppConfig::default();

        let mut input = fake_input("1\n\nclaude-haiku-4-5-20251001\n");
        let mut output = Vec::new();

        run_onboarding(config, &path, &mut input, &mut output).unwrap();

        // Re-load from disk and verify
        let loaded = crate::config::load_config(Some(&path)).unwrap();
        let anthropic = loaded.providers.anthropic.unwrap();
        assert_eq!(anthropic.model, "claude-haiku-4-5-20251001");
    }

    #[test]
    fn onboarding_output_contains_welcome_message() {
        let (_dir, path) = temp_config_path();
        let config = AppConfig::default();

        let mut input = fake_input("\n\n\n");
        let mut output = Vec::new();

        run_onboarding(config, &path, &mut input, &mut output).unwrap();

        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("Welcome to beezle"));
        assert!(output_str.contains("Config saved to"));
    }

    #[test]
    fn onboarding_warns_about_missing_env_var() {
        let (_dir, path) = temp_config_path();
        let config = AppConfig::default();

        // Use an env var that definitely doesn't exist
        let mut input = fake_input("1\nBEEZLE_NONEXISTENT_VAR_99999\n\n");
        let mut output = Vec::new();

        run_onboarding(config, &path, &mut input, &mut output).unwrap();

        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("is not set"));
    }
}
