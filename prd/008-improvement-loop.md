# PRD 008: Self-Improvement Loop

## Summary

Inspired by yoyo-evolve, add an automated self-improvement pipeline that
lets beezle assess and improve its own codebase on a schedule. Unlike
yoyo-evolve which runs via GitHub Actions, beezle's loop runs as a local
CLI subcommand.

## Problem

Beezle is a coding agent that should be able to improve itself. Having a
structured self-assessment and improvement workflow accelerates development
and serves as a compelling dogfooding mechanism.

## Solution

A `beezle evolve` subcommand that:
1. Reads its own source code and a journal of past improvements.
2. Runs a self-assessment agent to identify the highest-impact improvement.
3. Implements the improvement (with test-gating).
4. Journals the result.

## Scope

- `src/evolve/mod.rs` — evolution pipeline orchestrator
- `skills/self-assess/` — self-assessment skill definition
- `skills/evolve/` — implementation skill definition
- `JOURNAL.md` — append-only improvement log (at repo root)

## Requirements

### Must Have

1. **`beezle evolve` subcommand**: Runs one improvement cycle and exits.
2. **Self-assessment phase**: Agent reads `src/`, `CLAUDE.md`, `JOURNAL.md`,
   and open GitHub issues (if available). Produces a ranked list of
   improvement candidates.
3. **Implementation phase**: Agent picks the top candidate and implements it.
   Uses a sub-agent (PRD 005) so the implementation runs in a clean context.
4. **Test gate**: After implementation, runs `cargo test && cargo clippy`.
   If either fails, the agent gets up to 2 fix attempts. If all fail, revert
   all changes and log the failure.
5. **Journal entry**: Appends a timestamped entry to `JOURNAL.md` documenting
   what was attempted, whether it succeeded, and what was learned.
6. **Safety**: Never force-pushes. Never modifies `JOURNAL.md` entries
   retroactively (append-only). Never removes tests.

### Nice to Have

- `beezle evolve --dry-run` to see what would be attempted without changing
  files.
- `beezle evolve --focus <area>` to constrain improvements to a module.
- Cron-compatible: exit code 0 on success, 1 on failure for scripting.
- GitHub issue integration: read issues labeled `agent-input`, close on fix.

## Acceptance Criteria

- [ ] `beezle evolve` runs a self-assessment and produces improvement candidates
- [ ] The top candidate is implemented via a sub-agent
- [ ] `cargo test && cargo clippy` must pass after changes
- [ ] Failed improvements are reverted and documented in JOURNAL.md
- [ ] Successful improvements are committed with a descriptive message
- [ ] JOURNAL.md is append-only (existing entries never modified)
- [ ] The evolution is idempotent (running twice doesn't duplicate work)

## Dependencies

- PRD 002 (clap) for subcommand parsing
- PRD 005 (sub-agents) for isolated implementation context

## Estimated Size

~3-4 files + 2 skill definitions, ~400-500 lines + tests
