# PRD 001: Session Management

## Summary

Persist and resume multi-turn conversations across beezle sessions. This is
a core Claude Code feature that lets users pick up where they left off.

## Problem

Currently every beezle invocation starts a blank conversation. Users lose
all context when they exit. There is no way to resume prior work.

## Solution

Add a `session` module that serializes conversation state to disk and
supports resuming by session key or "most recent" shorthand.

## Scope

- `src/session/mod.rs` — `SessionManager` struct
- `src/main.rs` — wire session save/load into the REPL loop

## Requirements

### Must Have

1. **Auto-save on exit**: When the user types `/quit` or Ctrl+C, persist the
   current conversation to `~/.beezle/sessions/<key>.json`.
2. **Session key**: Default key is a timestamp-based ID (e.g. `2026-03-05_14-30`).
   Users can name sessions via `/save <name>`.
3. **Resume by key**: `beezle --resume <key>` loads a prior session's messages
   into the agent before starting the REPL.
4. **Resume most recent**: `beezle --resume` (no key) loads the most recent
   session by file modification time.
5. **List sessions**: `/sessions` command lists saved sessions with timestamps
   and message counts.

### Nice to Have

- Session metadata (model used, token count, duration).
- `/delete <key>` to remove a saved session.

## Acceptance Criteria

- [ ] Exiting beezle saves conversation to `~/.beezle/sessions/<key>.json`
- [ ] `--resume <key>` restores messages and the REPL continues the conversation
- [ ] `--resume` (no key) loads the most recent session
- [ ] `/sessions` prints a list of saved sessions
- [ ] `/save <name>` saves with a user-chosen key
- [ ] Session files are valid JSON that roundtrip through serde
- [ ] Unit tests for save/load/list/most-recent logic

## Dependencies

- None (builds on existing config module for `~/.beezle/sessions/` path)

## Estimated Size

~3 files touched, ~200-300 lines of new code + tests
