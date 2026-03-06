//! Beezle CLI entry point.
//!
//! Bootstraps configuration (with interactive onboarding on first run),
//! parses CLI arguments, then starts the agent REPL loop.

use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use clap::Parser;

use beezle::config::{self, AppConfig, is_config_complete, load_config, run_onboarding};
use yoagent::agent::Agent;
use yoagent::provider::{AnthropicProvider, ModelConfig, OpenAiCompatProvider};
use yoagent::skills::SkillSet;
use yoagent::tools::default_tools;
use yoagent::*;

// ANSI color helpers — gated by `use_color`.
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const CYAN: &str = "\x1b[36m";
const RED: &str = "\x1b[31m";

const SYSTEM_PROMPT: &str = r#"You are a coding assistant working in the user's terminal.
You have access to the filesystem and shell. Be direct and concise.
When the user asks you to do something, do it -- don't just explain how.
Use tools proactively: read files to understand context, run commands to verify your work.
After making changes, run tests or verify the result when appropriate."#;

/// Beezle — AI coding agent CLI.
#[derive(Parser, Debug)]
#[command(name = "beezle", version, about = "AI coding agent powered by yoagent")]
struct Cli {
    /// Override the model from config (e.g. claude-opus-4-6).
    #[arg(long)]
    model: Option<String>,

    /// Resume a previous session. Optionally provide a session key;
    /// omit to resume the most recent session.
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    resume: Option<String>,

    /// Run a single prompt and exit (non-interactive mode).
    #[arg(long)]
    prompt: Option<String>,

    /// Additional skill directories (can be specified multiple times).
    #[arg(long, action = clap::ArgAction::Append)]
    skills: Vec<PathBuf>,

    /// Path to config file (default: ~/.beezle/config.toml).
    #[arg(long)]
    config: Option<PathBuf>,

    /// Enable verbose (debug-level) logging.
    #[arg(long)]
    verbose: bool,

    /// Disable colored output.
    #[arg(long)]
    no_color: bool,
}

/// Returns the ANSI escape code if color is enabled, or empty string if not.
///
/// # Arguments
///
/// * `code` - The ANSI escape code to conditionally return.
/// * `use_color` - Whether color output is enabled.
fn color(code: &str, use_color: bool) -> &str {
    if use_color { code } else { "" }
}

fn print_banner(use_color: bool) {
    let (bold, cyan, dim, reset) = (
        color(BOLD, use_color),
        color(CYAN, use_color),
        color(DIM, use_color),
        color(RESET, use_color),
    );
    println!("\n{bold}{cyan}  beezle{reset} {dim}-- ai coding agent{reset}");
    println!("{dim}  Type /quit to exit, /clear to reset{reset}\n");
}

fn print_usage(usage: &Usage, use_color: bool) {
    if usage.input > 0 || usage.output > 0 {
        let (dim, reset) = (color(DIM, use_color), color(RESET, use_color));
        println!(
            "\n{dim}  tokens: {} in / {} out{reset}",
            usage.input, usage.output
        );
    }
}

/// Resolves the API key from the config's environment variable reference.
///
/// # Arguments
///
/// * `config` - The application config to read provider settings from.
///
/// # Returns
///
/// The API key string, or an empty string if using a local provider.
fn resolve_api_key(config: &AppConfig) -> String {
    if let Some(ref anthropic) = config.providers.anthropic {
        std::env::var(&anthropic.api_key_env).unwrap_or_default()
    } else {
        String::new()
    }
}

/// Determines the model name from config, with an optional CLI override.
///
/// # Arguments
///
/// * `config` - The application config to read provider settings from.
/// * `cli_override` - If `Some`, use this model instead of the config value.
///
/// # Returns
///
/// The model identifier string.
fn resolve_model(config: &AppConfig, cli_override: Option<&str>) -> String {
    if let Some(m) = cli_override {
        return m.to_owned();
    }
    if let Some(ref anthropic) = config.providers.anthropic {
        anthropic.model.clone()
    } else if let Some(ref ollama) = config.providers.ollama {
        ollama.model.clone()
    } else {
        "claude-sonnet-4-20250514".into()
    }
}

/// Builds a yoagent `Agent` from the resolved configuration.
///
/// # Arguments
///
/// * `config` - The application config.
/// * `model` - The model identifier to use.
/// * `api_key` - The resolved API key.
/// * `skills` - Loaded skill set.
///
/// # Returns
///
/// A configured `Agent` ready for prompting.
fn build_agent(config: &AppConfig, model: &str, api_key: &str, skills: SkillSet) -> Agent {
    let mut agent = if let Some(ref ollama) = config.providers.ollama {
        Agent::new(OpenAiCompatProvider)
            .with_model_config(ModelConfig::local(&ollama.base_url, model))
    } else {
        Agent::new(AnthropicProvider)
    };

    agent = agent
        .with_system_prompt(SYSTEM_PROMPT)
        .with_model(model)
        .with_api_key(api_key)
        .with_skills(skills)
        .with_tools(default_tools());

    agent
}

/// Truncates a string to `max` characters, preserving char boundaries.
fn truncate(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

/// Collects skill directories from CLI args and the default ~/.beezle/skills/ path.
///
/// # Arguments
///
/// * `cli_dirs` - Additional skill directories from `--skills` flags.
///
/// # Returns
///
/// A loaded `SkillSet`, or `SkillSet::empty()` if none found.
fn load_skills(cli_dirs: &[PathBuf]) -> SkillSet {
    let mut dirs: Vec<String> = cli_dirs
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    // Add default skills dir if it exists.
    if let Ok(home) = config::beezle_home() {
        let default_dir = home.join("skills");
        if default_dir.exists() {
            dirs.push(default_dir.to_string_lossy().to_string());
        }
    }

    if dirs.is_empty() {
        return SkillSet::empty();
    }

    SkillSet::load(&dirs).unwrap_or_else(|e| {
        tracing::error!("failed to load skills: {e}");
        SkillSet::empty()
    })
}

/// Runs a single prompt through the agent and prints the response.
///
/// # Arguments
///
/// * `agent` - The agent to prompt.
/// * `prompt` - The user's prompt text.
/// * `use_color` - Whether to use ANSI color output.
///
/// # Returns
///
/// The final token usage from the turn.
async fn run_single_prompt(agent: &mut Agent, prompt: &str, use_color: bool) -> Usage {
    let mut rx = agent.prompt(prompt).await;
    let mut last_usage = Usage::default();
    let mut in_text = false;

    while let Some(event) = rx.recv().await {
        match event {
            AgentEvent::ToolExecutionStart {
                tool_name, args, ..
            } => {
                if in_text {
                    println!();
                    in_text = false;
                }
                let (yellow, reset) = (color(YELLOW, use_color), color(RESET, use_color));
                let summary = format_tool_summary(&tool_name, &args);
                print!("{yellow}  > {summary}{reset}");
                io::stdout().flush().ok();
            }
            AgentEvent::ToolExecutionEnd { is_error, .. } => {
                let (green, red, reset) = (
                    color(GREEN, use_color),
                    color(RED, use_color),
                    color(RESET, use_color),
                );
                if is_error {
                    println!(" {red}x{reset}");
                } else {
                    println!(" {green}ok{reset}");
                }
            }
            AgentEvent::MessageUpdate {
                delta: StreamDelta::Text { delta },
                ..
            } => {
                if !in_text {
                    println!();
                    in_text = true;
                }
                print!("{delta}");
                io::stdout().flush().ok();
            }
            AgentEvent::MessageEnd {
                message:
                    AgentMessage::Llm(Message::Assistant {
                        stop_reason: StopReason::Error,
                        error_message,
                        ..
                    }),
            } => {
                if in_text {
                    println!();
                    in_text = false;
                }
                let (red, reset) = (color(RED, use_color), color(RESET, use_color));
                let msg = error_message.as_deref().unwrap_or("unknown error");
                tracing::error!("{msg}");
                println!("{red}  error: {msg}{reset}");
            }
            AgentEvent::AgentEnd { messages } => {
                for msg in messages.iter().rev() {
                    if let AgentMessage::Llm(Message::Assistant { usage, .. }) = msg {
                        last_usage = usage.clone();
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    if in_text {
        println!();
    }

    last_usage
}

/// Formats a human-readable summary of a tool invocation.
///
/// # Arguments
///
/// * `tool_name` - Name of the tool being called.
/// * `args` - The tool's arguments as a JSON value (expected to be an object).
///
/// # Returns
///
/// A short string summarizing the tool call.
fn format_tool_summary(tool_name: &str, args: &serde_json::Value) -> String {
    match tool_name {
        "bash" => {
            let cmd = args
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("...");
            format!("$ {}", truncate(cmd, 80))
        }
        "read_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            format!("read {path}")
        }
        "write_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            format!("write {path}")
        }
        "edit_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            format!("edit {path}")
        }
        "list_files" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            format!("ls {path}")
        }
        "search" => {
            let pat = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("?");
            format!("search '{}'", truncate(pat, 60))
        }
        _ => tool_name.to_owned(),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let use_color = !cli.no_color;

    // Ensure ~/.beezle/ directory structure exists.
    config::ensure_dirs()?;

    // Load or create config.
    let config_path = match cli.config {
        Some(ref p) => p.clone(),
        None => config::default_config_path()?,
    };
    let mut app_config = load_config(Some(&config_path))?;

    // If config is incomplete, run interactive onboarding.
    if !is_config_complete(&app_config) {
        let stdin = io::stdin();
        let mut reader = stdin.lock();
        let mut writer = io::stdout();
        app_config = run_onboarding(app_config, &config_path, &mut reader, &mut writer)?;
    }

    // Handle --resume stub.
    if let Some(ref key) = cli.resume {
        let (dim, reset) = (color(DIM, use_color), color(RESET, use_color));
        if key.is_empty() {
            println!("{dim}  (session resume not yet implemented){reset}");
        } else {
            println!("{dim}  (session resume for '{key}' not yet implemented){reset}");
        }
    }

    let model = resolve_model(&app_config, cli.model.as_deref());
    let api_key = resolve_api_key(&app_config);
    let skills = load_skills(&cli.skills);

    let mut agent = build_agent(&app_config, &model, &api_key, skills.clone());

    // Single-shot mode: run one prompt and exit.
    if let Some(ref prompt_text) = cli.prompt {
        let usage = run_single_prompt(&mut agent, prompt_text, use_color).await;
        print_usage(&usage, use_color);
        return Ok(());
    }

    // Graceful Ctrl+C exit.
    let color_flag = use_color;
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        let (dim, reset) = (color(DIM, color_flag), color(RESET, color_flag));
        println!("\n{dim}  bye{reset}\n");
        std::process::exit(0);
    });

    print_banner(use_color);
    let (dim, reset) = (color(DIM, use_color), color(RESET, use_color));
    println!("{dim}  model: {model}{reset}");
    if !skills.is_empty() {
        println!("{dim}  skills: {} loaded{reset}", skills.len());
    }
    println!(
        "{dim}  cwd:   {}{reset}\n",
        std::env::current_dir()?.display()
    );

    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();

    loop {
        let (bold, green, reset) = (
            color(BOLD, use_color),
            color(GREEN, use_color),
            color(RESET, use_color),
        );
        print!("{bold}{green}> {reset}");
        io::stdout().flush().ok();

        let line = match lines.next() {
            Some(Ok(l)) => l,
            _ => break,
        };

        let input = line.trim();
        if input.is_empty() {
            continue;
        }

        // Slash commands.
        let dim = color(DIM, use_color);
        match input {
            "/quit" | "/exit" => break,
            "/clear" => {
                agent.clear_messages();
                println!("{dim}  (conversation cleared){reset}\n");
                continue;
            }
            s if s.starts_with("/model ") => {
                let new_model = s.trim_start_matches("/model ").trim();
                agent = build_agent(&app_config, new_model, &api_key, skills.clone());
                println!("{dim}  (switched to {new_model}, conversation cleared){reset}\n");
                continue;
            }
            _ => {}
        }

        let usage = run_single_prompt(&mut agent, input, use_color).await;
        print_usage(&usage, use_color);
        println!();
    }

    let (dim, reset) = (color(DIM, use_color), color(RESET, use_color));
    println!("\n{dim}  bye{reset}\n");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to parse CLI args from a slice, simulating command-line invocation.
    fn parse_cli(args: &[&str]) -> Result<Cli, clap::Error> {
        // Prepend the binary name as clap expects it.
        let mut full_args = vec!["beezle"];
        full_args.extend_from_slice(args);
        Cli::try_parse_from(full_args)
    }

    #[test]
    fn cli_parses_no_args() {
        let cli = parse_cli(&[]).unwrap();
        assert!(cli.model.is_none());
        assert!(cli.prompt.is_none());
        assert!(cli.resume.is_none());
        assert!(cli.skills.is_empty());
        assert!(cli.config.is_none());
        assert!(!cli.verbose);
        assert!(!cli.no_color);
    }

    #[test]
    fn cli_parses_model_flag() {
        let cli = parse_cli(&["--model", "claude-opus-4-6"]).unwrap();
        assert_eq!(cli.model.as_deref(), Some("claude-opus-4-6"));
    }

    #[test]
    fn cli_parses_prompt_flag() {
        let cli = parse_cli(&["--prompt", "hello world"]).unwrap();
        assert_eq!(cli.prompt.as_deref(), Some("hello world"));
    }

    #[test]
    fn cli_parses_resume_without_key() {
        let cli = parse_cli(&["--resume"]).unwrap();
        // When --resume is given without a value, it uses the default_missing_value "".
        assert_eq!(cli.resume.as_deref(), Some(""));
    }

    #[test]
    fn cli_parses_resume_with_key() {
        let cli = parse_cli(&["--resume", "my-session"]).unwrap();
        assert_eq!(cli.resume.as_deref(), Some("my-session"));
    }

    #[test]
    fn cli_parses_multiple_skills() {
        let cli = parse_cli(&["--skills", "/a", "--skills", "/b"]).unwrap();
        assert_eq!(cli.skills.len(), 2);
        assert_eq!(cli.skills[0], PathBuf::from("/a"));
        assert_eq!(cli.skills[1], PathBuf::from("/b"));
    }

    #[test]
    fn cli_parses_config_path() {
        let cli = parse_cli(&["--config", "/tmp/my-config.toml"]).unwrap();
        assert_eq!(
            cli.config.as_deref(),
            Some(std::path::Path::new("/tmp/my-config.toml"))
        );
    }

    #[test]
    fn cli_parses_verbose_flag() {
        let cli = parse_cli(&["--verbose"]).unwrap();
        assert!(cli.verbose);
    }

    #[test]
    fn cli_parses_no_color_flag() {
        let cli = parse_cli(&["--no-color"]).unwrap();
        assert!(cli.no_color);
    }

    #[test]
    fn cli_rejects_unknown_flags() {
        let result = parse_cli(&["--bogus"]);
        assert!(result.is_err());
    }

    #[test]
    fn cli_parses_combined_flags() {
        let cli = parse_cli(&[
            "--model",
            "claude-haiku-4-5-20251001",
            "--verbose",
            "--no-color",
            "--skills",
            "/my/skills",
            "--prompt",
            "do stuff",
        ])
        .unwrap();
        assert_eq!(cli.model.as_deref(), Some("claude-haiku-4-5-20251001"));
        assert!(cli.verbose);
        assert!(cli.no_color);
        assert_eq!(cli.skills.len(), 1);
        assert_eq!(cli.prompt.as_deref(), Some("do stuff"));
    }

    #[test]
    fn resolve_model_prefers_cli_override() {
        let config = AppConfig::default();
        let result = resolve_model(&config, Some("my-custom-model"));
        assert_eq!(result, "my-custom-model");
    }

    #[test]
    fn resolve_model_falls_back_to_config() {
        let config = AppConfig::default();
        let result = resolve_model(&config, None);
        assert_eq!(result, "claude-sonnet-4-20250514");
    }

    #[test]
    fn color_returns_code_when_enabled() {
        assert_eq!(color(BOLD, true), BOLD);
    }

    #[test]
    fn color_returns_empty_when_disabled() {
        assert_eq!(color(BOLD, false), "");
    }

    #[test]
    fn format_tool_summary_bash() {
        let args = serde_json::json!({"command": "ls -la"});
        assert_eq!(format_tool_summary("bash", &args), "$ ls -la");
    }

    #[test]
    fn format_tool_summary_unknown_tool() {
        let args = serde_json::json!({});
        assert_eq!(format_tool_summary("custom_tool", &args), "custom_tool");
    }
}
