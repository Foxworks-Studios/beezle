//! Sub-agent definitions and YAML front-matter parsing.
//!
//! Provides [`SubAgentDef`] for declaring sub-agents (built-in or user-defined),
//! [`builtin_sub_agents()`] for the hardcoded agent roster, and internal parsing
//! helpers for loading agent definitions from Markdown files with YAML front matter.

use std::path::Path;
use std::sync::Arc;

use serde::Deserialize;
use tracing::{debug, warn};
use yoagent::AgentTool;
use yoagent::provider::StreamProvider;
use yoagent::sub_agent::SubAgentTool;
use yoagent::tools::{
    BashTool, EditFileTool, ListFilesTool, ReadFileTool, SearchTool, WriteFileTool,
};

use crate::config::{AppConfig, ModelEntry};

/// A declarative sub-agent definition (built-in or user-defined).
///
/// Each definition describes a sub-agent's identity, model preference, tool set,
/// and system prompt. Built-in definitions are returned by [`builtin_sub_agents()`];
/// user-defined definitions are parsed from Markdown files via [`parse_agent_file()`].
#[derive(Debug, Clone, PartialEq)]
pub struct SubAgentDef {
    /// Unique name for the sub-agent (e.g. `"explorer"`).
    pub name: String,
    /// Human-readable description of what the sub-agent does.
    pub description: String,
    /// Model to use. If `None`, inherits the parent coordinator's model.
    pub model: Option<String>,
    /// Maximum number of agent loop turns. If `None`, uses yoagent's default.
    pub max_turns: Option<usize>,
    /// Tool names as strings (e.g. `"read_file"`). If empty, gets `default_tools()`.
    pub tools: Vec<String>,
    /// System prompt that defines the sub-agent's behavior.
    pub system_prompt: String,
}

/// YAML front-matter structure for deserialization.
#[derive(Debug, Deserialize)]
struct FrontMatter {
    name: Option<String>,
    description: Option<String>,
    model: Option<String>,
    max_turns: Option<usize>,
    #[serde(default)]
    tools: Vec<String>,
}

/// Constructs a [`SubAgentTool`] from a [`SubAgentDef`].
///
/// Resolves the model from `def.model` (falling back to `parent_model` when
/// `None`), calls [`tools_for_names()`] to build the tool set, and optionally
/// sets `max_turns` when specified in the definition.
///
/// # Arguments
///
/// * `def` - The sub-agent definition to build from.
/// * `provider` - The stream provider for the sub-agent's LLM calls.
/// * `parent_model` - Fallback model when `def.model` is `None`.
/// * `api_key` - API key for the provider.
///
/// # Returns
///
/// A fully configured [`SubAgentTool`] ready to register with the parent agent.
pub fn build_sub_agent(
    def: &SubAgentDef,
    provider: Arc<dyn StreamProvider>,
    parent_model: &str,
    api_key: &str,
) -> SubAgentTool {
    let resolved_model = def.model.as_deref().unwrap_or(parent_model);

    debug!(
        agent_name = %def.name,
        model = %resolved_model,
        "building sub-agent"
    );

    let tools = tools_for_names(&def.tools);

    let mut tool = SubAgentTool::new(&def.name, provider)
        .with_description(&def.description)
        .with_system_prompt(&def.system_prompt)
        .with_model(resolved_model)
        .with_api_key(api_key)
        .with_tools(tools);

    if let Some(max_turns) = def.max_turns {
        tool = tool.with_max_turns(max_turns);
    }

    tool
}

/// Returns the hardcoded built-in sub-agent definitions.
///
/// The three built-in agents are:
/// - `explorer` -- fast, read-only file exploration (Haiku model)
/// - `researcher` -- deep research with structured summaries (Sonnet model)
/// - `coder` -- writes, edits, and tests code (Opus model)
///
/// Model fields use Anthropic model IDs by default; callers should map them
/// to the active provider's equivalent when building the actual sub-agent.
pub fn builtin_sub_agents() -> Vec<SubAgentDef> {
    vec![
        SubAgentDef {
            name: "explorer".into(),
            description: "Searches and lists files to answer quick questions about the codebase. Fast and cheap.".into(),
            model: Some("claude-haiku-4-5-20251001".into()),
            max_turns: None,
            tools: vec![
                "read_file".into(),
                "search".into(),
                "list_files".into(),
            ],
            system_prompt: "You are a fast file explorer. Your job is to search, list, and read files to answer questions about the codebase. Return concise answers. Do not modify any files.".into(),
        },
        SubAgentDef {
            name: "researcher".into(),
            description: "Researches topics by reading files, searching code, and exploring the codebase in depth.".into(),
            model: Some("claude-sonnet-4-6".into()),
            max_turns: None,
            tools: vec![
                "read_file".into(),
                "search".into(),
                "list_files".into(),
            ],
            system_prompt: "You are a thorough researcher. Read files, search code, and explore the codebase to build a deep understanding of the topic. Return structured summaries with sources and file references.".into(),
        },
        SubAgentDef {
            name: "coder".into(),
            description: "Writes, edits, and tests code. Use for implementation tasks.".into(),
            model: Some("claude-opus-4-6".into()),
            max_turns: None,
            tools: vec![
                "read_file".into(),
                "write_file".into(),
                "edit_file".into(),
                "bash".into(),
            ],
            system_prompt: "You are an expert coder. Write clean, correct, and well-tested code. Follow the project's conventions and patterns. Run tests to verify your changes.".into(),
        },
    ]
}

/// Resolves tool-name strings to yoagent [`AgentTool`] instances.
///
/// Maps the six known tool names (`read_file`, `write_file`, `edit_file`,
/// `list_files`, `search`, `bash`) to their corresponding yoagent tool
/// constructors. Unrecognized names emit a [`tracing::warn!`] and are skipped.
///
/// # Arguments
///
/// * `names` - Slice of tool name strings to resolve.
///
/// # Returns
///
/// A vec of `Arc<dyn AgentTool>` containing only the recognized tools,
/// in the same order as the input (minus skipped unknowns).
pub fn tools_for_names(names: &[String]) -> Vec<Arc<dyn AgentTool>> {
    names
        .iter()
        .filter_map(|name| {
            let tool: Option<Arc<dyn AgentTool>> = match name.as_str() {
                "read_file" => Some(Arc::new(ReadFileTool::default())),
                "write_file" => Some(Arc::new(WriteFileTool::new())),
                "edit_file" => Some(Arc::new(EditFileTool::new())),
                "list_files" => Some(Arc::new(ListFilesTool::default())),
                "search" => Some(Arc::new(SearchTool::default())),
                "bash" => Some(Arc::new(BashTool::default())),
                unknown => {
                    warn!(tool_name = unknown, "unrecognized tool name, skipping");
                    None
                }
            };
            tool
        })
        .collect()
}

/// Generates the Markdown section for the coordinator's system prompt.
///
/// Produces a prompt fragment describing each available sub-agent (name,
/// description, model) and optionally an `## Available Models` section
/// when multiple models are configured.
///
/// # Arguments
///
/// * `agents` - Sub-agent definitions to include in the prompt.
/// * `model_roster` - Configured models. An `## Available Models` section
///   is included only when this slice contains more than one entry.
///
/// # Returns
///
/// A Markdown string suitable for appending to the coordinator's system prompt.
pub fn coordinator_agent_prompt(agents: &[SubAgentDef], model_roster: &[ModelEntry]) -> String {
    let mut out = String::new();

    if !agents.is_empty() {
        out.push_str("## Available Sub-Agents\n\n");
        for agent in agents {
            out.push_str(&format!("### `{}`\n", agent.name));
            out.push_str(&format!("{}\n", agent.description));
            if let Some(ref model) = agent.model {
                out.push_str(&format!("- **Model:** {}\n", model));
            } else {
                out.push_str("- **Model:** inherits coordinator model\n");
            }
            out.push('\n');
        }
    }

    if model_roster.len() > 1 {
        out.push_str("## Available Models\n\n");
        for entry in model_roster {
            out.push_str(&format!("- **{}** ({})", entry.id, entry.provider));
            if !entry.guidance.is_empty() {
                out.push_str(&format!(" — {}", entry.guidance));
            }
            out.push('\n');
        }
        out.push('\n');
    }

    out
}

/// Parses a Markdown file with YAML front matter into a [`SubAgentDef`].
///
/// Expects the content to have YAML front matter delimited by `---` lines,
/// with a Markdown body that becomes the system prompt. Required front-matter
/// fields are `name` and `description`. Optional fields are `model`,
/// `max_turns`, and `tools`.
///
/// # Errors
///
/// Returns `Err` if:
/// - The `---` delimiter is absent or the front matter cannot be extracted
/// - The YAML block is malformed
/// - `name` or `description` is missing or empty
pub fn parse_agent_file(content: &str) -> Result<SubAgentDef, String> {
    // Find the opening and closing `---` delimiters.
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Err("missing opening '---' delimiter".into());
    }

    // Skip past the first `---` line.
    let after_open = &trimmed[3..];
    let after_open = after_open.strip_prefix('\n').unwrap_or(after_open);

    let close_pos = after_open
        .find("\n---")
        .ok_or_else(|| "missing closing '---' delimiter".to_string())?;

    let yaml_block = &after_open[..close_pos];
    let body_start = close_pos + 4; // skip "\n---"
    let body = if body_start < after_open.len() {
        after_open[body_start..].trim()
    } else {
        ""
    };

    let fm: FrontMatter =
        serde_yaml::from_str(yaml_block).map_err(|e| format!("invalid YAML: {e}"))?;

    let name = fm
        .name
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "missing or empty 'name' field".to_string())?;

    let description = fm
        .description
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "missing or empty 'description' field".to_string())?;

    Ok(SubAgentDef {
        name,
        description,
        model: fm.model,
        max_turns: fm.max_turns,
        tools: fm.tools,
        system_prompt: body.to_string(),
    })
}

/// Scans the given directory for `*.md` files and parses each into a [`SubAgentDef`].
///
/// Files that fail to parse (missing fields, bad YAML, etc.) are logged with
/// `tracing::warn!` and skipped. Returns an empty vec if the directory does
/// not exist.
///
/// # Arguments
///
/// * `dir` - Path to the directory to scan for agent definition files.
pub fn load_user_sub_agents_from(dir: &Path) -> Vec<SubAgentDef> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut agents = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                warn!(path = %path.display(), error = %e, "failed to read agent file");
                continue;
            }
        };

        match parse_agent_file(&content) {
            Ok(def) => agents.push(def),
            Err(e) => {
                warn!(path = %path.display(), error = %e, "failed to parse agent file");
            }
        }
    }

    agents
}

/// Scans `~/.beezle/agents/*.md` for user-defined sub-agent definitions.
///
/// Convenience wrapper around [`load_user_sub_agents_from`] using the default
/// agents directory. Returns an empty vec if the directory does not exist.
pub fn load_user_sub_agents() -> Vec<SubAgentDef> {
    let dir = match dirs::home_dir() {
        Some(home) => home.join(".beezle").join("agents"),
        None => return Vec::new(),
    };
    load_user_sub_agents_from(&dir)
}

/// Returns the default Anthropic model tier entries.
fn anthropic_model_roster() -> Vec<ModelEntry> {
    vec![
        ModelEntry {
            id: "claude-haiku-4-5-20251001".into(),
            provider: "anthropic".into(),
            guidance: "Fast and cheap. Use for simple lookups, formatting, classification, and tasks that don't need deep reasoning.".into(),
        },
        ModelEntry {
            id: "claude-sonnet-4-6".into(),
            provider: "anthropic".into(),
            guidance: "Balanced speed and capability. Use for research, summarization, code review, and moderate complexity tasks.".into(),
        },
        ModelEntry {
            id: "claude-opus-4-6".into(),
            provider: "anthropic".into(),
            guidance: "Most capable. Use for complex implementation, architecture decisions, subtle bugs, and tasks requiring deep reasoning.".into(),
        },
    ]
}

/// Returns the model roster based on the active configuration.
///
/// When the default provider is `"anthropic"`, returns the three standard
/// Anthropic tier entries (Haiku, Sonnet, Opus) plus any user-configured
/// `[[models]]` entries from the config. When the provider is `"ollama"`,
/// returns an empty vec (local providers typically have a single model).
///
/// # Arguments
///
/// * `config` - The application configuration to read provider and model info from.
pub fn load_model_roster(config: &AppConfig) -> Vec<ModelEntry> {
    match config.agent.default_provider.as_str() {
        "anthropic" => {
            let mut roster = anthropic_model_roster();
            roster.extend(config.models.iter().cloned());
            roster
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------
    // builtin_sub_agents() tests
    // ---------------------------------------------------------------

    #[test]
    fn builtin_sub_agents_returns_three_entries() {
        let agents = builtin_sub_agents();
        assert_eq!(agents.len(), 3);
    }

    #[test]
    fn builtin_sub_agents_names() {
        let agents = builtin_sub_agents();
        let names: Vec<&str> = agents.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["explorer", "researcher", "coder"]);
    }

    #[test]
    fn builtin_explorer_uses_haiku_model() {
        let agents = builtin_sub_agents();
        let explorer = &agents[0];
        assert_eq!(explorer.name, "explorer");
        assert_eq!(explorer.model.as_deref(), Some("claude-haiku-4-5-20251001"));
    }

    #[test]
    fn builtin_researcher_uses_sonnet_model() {
        let agents = builtin_sub_agents();
        let researcher = &agents[1];
        assert_eq!(researcher.name, "researcher");
        assert_eq!(researcher.model.as_deref(), Some("claude-sonnet-4-6"));
    }

    #[test]
    fn builtin_coder_uses_opus_model() {
        let agents = builtin_sub_agents();
        let coder = &agents[2];
        assert_eq!(coder.name, "coder");
        assert_eq!(coder.model.as_deref(), Some("claude-opus-4-6"));
    }

    #[test]
    fn builtin_explorer_has_read_only_tools() {
        let agents = builtin_sub_agents();
        let explorer = &agents[0];
        assert_eq!(explorer.tools, vec!["read_file", "search", "list_files"]);
    }

    #[test]
    fn builtin_researcher_has_read_only_tools() {
        let agents = builtin_sub_agents();
        let researcher = &agents[1];
        assert_eq!(researcher.tools, vec!["read_file", "search", "list_files"]);
    }

    #[test]
    fn builtin_coder_has_write_tools() {
        let agents = builtin_sub_agents();
        let coder = &agents[2];
        assert_eq!(
            coder.tools,
            vec!["read_file", "write_file", "edit_file", "bash"]
        );
    }

    #[test]
    fn builtin_agents_have_nonempty_descriptions() {
        for agent in builtin_sub_agents() {
            assert!(
                !agent.description.is_empty(),
                "{} has empty description",
                agent.name
            );
        }
    }

    #[test]
    fn builtin_agents_have_nonempty_system_prompts() {
        for agent in builtin_sub_agents() {
            assert!(
                !agent.system_prompt.is_empty(),
                "{} has empty system_prompt",
                agent.name
            );
        }
    }

    // ---------------------------------------------------------------
    // parse_agent_file() tests
    // ---------------------------------------------------------------

    #[test]
    fn parse_valid_agent_file() {
        let content = "\
---
name: reviewer
description: Reviews code for bugs and style issues
model: claude-haiku-4-5-20251001
max_turns: 10
tools:
  - read_file
  - search
  - list_files
---
You are a code reviewer. Analyze the code for:
- Bugs and logic errors
- Style violations";

        let def = parse_agent_file(content).unwrap();
        assert_eq!(def.name, "reviewer");
        assert_eq!(def.description, "Reviews code for bugs and style issues");
        assert_eq!(def.model.as_deref(), Some("claude-haiku-4-5-20251001"));
        assert_eq!(def.max_turns, Some(10));
        assert_eq!(def.tools, vec!["read_file", "search", "list_files"]);
        assert!(def.system_prompt.contains("code reviewer"));
        assert!(def.system_prompt.contains("Bugs and logic errors"));
    }

    #[test]
    fn parse_agent_file_without_model_or_max_turns() {
        let content = "\
---
name: simple
description: A simple agent
---
Do simple things.";

        let def = parse_agent_file(content).unwrap();
        assert_eq!(def.name, "simple");
        assert!(def.model.is_none());
        assert!(def.max_turns.is_none());
    }

    #[test]
    fn parse_agent_file_empty_tools_vec_when_tools_absent() {
        let content = "\
---
name: notool
description: Agent without tools
---
No tools here.";

        let def = parse_agent_file(content).unwrap();
        assert!(def.tools.is_empty());
    }

    #[test]
    fn parse_agent_file_error_missing_name() {
        let content = "\
---
description: Has no name
---
Body text.";

        let err = parse_agent_file(content).unwrap_err();
        assert!(err.contains("name"), "error should mention 'name': {err}");
    }

    #[test]
    fn parse_agent_file_error_empty_name() {
        let content = "\
---
name: \"\"
description: Has empty name
---
Body text.";

        let err = parse_agent_file(content).unwrap_err();
        assert!(err.contains("name"), "error should mention 'name': {err}");
    }

    #[test]
    fn parse_agent_file_error_missing_description() {
        let content = "\
---
name: nodesc
---
Body text.";

        let err = parse_agent_file(content).unwrap_err();
        assert!(
            err.contains("description"),
            "error should mention 'description': {err}"
        );
    }

    #[test]
    fn parse_agent_file_error_empty_description() {
        let content = "\
---
name: nodesc
description: \"  \"
---
Body text.";

        let err = parse_agent_file(content).unwrap_err();
        assert!(
            err.contains("description"),
            "error should mention 'description': {err}"
        );
    }

    #[test]
    fn parse_agent_file_error_missing_opening_delimiter() {
        let content = "name: oops\n---\nBody.";

        let err = parse_agent_file(content).unwrap_err();
        assert!(
            err.contains("delimiter"),
            "error should mention delimiter: {err}"
        );
    }

    #[test]
    fn parse_agent_file_error_missing_closing_delimiter() {
        let content = "---\nname: oops\ndescription: no closing";

        let err = parse_agent_file(content).unwrap_err();
        assert!(
            err.contains("delimiter"),
            "error should mention delimiter: {err}"
        );
    }

    #[test]
    fn parse_agent_file_error_malformed_yaml() {
        let content = "\
---
name: [invalid yaml
  this is broken: {{
---
Body.";

        let err = parse_agent_file(content).unwrap_err();
        assert!(
            err.contains("YAML") || err.contains("yaml"),
            "error should mention YAML: {err}"
        );
    }

    #[test]
    fn parse_agent_file_trims_system_prompt() {
        let content = "\
---
name: trimmer
description: Tests trimming
---

  Some prompt with whitespace.

";

        let def = parse_agent_file(content).unwrap();
        assert_eq!(def.system_prompt, "Some prompt with whitespace.");
    }

    #[test]
    fn parse_agent_file_empty_body() {
        let content = "\
---
name: nobody
description: Agent with no body
---";

        let def = parse_agent_file(content).unwrap();
        assert_eq!(def.system_prompt, "");
    }

    // ---------------------------------------------------------------
    // load_user_sub_agents_from() tests
    // ---------------------------------------------------------------

    #[test]
    fn load_user_sub_agents_returns_empty_when_dir_missing() {
        let dir = tempfile::TempDir::new().unwrap();
        let nonexistent = dir.path().join("does_not_exist");
        let agents = load_user_sub_agents_from(&nonexistent);
        assert!(agents.is_empty());
    }

    #[test]
    fn load_user_sub_agents_returns_valid_def_for_correct_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("reviewer.md");
        std::fs::write(
            &file_path,
            "\
---
name: reviewer
description: Reviews code
model: claude-haiku-4-5-20251001
---
You review code.",
        )
        .unwrap();

        let agents = load_user_sub_agents_from(dir.path());
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name, "reviewer");
        assert_eq!(agents[0].description, "Reviews code");
        assert_eq!(
            agents[0].model.as_deref(),
            Some("claude-haiku-4-5-20251001")
        );
        assert_eq!(agents[0].system_prompt, "You review code.");
    }

    #[test]
    fn load_user_sub_agents_skips_file_missing_name() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("bad.md");
        std::fs::write(
            &file_path,
            "\
---
description: No name here
---
Body.",
        )
        .unwrap();

        let agents = load_user_sub_agents_from(dir.path());
        assert!(agents.is_empty());
    }

    #[test]
    fn load_user_sub_agents_skips_file_missing_description() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("bad.md");
        std::fs::write(
            &file_path,
            "\
---
name: nodesc
---
Body.",
        )
        .unwrap();

        let agents = load_user_sub_agents_from(dir.path());
        assert!(agents.is_empty());
    }

    #[test]
    fn load_user_sub_agents_skips_file_missing_delimiter() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("bad.md");
        std::fs::write(&file_path, "no delimiters here\njust text").unwrap();

        let agents = load_user_sub_agents_from(dir.path());
        assert!(agents.is_empty());
    }

    #[test]
    fn load_user_sub_agents_skips_non_md_files() {
        let dir = tempfile::TempDir::new().unwrap();
        // Valid content but wrong extension
        std::fs::write(
            dir.path().join("agent.txt"),
            "\
---
name: txt_agent
description: Should be skipped
---
Body.",
        )
        .unwrap();

        let agents = load_user_sub_agents_from(dir.path());
        assert!(agents.is_empty());
    }

    #[test]
    fn load_user_sub_agents_returns_multiple_valid_agents() {
        let dir = tempfile::TempDir::new().unwrap();
        for i in 0..3 {
            std::fs::write(
                dir.path().join(format!("agent{i}.md")),
                format!(
                    "\
---
name: agent{i}
description: Agent number {i}
---
Prompt {i}."
                ),
            )
            .unwrap();
        }

        let agents = load_user_sub_agents_from(dir.path());
        assert_eq!(agents.len(), 3);
    }

    // ---------------------------------------------------------------
    // load_model_roster() tests
    // ---------------------------------------------------------------

    #[test]
    fn load_model_roster_returns_three_for_anthropic() {
        let config = AppConfig {
            agent: crate::config::AgentConfig {
                default_provider: "anthropic".into(),
                ..Default::default()
            },
            ..Default::default()
        };

        let roster = load_model_roster(&config);
        assert_eq!(roster.len(), 3);
        assert_eq!(roster[0].id, "claude-haiku-4-5-20251001");
        assert_eq!(roster[1].id, "claude-sonnet-4-6");
        assert_eq!(roster[2].id, "claude-opus-4-6");
    }

    #[test]
    fn load_model_roster_returns_empty_for_ollama() {
        let config = AppConfig {
            agent: crate::config::AgentConfig {
                default_provider: "ollama".into(),
                ..Default::default()
            },
            ..Default::default()
        };

        let roster = load_model_roster(&config);
        assert!(roster.is_empty());
    }

    #[test]
    fn load_model_roster_merges_user_models_with_anthropic() {
        let config = AppConfig {
            agent: crate::config::AgentConfig {
                default_provider: "anthropic".into(),
                ..Default::default()
            },
            models: vec![
                ModelEntry {
                    id: "gpt-4o".into(),
                    provider: "openai".into(),
                    guidance: "General purpose".into(),
                },
                ModelEntry {
                    id: "gpt-4o-mini".into(),
                    provider: "openai".into(),
                    guidance: "Fast and cheap".into(),
                },
            ],
            ..Default::default()
        };

        let roster = load_model_roster(&config);
        assert_eq!(roster.len(), 5); // 3 anthropic + 2 user
        assert_eq!(roster[3].id, "gpt-4o");
        assert_eq!(roster[4].id, "gpt-4o-mini");
    }

    // ---------------------------------------------------------------
    // tools_for_names() tests
    // ---------------------------------------------------------------

    #[test]
    fn tools_for_names_empty_input_returns_empty_vec() {
        let tools = tools_for_names(&[]);
        assert!(tools.is_empty());
    }

    #[test]
    fn tools_for_names_resolves_all_six_known_tools() {
        let names: Vec<String> = vec![
            "read_file".into(),
            "write_file".into(),
            "edit_file".into(),
            "list_files".into(),
            "search".into(),
            "bash".into(),
        ];
        let tools = tools_for_names(&names);
        assert_eq!(tools.len(), 6);
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert_eq!(
            tool_names,
            vec![
                "read_file",
                "write_file",
                "edit_file",
                "list_files",
                "search",
                "bash"
            ]
        );
    }

    #[test]
    fn tools_for_names_skips_unrecognized_names() {
        let names: Vec<String> = vec!["read_file".into(), "fly_rocket".into(), "bash".into()];
        let tools = tools_for_names(&names);
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name(), "read_file");
        assert_eq!(tools[1].name(), "bash");
    }

    #[test]
    fn tools_for_names_all_unrecognized_returns_empty() {
        let names: Vec<String> = vec!["fly_rocket".into(), "time_travel".into()];
        let tools = tools_for_names(&names);
        assert!(tools.is_empty());
    }

    #[test]
    fn tools_for_names_single_tool() {
        let names: Vec<String> = vec!["search".into()];
        let tools = tools_for_names(&names);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name(), "search");
    }

    // ---------------------------------------------------------------
    // build_sub_agent() tests
    // ---------------------------------------------------------------

    fn mock_provider() -> Arc<dyn StreamProvider> {
        Arc::new(yoagent::provider::MockProvider::text("mock"))
    }

    #[test]
    fn build_sub_agent_with_explicit_model_uses_that_model() {
        let def = SubAgentDef {
            name: "explorer".into(),
            description: "Fast file explorer".into(),
            model: Some("claude-haiku-4-5-20251001".into()),
            max_turns: None,
            tools: vec!["read_file".into()],
            system_prompt: "You explore files.".into(),
        };

        let tool = build_sub_agent(&def, mock_provider(), "parent-model", "sk-test");
        assert_eq!(tool.name(), "explorer");
    }

    #[test]
    fn build_sub_agent_with_none_model_uses_parent_model() {
        let def = SubAgentDef {
            name: "fallback-agent".into(),
            description: "Uses parent model".into(),
            model: None,
            max_turns: None,
            tools: vec!["bash".into()],
            system_prompt: "You do things.".into(),
        };

        let tool = build_sub_agent(&def, mock_provider(), "qwen2.5:14b", "");
        assert_eq!(tool.name(), "fallback-agent");
    }

    #[test]
    fn build_sub_agent_with_empty_tools_does_not_panic() {
        let def = SubAgentDef {
            name: "no-tools".into(),
            description: "Agent with no tools".into(),
            model: Some("claude-sonnet-4-6".into()),
            max_turns: None,
            tools: vec![],
            system_prompt: "You have no tools.".into(),
        };

        let tool = build_sub_agent(&def, mock_provider(), "parent-model", "sk-test");
        assert_eq!(tool.name(), "no-tools");
    }

    #[test]
    fn build_sub_agent_with_max_turns_sets_it() {
        let def = SubAgentDef {
            name: "limited".into(),
            description: "Turn-limited agent".into(),
            model: Some("claude-haiku-4-5-20251001".into()),
            max_turns: Some(5),
            tools: vec!["search".into()],
            system_prompt: "Limited turns.".into(),
        };

        let tool = build_sub_agent(&def, mock_provider(), "parent-model", "sk-test");
        assert_eq!(tool.name(), "limited");
    }

    #[test]
    fn build_sub_agent_without_max_turns_uses_default() {
        let def = SubAgentDef {
            name: "default-turns".into(),
            description: "Default turn limit agent".into(),
            model: Some("claude-sonnet-4-6".into()),
            max_turns: None,
            tools: vec!["read_file".into()],
            system_prompt: "Default turns.".into(),
        };

        let tool = build_sub_agent(&def, mock_provider(), "parent-model", "sk-test");
        assert_eq!(tool.name(), "default-turns");
    }

    #[test]
    fn build_sub_agent_resolves_multiple_tools() {
        let def = SubAgentDef {
            name: "multi-tool".into(),
            description: "Agent with multiple tools".into(),
            model: Some("claude-opus-4-6".into()),
            max_turns: Some(15),
            tools: vec![
                "read_file".into(),
                "write_file".into(),
                "edit_file".into(),
                "bash".into(),
            ],
            system_prompt: "You write code.".into(),
        };

        let tool = build_sub_agent(&def, mock_provider(), "parent-model", "sk-test");
        assert_eq!(tool.name(), "multi-tool");
    }

    // ---------------------------------------------------------------
    // coordinator_agent_prompt() tests
    // ---------------------------------------------------------------

    fn sample_agents() -> Vec<SubAgentDef> {
        vec![
            SubAgentDef {
                name: "explorer".into(),
                description: "Fast file explorer".into(),
                model: Some("claude-haiku-4-5-20251001".into()),
                max_turns: None,
                tools: vec!["read_file".into()],
                system_prompt: "You explore files.".into(),
            },
            SubAgentDef {
                name: "coder".into(),
                description: "Writes code".into(),
                model: Some("claude-opus-4-6".into()),
                max_turns: None,
                tools: vec!["edit_file".into(), "bash".into()],
                system_prompt: "You write code.".into(),
            },
        ]
    }

    #[test]
    fn coordinator_prompt_contains_agent_names() {
        let agents = sample_agents();
        let prompt = coordinator_agent_prompt(&agents, &[]);
        assert!(prompt.contains("explorer"), "should contain 'explorer'");
        assert!(prompt.contains("coder"), "should contain 'coder'");
    }

    #[test]
    fn coordinator_prompt_contains_agent_descriptions() {
        let agents = sample_agents();
        let prompt = coordinator_agent_prompt(&agents, &[]);
        assert!(
            prompt.contains("Fast file explorer"),
            "should contain explorer description"
        );
        assert!(
            prompt.contains("Writes code"),
            "should contain coder description"
        );
    }

    #[test]
    fn coordinator_prompt_contains_model_info() {
        let agents = sample_agents();
        let prompt = coordinator_agent_prompt(&agents, &[]);
        assert!(
            prompt.contains("claude-haiku-4-5-20251001"),
            "should contain explorer model"
        );
        assert!(
            prompt.contains("claude-opus-4-6"),
            "should contain coder model"
        );
    }

    #[test]
    fn coordinator_prompt_omits_models_section_when_empty_roster() {
        let agents = sample_agents();
        let prompt = coordinator_agent_prompt(&agents, &[]);
        assert!(
            !prompt.contains("## Available Models"),
            "should not contain Available Models with empty roster"
        );
    }

    #[test]
    fn coordinator_prompt_omits_models_section_when_single_model() {
        let agents = sample_agents();
        let roster = vec![ModelEntry {
            id: "claude-opus-4-6".into(),
            provider: "anthropic".into(),
            guidance: "Best for coding".into(),
        }];
        let prompt = coordinator_agent_prompt(&agents, &roster);
        assert!(
            !prompt.contains("## Available Models"),
            "should not contain Available Models with single model"
        );
    }

    #[test]
    fn coordinator_prompt_includes_models_section_when_multiple() {
        let agents = sample_agents();
        let roster = vec![
            ModelEntry {
                id: "claude-opus-4-6".into(),
                provider: "anthropic".into(),
                guidance: "Best for coding".into(),
            },
            ModelEntry {
                id: "claude-haiku-4-5-20251001".into(),
                provider: "anthropic".into(),
                guidance: "Fast and cheap".into(),
            },
        ];
        let prompt = coordinator_agent_prompt(&agents, &roster);
        assert!(prompt.contains("## Available Models"));
        assert!(prompt.contains("claude-opus-4-6"));
        assert!(prompt.contains("claude-haiku-4-5-20251001"));
        assert!(prompt.contains("Best for coding"));
        assert!(prompt.contains("Fast and cheap"));
    }

    #[test]
    fn coordinator_prompt_empty_agents_and_empty_roster() {
        let prompt = coordinator_agent_prompt(&[], &[]);
        assert!(!prompt.contains("## Available Models"));
    }
}
