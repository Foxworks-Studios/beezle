//! Beezle CLI entry point.
//!
//! Bootstraps configuration (with interactive onboarding on first run),
//! parses CLI arguments, then starts the agent REPL loop.

use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use clap::Parser;

use beezle::config::{self, AppConfig, is_config_complete, load_config, run_onboarding};
use beezle::context;
use beezle::session::SessionManager;
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
/// Wraps an `AgentTool` to print real-time execution feedback to stdout.
///
/// Since `agent.prompt()` awaits the full agent loop before returning events,
/// this wrapper is the only way to show tool activity as it happens. The
/// `execute` method prints a start line, delegates to the inner tool, then
/// prints success/failure — all during the loop, before `prompt()` returns.
struct ToolWrapper {
    inner: Box<dyn AgentTool>,
    use_color: bool,
}

#[async_trait::async_trait]
impl AgentTool for ToolWrapper {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn label(&self) -> &str {
        self.inner.label()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.inner.parameters_schema()
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        ctx: ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let (yellow, green, red, reset) = (
            color(YELLOW, self.use_color),
            color(GREEN, self.use_color),
            color(RED, self.use_color),
            color(RESET, self.use_color),
        );

        // Clear any thinking indicator and show tool start.
        clear_thinking_line();
        let summary = format_tool_summary(self.inner.name(), &params);
        print!("{yellow}  > {summary}{reset}");
        io::stdout().flush().ok();

        let result = self.inner.execute(params, ctx).await;

        // Print result status on the same line.
        match &result {
            Ok(_) => println!(" {green}ok{reset}"),
            Err(_) => println!(" {red}x{reset}"),
        }

        result
    }
}

/// Wraps all tools in `ToolWrapper` for real-time execution feedback.
fn wrap_tools(tools: Vec<Box<dyn AgentTool>>, use_color: bool) -> Vec<Box<dyn AgentTool>> {
    tools
        .into_iter()
        .map(|inner| -> Box<dyn AgentTool> { Box::new(ToolWrapper { inner, use_color }) })
        .collect()
}

/// A configured `Agent` ready for prompting.
///
/// Tools are wrapped in `ToolWrapper` to print real-time execution feedback
/// during the agent loop (since `prompt()` doesn't return events until done).
fn build_agent(
    config: &AppConfig,
    model: &str,
    api_key: &str,
    skills: SkillSet,
    system_prompt: &str,
    use_color: bool,
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

    agent = agent
        .with_system_prompt(system_prompt)
        .with_model(model)
        .with_api_key(api_key)
        .with_skills(skills)
        .with_tools(wrap_tools(default_tools(), use_color));

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

    let label = result.trim().to_owned();
    if label.is_empty() { None } else { Some(label) }
}

/// Clears the current thinking indicator line.
fn clear_thinking_line() {
    print!("\r                                        \r");
    io::stdout().flush().ok();
}

/// Runs a single prompt through the agent and prints the response.
///
/// Shows a thinking indicator while waiting for the agent loop to complete.
///
/// **Note:** `agent.prompt()` awaits the entire agent loop internally before
/// returning the event receiver. Events are buffered, not truly streamed to
/// us in real time. The thinking indicator covers this wait. Once we have
/// the receiver, we drain events immediately.
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
async fn run_single_prompt(
    agent: &mut Agent,
    prompt: &str,
    use_color: bool,
    thinking_api_key: Option<&str>,
) -> Usage {
    let (dim, reset_code) = (color(DIM, use_color), color(RESET, use_color));

    // Show thinking indicator immediately so the user sees feedback while
    // agent.prompt() runs the entire agent loop internally.
    let static_label = thinking_label();
    print!("{dim}  {static_label}...{reset_code}");
    io::stdout().flush().ok();

    // Optionally fire a Haiku call in parallel for a contextual label.
    // It races against agent.prompt() — whichever finishes first wins.
    let label_task = thinking_api_key.map(|key| {
        let key = key.to_owned();
        let msg = prompt.to_owned();
        tokio::spawn(async move { fetch_thinking_label(&key, &msg).await })
    });

    // agent.prompt() awaits the full agent loop — all events are buffered
    // in the unbounded channel by the time it returns.
    let rx = agent.prompt(prompt).await;

    // Clear the thinking indicator now that the loop is done.
    clear_thinking_line();

    // Abort the label task if it's still running — we no longer need it.
    if let Some(handle) = label_task {
        handle.abort();
    }

    // Drain buffered events and render output.
    render_events(rx, use_color).await
}

/// Drains agent events from the receiver and renders them to stdout.
///
/// # Arguments
///
/// * `rx` - The event receiver (events are already buffered).
/// * `use_color` - Whether to use ANSI color output.
///
/// # Returns
///
/// The final token usage from the turn.
async fn render_events(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<AgentEvent>,
    use_color: bool,
) -> Usage {
    let mut last_usage = Usage::default();
    let mut in_text = false;

    while let Some(event) = rx.recv().await {
        match event {
            // ToolExecutionStart/End are handled in real time by ToolWrapper
            // during the agent loop — skip them here to avoid duplicates.
            AgentEvent::ToolExecutionStart { .. } | AgentEvent::ToolExecutionEnd { .. } => {}
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

/// Returns a random thinking-state verb for the status indicator.
fn thinking_label() -> &'static str {
    const LABELS: &[&str] = &[
        "thinking",
        "pondering",
        "reasoning",
        "tinkering",
        "noodling",
        "mulling",
        "brewing",
        "conjuring",
        "scheming",
        "hatching",
    ];
    // Simple fast random: use lower bits of nanosecond timestamp.
    let idx = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as usize
        % LABELS.len();
    LABELS[idx]
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
    // Only use dynamic thinking labels for Anthropic (needs a Haiku call).
    let thinking_key: Option<&str> = if app_config.agent.default_provider != "ollama" {
        Some(&api_key)
    } else {
        None
    };
    let skills = load_skills(&cli.skills);
    tracing::debug!(%model, skills_count = skills.len(), "resolved model and skills");

    // Assemble system prompt: project context (if any) + base prompt.
    let cwd = std::env::current_dir()?;
    let project_context = context::load_project_context(&cwd, context::DEFAULT_MAX_CHARS);
    tracing::debug!(
        context_len = project_context.len(),
        "loaded project context"
    );
    let system_prompt = if project_context.is_empty() {
        SYSTEM_PROMPT.to_owned()
    } else {
        format!("{project_context}\n{SYSTEM_PROMPT}")
    };

    let mut agent = build_agent(
        &app_config,
        &model,
        &api_key,
        skills.clone(),
        &system_prompt,
        use_color,
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
        let usage = run_single_prompt(&mut agent, prompt_text, use_color, thinking_key).await;
        print_usage(&usage, use_color);
        // Save single-shot session too.
        if let Ok(json) = agent.save_messages() {
            let _ = session_mgr.save(&session_key, &json);
        }
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
            "/sessions" => {
                match session_mgr.list() {
                    Ok(sessions) if sessions.is_empty() => {
                        println!("{dim}  (no saved sessions){reset}\n");
                    }
                    Ok(sessions) => {
                        println!("{dim}  saved sessions:{reset}");
                        for s in &sessions {
                            let kb = s.size_bytes / 1024;
                            println!("{dim}    {:<30} ({kb} KB){reset}", s.key);
                        }
                        println!();
                    }
                    Err(e) => {
                        println!("{dim}  (error listing sessions: {e}){reset}\n");
                    }
                }
                continue;
            }
            s if s.starts_with("/save") => {
                let name = s.trim_start_matches("/save").trim();
                let key = if name.is_empty() {
                    session_key.as_str()
                } else {
                    // Update session key to the user-chosen name.
                    session_key = name.to_owned();
                    name
                };
                match agent.save_messages() {
                    Ok(json) => match session_mgr.save(key, &json) {
                        Ok(_) => println!("{dim}  (saved as '{key}'){reset}\n"),
                        Err(e) => println!("{dim}  (save error: {e}){reset}\n"),
                    },
                    Err(e) => println!("{dim}  (save error: {e}){reset}\n"),
                }
                continue;
            }
            s if s.starts_with("/model") => {
                let arg = s.trim_start_matches("/model").trim();
                if arg.is_empty() {
                    println!("{dim}  (model: {model}){reset}\n");
                } else {
                    model = arg.to_owned();
                    agent = build_agent(
                        &app_config,
                        &model,
                        &api_key,
                        skills.clone(),
                        &system_prompt,
                        use_color,
                    );
                    println!("{dim}  (switched to {model}, conversation cleared){reset}\n");
                }
                continue;
            }
            _ => {}
        }

        let usage = run_single_prompt(&mut agent, input, use_color, thinking_key).await;
        print_usage(&usage, use_color);
        println!();
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
