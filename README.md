# Beezle

An AI coding agent CLI built in Rust on the [yoagent](https://github.com/yologdev/yoagent) agent loop crate. Think Claude Code, but open-source and extensible with planned multi-channel input support (Discord, Slack, Telegram).

## Features

**Available now:**

- Interactive REPL with multi-turn conversation and streaming output
- Built-in tools: shell execution, file read/write/edit, directory listing, content search
- TOML-based configuration at `~/.beezle/config.toml`
- Interactive first-run onboarding (provider selection, API key setup, model choice)
- Multi-provider support: Anthropic Claude (cloud) and Ollama (local)
- Skill loading from directories ([AgentSkills](https://github.com/anthropics/agentskills) compatible)
- Single-shot mode (`--prompt`) for scripting and CI use
- Color-gated terminal output (`--no-color` for piped/scripted usage)

**On the roadmap:**

- Session persistence and resume
- Project context injection (auto-read CLAUDE.md / BEEZLE.md)
- Unified command bus for multi-channel input
- Black-box sub-agent architecture with progress callbacks
- Persistent memory system (long-term + daily notes)
- Tool-level permissions (allow / ask-once / ask-always / deny)
- Self-improvement loop (`beezle evolve`)
- TUI with ratatui

## Requirements

- Rust 2024 edition (1.85+)
- An Anthropic API key **or** a running [Ollama](https://ollama.com) instance

## Quick Start

```bash
# Clone and build
git clone https://github.com/Foxworks-Studios/beezle.git
cd beezle
cargo build --release

# Run (first run triggers interactive onboarding)
cargo run
```

On first launch, beezle will walk you through setup:

```
  Welcome to beezle! Let's get you set up.

  Which LLM provider would you like to use?

    [1] Anthropic Claude (cloud, API key)
    [2] Ollama (local, no API key needed)

  Choice [1]:
```

Your configuration is saved to `~/.beezle/config.toml` and reused in subsequent sessions.

## Usage

```
beezle [OPTIONS]

Options:
      --model <MODEL>      Override the model from config (e.g. claude-opus-4-6)
      --resume [<RESUME>]  Resume a previous session (not yet implemented)
      --prompt <PROMPT>    Run a single prompt and exit (non-interactive mode)
      --skills <SKILLS>    Additional skill directories (can be specified multiple times)
      --config <CONFIG>    Path to config file (default: ~/.beezle/config.toml)
      --verbose            Enable verbose (debug-level) logging
      --no-color           Disable colored output
  -h, --help               Print help
  -V, --version            Print version
```

### Interactive mode

```bash
# Start with default config
beezle

# Override model for this session
beezle --model claude-opus-4-6

# Load additional skills
beezle --skills ./my-skills --skills ./team-skills
```

### Single-shot mode

```bash
# Run one prompt and exit (useful for scripts and CI)
beezle --prompt "explain the architecture of this project"

# Pipe-friendly with no ANSI colors
beezle --prompt "list all TODO comments" --no-color
```

### REPL commands

| Command | Description |
|---------|-------------|
| `/quit`, `/exit` | Exit beezle |
| `/clear` | Clear conversation history |
| `/model <name>` | Switch model mid-session |

## Configuration

Configuration lives at `~/.beezle/config.toml`. Created automatically on first run.

```toml
[agent]
name = "beezle"
max_iterations = 20

[providers.anthropic]
api_key_env = "ANTHROPIC_API_KEY"
model = "claude-sonnet-4-20250514"

# Uncomment to use Ollama instead:
# [providers.ollama]
# base_url = "http://localhost:11434"
# model = "qwen2.5:14b"

[shell]
allowed_dirs = ["~/.beezle"]
blocked_commands = []
```

### Provider setup

**Anthropic Claude** (default): Set your API key in the environment:

```bash
export ANTHROPIC_API_KEY=sk-ant-...
```

**Ollama** (local, no API key): Install [Ollama](https://ollama.com), pull a model, and configure beezle to use it:

```bash
ollama pull qwen2.5:14b
```

Then edit `~/.beezle/config.toml` to remove the `[providers.anthropic]` section and add:

```toml
[providers.ollama]
base_url = "http://localhost:11434"
model = "qwen2.5:14b"
```

## Directory Structure

Beezle creates `~/.beezle/` on first run with:

```
~/.beezle/
  config.toml     # Configuration
  sessions/       # Saved conversation sessions (planned)
  memory/         # Persistent agent memory (planned)
  skills/         # User-defined skills (AgentSkills format)
```

## Project Architecture

```
src/
  main.rs         # CLI entry point, REPL loop, arg parsing
  lib.rs          # Crate root, module re-exports
  config/
    mod.rs        # AppConfig, load/save, directory setup
    onboard.rs    # Interactive first-run onboarding flow
```

Built on [yoagent](https://github.com/yologdev/yoagent), which provides:

- Agent loop with streaming LLM responses
- Multi-provider abstraction (Anthropic, OpenAI-compat, Google, Azure, Bedrock)
- Built-in tools (bash, read/write/edit files, search, list)
- Skill loading (AgentSkills standard)
- Context management and compaction
- MCP (Model Context Protocol) server integration

Beezle adds the application layer: configuration, onboarding, CLI interface, and (planned) multi-channel input, permissions, memory, sessions, and TUI.

## Development

```bash
# Run tests
cargo test

# Lint
cargo clippy -- -D warnings

# Format
cargo fmt

# Build
cargo build
```

This project follows **strict red/green TDD** -- every feature starts with a failing test. See [CLAUDE.md](CLAUDE.md) for full development conventions.

## Tech Stack

| Component | Crate |
|-----------|-------|
| Agent loop | [yoagent](https://github.com/yologdev/yoagent) |
| Async runtime | [tokio](https://tokio.rs) |
| CLI parsing | [clap](https://docs.rs/clap) (derive) |
| Serialization | [serde](https://serde.rs) + [toml](https://docs.rs/toml) |
| Error handling | [thiserror](https://docs.rs/thiserror) + [anyhow](https://docs.rs/anyhow) |
| Logging | [tracing](https://docs.rs/tracing) |

## License

Private -- Foxworks Studios.
