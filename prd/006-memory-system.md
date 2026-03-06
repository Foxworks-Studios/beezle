# PRD 006: Persistent Memory System

**Status:** TICKETS READY

## Summary

Give the agent long-term and daily memory that persists across sessions,
injected into the system prompt so the agent accumulates knowledge over time.

## Problem

Each beezle session starts with zero learned context. The agent can't
remember user preferences, project patterns, or past decisions. This forces
users to re-explain context every session.

## Solution

Two-tier markdown-based memory:
- **Long-term** (`~/.beezle/memory/MEMORY.md`): Stable facts, preferences,
  patterns. Updated by the agent via a tool.
- **Daily notes** (`~/.beezle/memory/YYYY-MM-DD.md`): Timestamped entries
  for the current day. Auto-created, append-only.

Both are injected into the system prompt on startup.

## Scope

- `src/memory/mod.rs` — `MemoryStore` struct
- `src/tools/memory.rs` — `MemoryReadTool`, `MemoryWriteTool`
- `src/main.rs` — inject memory into system prompt

## Requirements

### Must Have

1. **MemoryStore**: Reads/writes memory files from `~/.beezle/memory/`.
2. **memory_read tool**: Agent can read long-term or today's memory.
3. **memory_write tool**: Agent can append to today's notes or update
   long-term memory (full replacement of MEMORY.md content).
4. **Prompt injection**: On startup, MEMORY.md contents are appended to
   the system prompt (truncated to 4000 chars if large).
5. **Date-based daily files**: Today's file is `YYYY-MM-DD.md`, entries
   are timestamped `## HH:MM` headers.

### Nice to Have

- Clock trait for testability (inject fake time in tests).
- `/memory` slash command to show current memory contents.

## Acceptance Criteria

- [ ] Agent can call `memory_write` to persist notes
- [ ] `memory_read` returns memory file contents
- [ ] MEMORY.md content appears in the system prompt
- [ ] Daily notes file is auto-created with today's date
- [ ] Memory persists across sessions (file-based)
- [ ] Unit tests with temp dirs and fake clock

## Dependencies

- None

## Estimated Size

~3 files, ~200-250 lines + tests
