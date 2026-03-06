# PRD 007: Permissions System

## Summary

Add tool-level access control so users can approve, deny, or auto-allow
tool invocations. Essential for safety when the agent executes shell commands
or writes files.

## Problem

Currently all tools execute without user consent. The agent can run arbitrary
shell commands, write files, and access the network with no guardrails.
Claude Code asks before destructive operations; beezle should too.

## Solution

A permission manager that intercepts tool calls and applies a policy
hierarchy: config defaults -> per-session overrides from user prompts.

## Scope

- `src/permissions/mod.rs` — `PermissionManager`, `PermissionLevel` enum
- `src/config/mod.rs` — add `PermissionsConfig` to `AppConfig`
- Integration with yoagent's tool execution (via callbacks or wrapper)

## Requirements

### Must Have

1. **PermissionLevel enum**: `Allow`, `AskOnce`, `AskAlways`, `Deny`.
2. **Tool categories**: `Read`, `Write`, `Execute`, `Network`.
3. **Default policy**:
   - Read tools: `Allow`
   - Write tools: `AskOnce`
   - Execute (bash): `AskAlways`
   - Network: `Allow`
4. **Interactive prompt**: When a tool requires permission, display what
   the tool wants to do and ask `[Y]es / [N]o / [A]lways`. "Always"
   upgrades the session policy to `Allow` for that tool.
5. **Config overrides**: `[permissions]` section in config.toml lets users
   set default policies per tool category.

### Nice to Have

- Per-tool policies (e.g. allow `read_file` but ask for `bash`).
- Workspace-level `.beezle/permissions.toml`.

## Acceptance Criteria

- [ ] `bash` tool prompts the user before executing by default
- [ ] Choosing "Always" stops prompting for that tool category
- [ ] Choosing "No" returns a permission denied error to the agent
- [ ] `read_file` executes without prompting by default
- [ ] Config file can override default permission levels
- [ ] Unit tests for permission resolution logic

## Dependencies

- PRD 004 (command bus) for routing permission prompts to the right channel

## Estimated Size

~2-3 files, ~200-300 lines + tests
