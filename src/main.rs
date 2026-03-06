//! Beezle CLI entry point.
//!
//! Bootstraps configuration (with interactive onboarding on first run),
//! parses CLI arguments, then starts the agent REPL loop.

use std::collections::HashMap;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use clap::Parser;

use beezle::agent::build_subagent;
use beezle::bus::{self, Response};
use beezle::channels::Channel;
use beezle::channels::terminal::TerminalChannel;
use beezle::config::{self, AppConfig, is_config_complete, load_config, run_onboarding};
use beezle::context;
use beezle::memory::{MemoryStore, SystemClock};
use beezle::session::SessionManager;
use beezle::tools::memory::{MemoryReadTool, MemoryWriteTool};
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
After making changes, run tests or verify the result when appropriate.

You have persistent memory that survives across sessions. Your long-term memory
(MEMORY.md) is already included in this prompt above. Use your memory tools to:
- Save user preferences, project conventions, and key decisions to long-term memory.
- When the user corrects you or states a preference, save it so you don't repeat the mistake.
- Use daily notes to record what you accomplished during a session.
- Read daily notes from previous days when you need context on recent work.
- Keep long-term memory concise: update or remove stale entries rather than appending indefinitely."#;

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
    println!("{dim}  /quit /clear /save /sessions /model{reset}\n");
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

/// Resolves the API key from the config, respecting `default_provider`.
///
/// For Anthropic, prefers OAuth token over env var. For Ollama, returns empty.
///
/// # Arguments
///
/// * `config` - The application config to read provider settings from.
///
/// # Returns
///
/// The API key string, or an empty string if using a local provider.
fn resolve_api_key(config: &AppConfig) -> String {
    match config.agent.default_provider.as_str() {
        "ollama" => String::new(),
        _ => config
            .providers
            .anthropic
            .as_ref()
            .map(|a| a.resolve_api_key())
            .unwrap_or_default(),
    }
}

/// Determines the model name from config, with an optional CLI override.
///
/// Respects `default_provider` to pick the right provider's model.
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
    match config.agent.default_provider.as_str() {
        "ollama" => config
            .providers
            .ollama
            .as_ref()
            .map(|o| o.model.clone())
            .unwrap_or_else(|| "qwen2.5:14b".into()),
        _ => config
            .providers
            .anthropic
            .as_ref()
            .map(|a| a.model.clone())
            .unwrap_or_else(|| "claude-sonnet-4-20250514".into()),
    }
}

/// Builds a yoagent `Agent` from the resolved configuration.
///
/// Uses `default_provider` to select `AnthropicProvider` or
/// `OpenAiCompatProvider` (for Ollama).
///
/// # Arguments
///
/// * `config` - The application config.
/// * `model` - The model identifier to use.
/// * `api_key` - The resolved API key.
/// * `skills` - Loaded skill set.
/// * `system_prompt` - The full system prompt (base + project context).
///
/// # Returns
///
/// Loads a [`MemoryStore`] rooted at `~/.beezle/memory/`.
///
/// Returns `None` if the home directory cannot be determined (logs a warning).
/// The store is created without touching the filesystem -- directory creation
/// happens lazily on first write.
///
/// # Returns
///
/// An `Arc<MemoryStore>` ready to be shared across tools, or `None` on failure.
fn load_memory_store() -> Option<Arc<MemoryStore>> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => {
            tracing::warn!("could not determine home directory; memory system disabled");
            return None;
        }
    };
    let memory_dir = home.join(".beezle").join("memory");
    Some(Arc::new(MemoryStore::new(
        memory_dir,
        Arc::new(SystemClock),
    )))
}

/// A configured `Agent` ready for prompting.
///
/// Assembles the provider, tools (default + subagent + memory), skills, and
/// system prompt into a fully wired `Agent`. Tools are passed unwrapped --
/// all rendering happens in `run_prompt()`'s event-driven loop.
fn build_agent(
    config: &AppConfig,
    model: &str,
    api_key: &str,
    skills: SkillSet,
    system_prompt: &str,
    memory_store: Option<Arc<MemoryStore>>,
) -> Agent {
    let is_ollama = config.agent.default_provider == "ollama";

    let mut agent = if is_ollama {
        let base_url = config
            .providers
            .ollama
            .as_ref()
            .map(|o| o.base_url.as_str())
            .unwrap_or("http://localhost:11434");
        Agent::new(OpenAiCompatProvider).with_model_config(ModelConfig::local(base_url, model))
    } else {
        Agent::new(AnthropicProvider)
    };

    // Build the sub-agent tool.
    let subagent = build_subagent(
        "spawn_agent",
        "Spawn a sub-agent to handle a focused task independently. \
         The sub-agent runs with a fresh context and returns only its final result.",
        "You are a helpful sub-agent. Complete the task you are given \
         thoroughly and return the result.",
        config,
        model,
        api_key,
    );

    let mut tools: Vec<Box<dyn AgentTool>> = default_tools();
    tools.push(Box::new(subagent));

    // Register memory tools when a MemoryStore is available.
    if let Some(store) = memory_store {
        tools.push(Box::new(MemoryReadTool::new(Arc::clone(&store))));
        tools.push(Box::new(MemoryWriteTool::new(store)));
    }

    agent = agent
        .with_system_prompt(system_prompt)
        .with_model(model)
        .with_api_key(api_key)
        .with_skills(skills)
        .with_tools(tools);

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

/// Returns a random capitalized thinking-state gerund for the status indicator.
///
/// Picks from a static list using the nanosecond timestamp as a simple fast
/// random source.
fn thinking_label() -> &'static str {
    const LABELS: &[&str] = &[
        "Thinking",
        "Pondering",
        "Reasoning",
        "Tinkering",
        "Noodling",
        "Mulling",
        "Brewing",
        "Conjuring",
        "Scheming",
        "Hatching",
    ];
    let idx = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as usize
        % LABELS.len();
    LABELS[idx]
}

/// Prints the status line showing the current thinking label.
///
/// Format matches Claude Code style: `* Verb...` with a colored asterisk.
fn print_status_line(label: &str, yellow: &str, dim: &str, reset: &str) {
    print!("\r{yellow}*{reset} {dim}{label}...{reset}");
    io::stdout().flush().ok();
}

/// Clears the current status/thinking line by overwriting with spaces.
fn clear_status_line() {
    print!("\r\x1b[2K");
    io::stdout().flush().ok();
}

/// Fetches a contextual thinking label by asking a fast model to generate
/// a whimsical gerund related to the user's message.
///
/// Returns `None` if the call fails or produces empty output.
///
/// # Arguments
///
/// * `api_key` - Anthropic API key or OAuth token.
/// * `user_prompt` - The user's message to derive the label from.
async fn fetch_thinking_label(api_key: &str, user_prompt: &str) -> Option<String> {
    let system = "\
Analyze this message and come up with a single positive, cheerful and delightful \
verb in gerund form that's related to the message. Only include the word with no \
other text or punctuation. The word should have the first letter capitalized. Add \
some whimsy and surprise to entertain the user. Ensure the word is highly relevant \
to the user's message. Synonyms are welcome, including obscure words. Be careful \
to avoid words that might look alarming or concerning to the software engineer \
seeing it as a status notification, such as Connecting, Disconnecting, Retrying, \
Lagging, Freezing, etc. NEVER use a destructive word, such as Terminating, \
Killing, Deleting, Destroying, Stopping, Exiting, or similar. NEVER use a word \
that may be derogatory, offensive, or inappropriate in a non-coding context, \
such as Penetrating.";

    let mut agent = Agent::new(AnthropicProvider)
        .with_model("claude-haiku-4-5-20251001")
        .with_api_key(api_key)
        .with_system_prompt(system);

    let mut rx = agent.prompt(user_prompt).await;
    let mut result = String::new();

    while let Some(event) = rx.recv().await {
        if let AgentEvent::MessageUpdate {
            delta: StreamDelta::Text { delta },
            ..
        } = event
        {
            result.push_str(&delta);
        }
    }

    agent.finish().await;

    let label = result.trim().to_owned();
    if label.is_empty() { None } else { Some(label) }
}

/// Runs a prompt through the agent using an event-driven loop.
///
/// Spawns the agent loop on a background task via `agent.prompt()`, then
/// processes streamed `AgentEvent`s in real time. Shows a thinking indicator
/// while waiting for the first event. Supports Ctrl+C cancellation via
/// `agent.abort()`. After the loop, `agent.finish()` restores agent state.
///
/// # Arguments
///
/// * `agent` - The agent to prompt.
/// * `prompt` - The user's prompt text.
/// * `use_color` - Whether to use ANSI color output.
/// * `thinking_api_key` - If `Some`, uses Haiku to generate a contextual
///   thinking label. Falls back to a random static label if `None` or on error.
///
/// # Returns
///
/// The final token usage from the turn.
async fn run_prompt(
    agent: &mut Agent,
    prompt: &str,
    use_color: bool,
    thinking_api_key: Option<&str>,
) -> Usage {
    let mut rx = agent.prompt(prompt).await;
    let mut last_usage = Usage::default();
    let mut tool_starts: HashMap<String, Instant> = HashMap::new();
    let mut streaming_text = false;

    let (yellow, green, red, dim, reset) = (
        color(YELLOW, use_color),
        color(GREEN, use_color),
        color(RED, use_color),
        color(DIM, use_color),
        color(RESET, use_color),
    );

    // Show a static fallback immediately so it never looks frozen.
    let mut current_label = thinking_label().to_owned();
    print_status_line(&current_label, yellow, dim, reset);

    // Optionally spawn a Haiku call to get a contextual label.
    let label_handle = thinking_api_key.map(|key| {
        let key = key.to_owned();
        let msg = prompt.to_owned();
        tokio::spawn(async move { fetch_thinking_label(&key, &msg).await })
    });
    let mut label_fut = label_handle.map(Box::pin);

    loop {
        tokio::select! {
            // If the contextual label arrives, update the status line.
            result = async { label_fut.as_mut().unwrap().as_mut().await },
                if label_fut.is_some() =>
            {
                label_fut = None;
                if let Ok(Some(label)) = result {
                    current_label = label;
                    if !streaming_text {
                        clear_status_line();
                        print_status_line(&current_label, yellow, dim, reset);
                    }
                }
            }
            event = rx.recv() => {
                let Some(event) = event else { break };

                match event {
                    AgentEvent::ToolExecutionStart { tool_call_id, tool_name, args, .. } => {
                        clear_status_line();
                        if streaming_text {
                            println!();
                            streaming_text = false;
                        }
                        tool_starts.insert(tool_call_id.clone(), Instant::now());
                        let summary = format_tool_summary(&tool_name, &args);
                        // Print tool line without newline; ToolExecutionEnd appends result.
                        print!("{yellow}  > {summary}{reset}");
                        io::stdout().flush().ok();
                    }
                    AgentEvent::ToolExecutionEnd { tool_call_id, is_error, .. } => {
                        let elapsed = tool_starts.remove(&tool_call_id)
                            .map(|t| t.elapsed());
                        let duration = elapsed
                            .map(|d| format!(" ({:.1}s)", d.as_secs_f64()))
                            .unwrap_or_default();
                        if is_error {
                            println!(" {red}x{duration}{reset}");
                        } else {
                            println!(" {green}ok{duration}{reset}");
                        }
                        print_status_line(&current_label, yellow, dim, reset);
                    }
                    AgentEvent::MessageUpdate { delta: StreamDelta::Text { delta }, .. } => {
                        // During text streaming, hide the status line.
                        if !streaming_text {
                            clear_status_line();
                            streaming_text = true;
                        }
                        print!("{delta}");
                        io::stdout().flush().ok();
                    }
                    AgentEvent::MessageUpdate { delta: StreamDelta::Thinking { delta }, .. } => {
                        if !streaming_text {
                            clear_status_line();
                            streaming_text = true;
                        }
                        print!("{dim}{delta}{reset}");
                        io::stdout().flush().ok();
                    }
                    AgentEvent::MessageEnd { message: AgentMessage::Llm(Message::Assistant {
                        stop_reason: StopReason::Error, error_message, ..
                    }) } => {
                        clear_status_line();
                        if streaming_text {
                            println!();
                            streaming_text = false;
                        }
                        let msg = error_message.as_deref().unwrap_or("unknown error");
                        tracing::error!("{msg}");
                        println!("{red}  error: {msg}{reset}");
                        print_status_line(&current_label, yellow, dim, reset);
                    }
                    AgentEvent::MessageEnd { .. } => {
                        // Non-error message end: text streaming done, re-show status.
                        if streaming_text {
                            println!();
                            streaming_text = false;
                        }
                        clear_status_line();
                        print_status_line(&current_label, yellow, dim, reset);
                    }
                    AgentEvent::AgentEnd { messages } => {
                        for msg in messages.iter().rev() {
                            if let AgentMessage::Llm(Message::Assistant { usage, .. }) = msg {
                                last_usage = usage.clone();
                                break;
                            }
                        }
                    }
                    AgentEvent::ProgressMessage { text, .. } => {
                        clear_status_line();
                        println!("{dim}  {text}{reset}");
                        print_status_line(&current_label, yellow, dim, reset);
                    }
                    AgentEvent::InputRejected { reason } => {
                        clear_status_line();
                        println!("{red}  rejected: {reason}{reset}");
                        print_status_line(&current_label, yellow, dim, reset);
                    }
                    _ => {}
                }
            }
            _ = tokio::signal::ctrl_c() => {
                agent.abort();
                break;
            }
        }
    }

    // Clean up the persistent status line.
    clear_status_line();
    if streaming_text {
        println!();
    }

    agent.finish().await;

    last_usage
}

/// Maximum number of characters to include from memory content in the system prompt.
const MEMORY_MAX_CHARS: usize = 4000;

/// Builds the effective system prompt by optionally appending memory content.
///
/// If `memory_content` is empty, returns `base` unchanged. Otherwise, appends
/// a `## Persistent Memory` section. Content exceeding [`MEMORY_MAX_CHARS`] is
/// truncated with a `[truncated]` suffix.
///
/// # Arguments
///
/// * `base` - The base system prompt (project context + core prompt).
/// * `memory_content` - Raw content from `MEMORY.md`, possibly empty.
///
/// # Returns
///
/// The assembled system prompt string.
fn build_effective_system_prompt(base: &str, memory_content: &str) -> String {
    if memory_content.is_empty() {
        return base.to_owned();
    }

    let truncated = if memory_content.len() > MEMORY_MAX_CHARS {
        // Truncate at a char boundary, then append marker.
        let end = memory_content
            .char_indices()
            .take_while(|(i, _)| *i < MEMORY_MAX_CHARS)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(MEMORY_MAX_CHARS);
        format!("{}[truncated]", &memory_content[..end])
    } else {
        memory_content.to_owned()
    };

    format!("{base}\n\n## Persistent Memory\n{truncated}")
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

/// Result of processing a potential slash command.
enum SlashResult {
    /// The command was `/quit` or `/exit`.
    Quit,
    /// A slash command was handled; contains the response message.
    Handled(String),
    /// Not a slash command — should be sent to the agent.
    NotSlash,
}

/// Checks if `input` is a slash command and handles it if so.
///
/// Mutates agent/model/session state as needed for commands like `/clear`,
/// `/model`, `/save`. Returns a `SlashResult` indicating what happened.
#[allow(clippy::too_many_arguments)]
fn handle_slash_command(
    input: &str,
    agent: &mut Agent,
    model: &mut String,
    session_key: &mut String,
    app_config: &AppConfig,
    api_key: &str,
    skills: &SkillSet,
    system_prompt: &str,
    session_mgr: &SessionManager,
    dim: &str,
    reset: &str,
) -> SlashResult {
    match input {
        "/quit" | "/exit" => SlashResult::Quit,
        "/clear" => {
            agent.clear_messages();
            SlashResult::Handled(format!("{dim}  (conversation cleared){reset}\n"))
        }
        "/sessions" => {
            let msg = match session_mgr.list() {
                Ok(sessions) if sessions.is_empty() => {
                    format!("{dim}  (no saved sessions){reset}\n")
                }
                Ok(sessions) => {
                    let mut out = format!("{dim}  saved sessions:{reset}\n");
                    for s in &sessions {
                        let kb = s.size_bytes / 1024;
                        out.push_str(&format!("{dim}    {:<30} ({kb} KB){reset}\n", s.key));
                    }
                    out
                }
                Err(e) => format!("{dim}  (error listing sessions: {e}){reset}\n"),
            };
            SlashResult::Handled(msg)
        }
        s if s.starts_with("/save") => {
            let name = s.trim_start_matches("/save").trim();
            let key = if name.is_empty() {
                session_key.as_str()
            } else {
                *session_key = name.to_owned();
                name
            };
            let msg = match agent.save_messages() {
                Ok(json) => match session_mgr.save(key, &json) {
                    Ok(_) => format!("{dim}  (saved as '{key}'){reset}\n"),
                    Err(e) => format!("{dim}  (save error: {e}){reset}\n"),
                },
                Err(e) => format!("{dim}  (save error: {e}){reset}\n"),
            };
            SlashResult::Handled(msg)
        }
        s if s.starts_with("/model") => {
            let arg = s.trim_start_matches("/model").trim();
            let msg = if arg.is_empty() {
                format!("{dim}  (model: {model}){reset}\n")
            } else {
                *model = arg.to_owned();
                *agent = build_agent(
                    app_config,
                    model,
                    api_key,
                    skills.clone(),
                    system_prompt,
                    None,
                );
                format!("{dim}  (switched to {model}, conversation cleared){reset}\n")
            };
            SlashResult::Handled(msg)
        }
        _ => SlashResult::NotSlash,
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let use_color = !cli.no_color;

    // Initialize structured logging.
    // --verbose overrides to debug; otherwise BEEZLE_LOG env var or default warn.
    let log_filter = if cli.verbose {
        "debug".to_owned()
    } else {
        std::env::var("BEEZLE_LOG").unwrap_or_else(|_| "warn".into())
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_new(&log_filter)
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_ansi(use_color)
        .init();

    // Ensure ~/.beezle/ directory structure exists.
    config::ensure_dirs()?;

    // Load or create config.
    let config_path = match cli.config {
        Some(ref p) => p.clone(),
        None => config::default_config_path()?,
    };
    let mut app_config = load_config(Some(&config_path))?;
    tracing::debug!(config = %config_path.display(), "loaded configuration");

    // If config is incomplete, run interactive onboarding.
    if !is_config_complete(&app_config) {
        let stdin = io::stdin();
        let mut reader = stdin.lock();
        let mut writer = io::stdout();
        app_config = run_onboarding(app_config, &config_path, &mut reader, &mut writer)?;
    }

    // Initialize session manager.
    let beezle_home = config::beezle_home()?;
    let session_mgr = SessionManager::new(&beezle_home.join("sessions"))?;
    let mut session_key = SessionManager::generate_key();

    let mut model = resolve_model(&app_config, cli.model.as_deref());
    let api_key = resolve_api_key(&app_config);
    let skills = load_skills(&cli.skills);
    tracing::debug!(%model, skills_count = skills.len(), "resolved model and skills");

    // Assemble system prompt: project context (if any) + base prompt.
    let cwd = std::env::current_dir()?;
    let project_context = context::load_project_context(&cwd, context::DEFAULT_MAX_CHARS);
    tracing::debug!(
        context_len = project_context.len(),
        "loaded project context"
    );
    let base_prompt = if project_context.is_empty() {
        SYSTEM_PROMPT.to_owned()
    } else {
        format!("{project_context}\n{SYSTEM_PROMPT}")
    };

    // Load persistent memory and inject into the system prompt.
    let memory_store = load_memory_store();
    let memory_content = memory_store
        .as_ref()
        .and_then(|store| match store.read_long_term() {
            Ok(content) => Some(content),
            Err(e) => {
                tracing::warn!("failed to read MEMORY.md: {e}");
                None
            }
        })
        .unwrap_or_default();
    let system_prompt = build_effective_system_prompt(&base_prompt, &memory_content);

    // Only use dynamic thinking labels for Anthropic (needs a Haiku call).
    let thinking_key: Option<&str> = if app_config.agent.default_provider != "ollama" {
        Some(&api_key)
    } else {
        None
    };

    let mut agent = build_agent(
        &app_config,
        &model,
        &api_key,
        skills.clone(),
        &system_prompt,
        memory_store,
    );

    // Resume a previous session if requested.
    if let Some(ref resume_key) = cli.resume {
        let (dim, reset) = (color(DIM, use_color), color(RESET, use_color));
        let key = if resume_key.is_empty() {
            // --resume with no key: load most recent.
            session_mgr.most_recent()?
        } else {
            Some(resume_key.clone())
        };

        match key {
            Some(k) => match session_mgr.load(&k) {
                Ok(json) => {
                    agent
                        .restore_messages(&json)
                        .map_err(|e| anyhow::anyhow!("failed to restore session: {e}"))?;
                    session_key = k.clone();
                    println!("{dim}  (resumed session: {k}){reset}");
                }
                Err(e) => {
                    println!("{dim}  (could not resume: {e}){reset}");
                }
            },
            None => {
                println!("{dim}  (no previous sessions found){reset}");
            }
        }
    }

    // Single-shot mode: run one prompt and exit.
    if let Some(ref prompt_text) = cli.prompt {
        let usage = run_prompt(&mut agent, prompt_text, use_color, thinking_key).await;
        print_usage(&usage, use_color);
        // Save single-shot session too.
        if let Ok(json) = agent.save_messages() {
            let _ = session_mgr.save(&session_key, &json);
        }
        return Ok(());
    }

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

    // Create the command bus and spawn the terminal channel.
    let (command_bus, mut bus_rx) = bus::command_bus(16);
    let terminal_channel = TerminalChannel::new(use_color);
    tokio::spawn(async move {
        if let Err(e) = terminal_channel.run(command_bus).await {
            tracing::error!("terminal channel error: {e}");
        }
    });

    // Consume commands from the bus instead of reading stdin directly.
    while let Some(cmd) = bus_rx.recv().await {
        let input = cmd.content.trim().to_owned();
        let (dim, reset) = (color(DIM, use_color), color(RESET, use_color));

        // Handle slash commands on the consumer side.
        // Compute a response message; None means it's a regular prompt.
        let slash_response = handle_slash_command(
            &input,
            &mut agent,
            &mut model,
            &mut session_key,
            &app_config,
            &api_key,
            &skills,
            &system_prompt,
            &session_mgr,
            dim,
            reset,
        );

        match slash_response {
            SlashResult::Quit => {
                let _ = cmd.response_tx.send(Response {
                    content: String::new(),
                });
                break;
            }
            SlashResult::Handled(msg) => {
                let _ = cmd.response_tx.send(Response { content: msg });
                continue;
            }
            SlashResult::NotSlash => {
                // Regular prompt — send to agent, respond via oneshot.
                let usage = run_prompt(&mut agent, &input, use_color, thinking_key).await;
                print_usage(&usage, use_color);
                println!();

                // Send empty response — output was already streamed to stdout.
                let _ = cmd.response_tx.send(Response {
                    content: String::new(),
                });
            }
        }
    }

    // Auto-save session on exit.
    let (dim, reset) = (color(DIM, use_color), color(RESET, use_color));
    if let Ok(json) = agent.save_messages()
        && !json.is_empty()
        && json != "[]"
    {
        match session_mgr.save(&session_key, &json) {
            Ok(_) => println!("{dim}  (session saved: {session_key}){reset}"),
            Err(e) => tracing::warn!("failed to save session on exit: {e}"),
        }
    }
    println!("{dim}  bye{reset}\n");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use yoagent::provider::MockProvider;

    /// Helper to parse CLI args from a slice, simulating command-line invocation.
    fn parse_cli(args: &[&str]) -> Result<Cli, clap::Error> {
        // Prepend the binary name as clap expects it.
        let mut full_args = vec!["beezle"];
        full_args.extend_from_slice(args);
        Cli::try_parse_from(full_args)
    }

    /// Helper to build a mock agent for testing. Uses `MockProvider` so no
    /// real API calls are made.
    fn mock_agent(response: &str) -> Agent {
        Agent::new(MockProvider::text(response))
            .with_system_prompt("test")
            .with_model("mock")
            .with_api_key("test-key")
    }

    /// Helper to call `handle_slash_command` with default test fixtures.
    /// Returns the `SlashResult` along with the potentially-mutated model
    /// and session key.
    fn run_slash(
        input: &str,
        agent: &mut Agent,
        session_mgr: &SessionManager,
    ) -> (SlashResult, String, String) {
        let config = AppConfig::default();
        let skills = SkillSet::empty();
        let mut model = "mock-model".to_owned();
        let mut session_key = "test-session".to_owned();

        let result = handle_slash_command(
            input,
            agent,
            &mut model,
            &mut session_key,
            &config,
            "test-key",
            &skills,
            "test prompt",
            session_mgr,
            "",
            "",
        );

        (result, model, session_key)
    }

    // -----------------------------------------------------------------------
    // CLI parsing tests
    // -----------------------------------------------------------------------

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

    // -----------------------------------------------------------------------
    // resolve_model tests
    // -----------------------------------------------------------------------

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

    // -----------------------------------------------------------------------
    // color helper tests
    // -----------------------------------------------------------------------

    #[test]
    fn color_returns_code_when_enabled() {
        assert_eq!(color(BOLD, true), BOLD);
    }

    #[test]
    fn color_returns_empty_when_disabled() {
        assert_eq!(color(BOLD, false), "");
    }

    // -----------------------------------------------------------------------
    // truncate tests
    // -----------------------------------------------------------------------

    #[test]
    fn truncate_short_string_unchanged() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_at_exact_length() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn truncate_cuts_long_string() {
        assert_eq!(truncate("hello world", 5), "hello");
    }

    #[test]
    fn truncate_preserves_multibyte_boundaries() {
        // Each char here is multi-byte; ensure we don't panic or split mid-char.
        let s = "abcde";
        assert_eq!(truncate(s, 3), "abc");
    }

    #[test]
    fn truncate_empty_string() {
        assert_eq!(truncate("", 5), "");
    }

    // -----------------------------------------------------------------------
    // format_tool_summary tests
    // -----------------------------------------------------------------------

    #[test]
    fn format_tool_summary_bash() {
        let args = serde_json::json!({"command": "ls -la"});
        assert_eq!(format_tool_summary("bash", &args), "$ ls -la");
    }

    #[test]
    fn format_tool_summary_bash_truncates_long_command() {
        let long_cmd = "x".repeat(200);
        let args = serde_json::json!({"command": long_cmd});
        let summary = format_tool_summary("bash", &args);
        // "$ " prefix + 80 chars max.
        assert!(summary.len() <= 82 + 2);
        assert!(summary.starts_with("$ "));
    }

    #[test]
    fn format_tool_summary_read_file() {
        let args = serde_json::json!({"path": "/src/main.rs"});
        assert_eq!(format_tool_summary("read_file", &args), "read /src/main.rs");
    }

    #[test]
    fn format_tool_summary_write_file() {
        let args = serde_json::json!({"path": "/tmp/out.txt"});
        assert_eq!(
            format_tool_summary("write_file", &args),
            "write /tmp/out.txt"
        );
    }

    #[test]
    fn format_tool_summary_edit_file() {
        let args = serde_json::json!({"path": "/src/lib.rs"});
        assert_eq!(format_tool_summary("edit_file", &args), "edit /src/lib.rs");
    }

    #[test]
    fn format_tool_summary_list_files() {
        let args = serde_json::json!({"path": "/src"});
        assert_eq!(format_tool_summary("list_files", &args), "ls /src");
    }

    #[test]
    fn format_tool_summary_list_files_defaults_to_dot() {
        let args = serde_json::json!({});
        assert_eq!(format_tool_summary("list_files", &args), "ls .");
    }

    #[test]
    fn format_tool_summary_search() {
        let args = serde_json::json!({"pattern": "fn main"});
        assert_eq!(format_tool_summary("search", &args), "search 'fn main'");
    }

    #[test]
    fn format_tool_summary_unknown_tool() {
        let args = serde_json::json!({});
        assert_eq!(format_tool_summary("custom_tool", &args), "custom_tool");
    }

    // -----------------------------------------------------------------------
    // handle_slash_command tests
    // -----------------------------------------------------------------------

    #[test]
    fn slash_quit_returns_quit() {
        let tmp = tempfile::tempdir().unwrap();
        let session_mgr = SessionManager::new(tmp.path()).unwrap();
        let mut agent = mock_agent("ok");
        let (result, _, _) = run_slash("/quit", &mut agent, &session_mgr);
        assert!(matches!(result, SlashResult::Quit));
    }

    #[test]
    fn slash_exit_returns_quit() {
        let tmp = tempfile::tempdir().unwrap();
        let session_mgr = SessionManager::new(tmp.path()).unwrap();
        let mut agent = mock_agent("ok");
        let (result, _, _) = run_slash("/exit", &mut agent, &session_mgr);
        assert!(matches!(result, SlashResult::Quit));
    }

    #[test]
    fn slash_clear_clears_messages_and_returns_handled() {
        let tmp = tempfile::tempdir().unwrap();
        let session_mgr = SessionManager::new(tmp.path()).unwrap();
        let mut agent = mock_agent("ok");

        let (result, _, _) = run_slash("/clear", &mut agent, &session_mgr);
        assert!(
            matches!(result, SlashResult::Handled(ref msg) if msg.contains("conversation cleared"))
        );
        assert!(agent.messages().is_empty());
    }

    #[test]
    fn slash_sessions_empty_shows_no_sessions() {
        let tmp = tempfile::tempdir().unwrap();
        let session_mgr = SessionManager::new(tmp.path()).unwrap();
        let mut agent = mock_agent("ok");

        let (result, _, _) = run_slash("/sessions", &mut agent, &session_mgr);
        assert!(
            matches!(result, SlashResult::Handled(ref msg) if msg.contains("no saved sessions"))
        );
    }

    #[test]
    fn slash_sessions_lists_existing_sessions() {
        let tmp = tempfile::tempdir().unwrap();
        let session_mgr = SessionManager::new(tmp.path()).unwrap();
        session_mgr
            .save("my-session", r#"[{"role":"user"}]"#)
            .unwrap();
        let mut agent = mock_agent("ok");

        let (result, _, _) = run_slash("/sessions", &mut agent, &session_mgr);
        assert!(matches!(result, SlashResult::Handled(ref msg) if msg.contains("my-session")));
    }

    #[test]
    fn slash_save_uses_default_session_key() {
        let tmp = tempfile::tempdir().unwrap();
        let session_mgr = SessionManager::new(tmp.path()).unwrap();
        let mut agent = mock_agent("ok");

        let (result, _, session_key) = run_slash("/save", &mut agent, &session_mgr);
        assert!(matches!(result, SlashResult::Handled(ref msg) if msg.contains("saved as")));
        // Session key should remain the default "test-session".
        assert_eq!(session_key, "test-session");
    }

    #[test]
    fn slash_save_with_name_updates_session_key() {
        let tmp = tempfile::tempdir().unwrap();
        let session_mgr = SessionManager::new(tmp.path()).unwrap();
        let mut agent = mock_agent("ok");

        let (result, _, session_key) = run_slash("/save my-name", &mut agent, &session_mgr);
        assert!(matches!(result, SlashResult::Handled(ref msg) if msg.contains("my-name")));
        assert_eq!(session_key, "my-name");
    }

    #[test]
    fn slash_model_bare_shows_current_model() {
        let tmp = tempfile::tempdir().unwrap();
        let session_mgr = SessionManager::new(tmp.path()).unwrap();
        let mut agent = mock_agent("ok");

        let (result, model, _) = run_slash("/model", &mut agent, &session_mgr);
        assert!(matches!(result, SlashResult::Handled(ref msg) if msg.contains("mock-model")));
        assert_eq!(model, "mock-model");
    }

    #[test]
    fn slash_model_with_arg_switches_model() {
        let tmp = tempfile::tempdir().unwrap();
        let session_mgr = SessionManager::new(tmp.path()).unwrap();
        let mut agent = mock_agent("ok");

        let (result, model, _) = run_slash("/model new-model", &mut agent, &session_mgr);
        assert!(
            matches!(result, SlashResult::Handled(ref msg) if msg.contains("switched to new-model"))
        );
        assert_eq!(model, "new-model");
    }

    #[test]
    fn non_slash_input_returns_not_slash() {
        let tmp = tempfile::tempdir().unwrap();
        let session_mgr = SessionManager::new(tmp.path()).unwrap();
        let mut agent = mock_agent("ok");

        let (result, _, _) = run_slash("hello world", &mut agent, &session_mgr);
        assert!(matches!(result, SlashResult::NotSlash));
    }

    #[test]
    fn unknown_slash_returns_not_slash() {
        let tmp = tempfile::tempdir().unwrap();
        let session_mgr = SessionManager::new(tmp.path()).unwrap();
        let mut agent = mock_agent("ok");

        let (result, _, _) = run_slash("/unknown", &mut agent, &session_mgr);
        assert!(matches!(result, SlashResult::NotSlash));
    }

    // -----------------------------------------------------------------------
    // Bus consumer integration tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn bus_slash_command_returns_response_via_oneshot() {
        let tmp = tempfile::tempdir().unwrap();
        let session_mgr = SessionManager::new(tmp.path()).unwrap();
        let mut agent = mock_agent("ok");
        let config = AppConfig::default();
        let skills = SkillSet::empty();
        let mut model = "mock".to_owned();
        let mut session_key = "test".to_owned();

        // Create bus, send a /clear command, consume it.
        let (bus, mut rx) = beezle::bus::command_bus(1);
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();

        bus.send(beezle::bus::Command {
            source: beezle::bus::ChannelKind::Terminal,
            content: "/clear".into(),
            response_tx: resp_tx,
        })
        .await
        .unwrap();

        let cmd = rx.recv().await.unwrap();
        let input = cmd.content.trim().to_owned();
        let (dim, reset) = (color(DIM, false), color(RESET, false));

        let result = handle_slash_command(
            &input,
            &mut agent,
            &mut model,
            &mut session_key,
            &config,
            "test-key",
            &skills,
            "test",
            &session_mgr,
            dim,
            reset,
        );

        if let SlashResult::Handled(msg) = result {
            cmd.response_tx.send(Response { content: msg }).unwrap();
        } else {
            panic!("expected SlashResult::Handled");
        }

        let response = resp_rx.await.unwrap();
        assert!(response.content.contains("conversation cleared"));
    }

    #[tokio::test]
    async fn bus_regular_prompt_processes_through_mock_agent() {
        let (bus, mut rx) = beezle::bus::command_bus(1);
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();

        bus.send(beezle::bus::Command {
            source: beezle::bus::ChannelKind::Terminal,
            content: "hello agent".into(),
            response_tx: resp_tx,
        })
        .await
        .unwrap();

        let cmd = rx.recv().await.unwrap();
        assert_eq!(cmd.content, "hello agent");

        // Simulate the consumer: it's not a slash command, so run agent.
        let mut agent = mock_agent("agent response");
        let _usage = run_prompt(&mut agent, &cmd.content, false, None).await;

        // Send empty response (output was streamed).
        cmd.response_tx
            .send(Response {
                content: String::new(),
            })
            .unwrap();

        let response = resp_rx.await.unwrap();
        // Response content is empty because streaming happened via wrappers.
        assert!(response.content.is_empty());

        // But the agent did process the prompt.
        assert_eq!(agent.messages().len(), 2);
    }

    // -----------------------------------------------------------------------
    // build_agent_tools_include_spawn_agent
    // -----------------------------------------------------------------------

    #[test]
    fn build_agent_tools_include_spawn_agent() {
        // Verify that default_tools() plus the unwrapped subagent tool
        // yields a list containing "spawn_agent".
        use beezle::agent::build_subagent;

        let config = AppConfig::default();
        let subagent = build_subagent(
            "spawn_agent",
            "Spawn a sub-agent to handle a focused task independently.",
            "You are a helpful sub-agent. Complete the task thoroughly and return the result.",
            &config,
            "claude-sonnet-4-20250514",
            "test-key",
        );

        let mut tools: Vec<Box<dyn AgentTool>> = default_tools();
        tools.push(Box::new(subagent));

        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(
            names.contains(&"spawn_agent"),
            "tools should contain spawn_agent, got: {names:?}"
        );
        // spawn_agent should be in addition to default tools.
        assert!(tools.len() > default_tools().len());
    }

    // -----------------------------------------------------------------------
    // build_effective_system_prompt tests
    // -----------------------------------------------------------------------

    #[test]
    fn build_effective_system_prompt_empty_memory_returns_base() {
        let base = "You are a helpful assistant.";
        let result = build_effective_system_prompt(base, "");
        assert_eq!(result, base);
    }

    #[test]
    fn build_effective_system_prompt_appends_memory_under_limit() {
        let base = "You are a helpful assistant.";
        let memory = "User prefers Rust.";
        let result = build_effective_system_prompt(base, memory);
        assert!(result.starts_with(base));
        assert!(result.contains("\n\n## Persistent Memory\n"));
        assert!(result.contains(memory));
    }

    #[test]
    fn build_effective_system_prompt_truncates_over_4000_chars() {
        let base = "You are a helpful assistant.";
        let memory = "x".repeat(5000);
        let result = build_effective_system_prompt(base, &memory);
        assert!(result.contains("[truncated]"));
        // The memory section (after the header) should not exceed 4000 chars
        // plus the "[truncated]" suffix.
        let header = "\n\n## Persistent Memory\n";
        let memory_section = result.strip_prefix(base).unwrap();
        assert!(memory_section.starts_with(header));
        let content = memory_section.strip_prefix(header).unwrap();
        // 4000 chars of memory + "[truncated]" = 4011
        assert!(content.len() <= 4000 + "[truncated]".len());
    }

    // -----------------------------------------------------------------------
    // run_prompt tests (TDD red step -- run_prompt does not exist yet)
    //
    // These tests target the future `run_prompt()` function specified in
    // PRD 011. They will fail to compile until `run_prompt` is implemented.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn run_prompt_returns_usage_from_agent_end() {
        let mut agent = mock_agent("hello");

        // run_prompt will replace run_single_prompt with a streamlined
        // event-driven loop. MockProvider returns zero usage by default.
        let usage = run_prompt(&mut agent, "hi", false, None).await;

        assert_eq!(usage.input, 0);
        assert_eq!(usage.output, 0);
        // Agent should accumulate user + assistant messages.
        assert_eq!(agent.messages().len(), 2);
    }

    #[tokio::test]
    async fn run_prompt_accumulates_usage_across_turns() {
        let provider = MockProvider::texts(vec!["First", "Second"]);
        let mut agent = Agent::new(provider)
            .with_system_prompt("test")
            .with_model("mock")
            .with_api_key("test-key");

        let _ = run_prompt(&mut agent, "msg1", false, None).await;
        assert_eq!(agent.messages().len(), 2);

        let _ = run_prompt(&mut agent, "msg2", false, None).await;
        assert_eq!(agent.messages().len(), 4);
    }

    #[test]
    fn thinking_label_returns_nonempty_capitalized_gerund() {
        let label = thinking_label();
        assert!(!label.is_empty(), "thinking label should not be empty");
        // First character should be uppercase (capitalized gerunds).
        assert!(
            label.chars().next().unwrap().is_ascii_uppercase(),
            "thinking label should start with an uppercase letter, got: {label}"
        );
    }

    #[tokio::test]
    async fn run_prompt_processes_tool_execution_events_without_panic() {
        use yoagent::provider::mock::{MockResponse, MockToolCall};
        use yoagent::tools::default_tools;

        // Set up a mock provider that first requests a tool call (bash echo),
        // then produces a final text response. The agent loop will emit
        // ToolExecutionStart and ToolExecutionEnd events for the tool call.
        let provider = MockProvider::new(vec![
            MockResponse::ToolCalls(vec![MockToolCall {
                name: "bash".into(),
                arguments: serde_json::json!({"command": "echo ok"}),
            }]),
            MockResponse::Text("Done.".into()),
        ]);
        let mut agent = Agent::new(provider)
            .with_system_prompt("test")
            .with_model("mock")
            .with_api_key("test-key")
            .with_tools(default_tools());

        // Should process tool execution events and return without panicking.
        let _usage = run_prompt(&mut agent, "run a command", false, None).await;
    }
}
