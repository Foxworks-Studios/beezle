# PRD 003: Project Context Loader

## Summary

Automatically detect and inject project context files (CLAUDE.md, BEEZLE.md,
README.md) into the agent's system prompt, giving the agent awareness of the
project it's operating in.

## Problem

The agent has no awareness of the project it's running in. Both Claude Code
and yoyo-evolve read project-level instruction files to guide behavior.

## Solution

On startup, walk up from CWD looking for context files. Inject their contents
into the system prompt before the first user turn.

## Scope

- `src/context/mod.rs` — new module for project context discovery and loading
- `src/main.rs` — integrate context into system prompt construction

## Requirements

### Must Have

1. **File discovery**: Starting from CWD, search for these files (in priority
   order): `CLAUDE.md`, `BEEZLE.md`, `.beezle/instructions.md`.
2. **Content injection**: Prepend discovered file contents to the system prompt,
   wrapped in a clear delimiter (e.g. `<project-context>...</project-context>`).
3. **Size guard**: Truncate context to a configurable max (default 8000 chars)
   to avoid blowing the context window on large READMEs.
4. **Multiple files**: If both `CLAUDE.md` and `BEEZLE.md` exist, include both
   (CLAUDE.md first, as it's the de facto standard).

### Nice to Have

- `.beezleignore` to exclude files from context.
- `/context` slash command to show what was loaded.

## Acceptance Criteria

- [ ] Running beezle in a directory with `CLAUDE.md` injects its content into
      the system prompt
- [ ] Running beezle in a subdirectory finds `CLAUDE.md` in parent directories
- [ ] Content exceeding 8000 chars is truncated with a notice
- [ ] Multiple context files are concatenated in priority order
- [ ] Running beezle with no context files works normally (no error)
- [ ] Unit tests for file discovery, truncation, and prompt assembly

## Dependencies

- None

## Estimated Size

~2 files, ~150-200 lines + tests
