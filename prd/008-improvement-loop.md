# PRD 008: Self-Improvement Loop

**Status:** DRAFT (REVISED)
**Created:** 2026-02-28
**Revised:** 2026-03-07
**Author:** PRD Writer Agent

---

## Problem Statement

Beezle is a coding agent that should be able to improve itself, but today there
is no structured mechanism for it to do so. The agent cannot compare its own
capabilities against known targets (Claude Code, beezle-rs), select the
highest-impact improvement, implement it using its own pipeline, or record what
it tried and learned. Without this loop, human developers must manually drive
all growth instead of letting the agent compound on itself.

## Goals

- Provide a `beezle evolve` subcommand that runs one complete self-improvement
  cycle and exits with code 0 on success, 1 on failure.
- Drive improvement selection from two concrete goal sources: Claude Code
  feature parity and beezle-rs feature parity — not from vague self-inspection
  alone.
- Re-use beezle's existing PRD → tickets → orchestrate pipeline for all
  implementation work, so the loop benefits from every improvement to that
  pipeline.
- Produce a persistent, append-only `JOURNAL.md` at the repo root that
  honestly records every cycle (attempted change, outcome, what was learned).
- Gate every cycle with `cargo build && cargo test && cargo clippy -- -D
  warnings` before starting and again before committing, reverting on failure.

## Non-Goals

- Does not implement a scheduler or cron daemon; running on a schedule is the
  user's responsibility (e.g. `cron`, `systemd` timer).
- Does not implement the PRD ticket-breaker or orchestrate skills — those are
  existing capabilities that this PRD merely calls.
- Does not implement web search or web fetch tools — those are PRD 012. The
  evolve loop depends on PRD 012 being available for the research phase; if
  absent, research falls back to reading local source only.
- Does not push to remote or open GitHub PRs; commits stay local.
- Does not implement multi-cycle loops or background daemons; one invocation =
  one cycle.
- Does not validate or improve the PRD it produces during the same cycle in
  which it writes it; that is a future capability.
- Does not provide a `--dry-run` flag in this iteration.

## User Stories

- As a developer, I want to run `beezle evolve` and have beezle pick,
  implement, test, and commit one focused improvement to itself, so that I can
  grow the codebase without manually driving every step.
- As a developer, I want the evolve loop to consult beezle-rs and Claude Code
  as goal sources, so that improvements target real capability gaps rather than
  arbitrary self-generated ideas.
- As a developer, I want every evolve cycle appended to `JOURNAL.md` with its
  outcome, so I can audit what the agent tried and learned over time.
- As a developer, I want failed improvements to be reverted automatically and
  documented honestly in `JOURNAL.md`, so bad cycles don't leave the codebase
  broken.
- As a developer, I want the loop to refuse to start if the codebase is already
  broken (failing build/tests/clippy), so the evolve command never amplifies an
  existing problem.

## Technical Approach

### Command registration

Add `Evolve` to the `Commands` enum in `src/main.rs` (parsed via `clap`
derive, following the existing pattern from PRD 002):

```rust
/// Run one self-improvement cycle and exit.
Evolve,
```

Dispatch to `evolve::run(config).await` from the `main` match arm.

### New module: `src/evolve/mod.rs`

Owns the cycle orchestration. No direct LLM calls — it constructs prompts and
delegates to agents via the existing agent infrastructure.

Public surface:

```rust
pub async fn run(config: AppConfig) -> anyhow::Result<()>;
```

Internal steps (each is a distinct async function, each independently
testable):

```
1. gate_check()          — cargo build + test + clippy; abort if any fail
2. self_assess()         — produce a ranked list of improvement candidates
3. select_or_resume()    — pick top candidate or resume an unfinished PRD
4. write_prd()           — write PRD file under prd/NNN-feature-name.md
5. break_tickets()       — invoke ticket-breaker skill on the PRD
6. orchestrate()         — invoke orchestrate skill on the ticket file
7. qa_gate()             — cargo test + clippy; revert on failure
8. commit()              — git commit with descriptive message
9. journal()             — append entry to JOURNAL.md
```

### Self-assessment agent

`self_assess()` builds a prompt for beezle's coordinator agent (using
`agent::build_agent()`) with the following inputs injected into context:

| Input | Source |
|-------|--------|
| Own source tree summary | `find src/ -name "*.rs"` output + key file reads |
| `CLAUDE.md` | read from repo root |
| `JOURNAL.md` | read from repo root (last 50 entries if long) |
| Existing PRDs | list of files in `prd/` with their status fields |
| beezle-rs feature list | researcher sub-agent reads `Foxworks-Studios/beezle-rs` via web fetch (PRD 012) or falls back to a bundled snapshot |
| Claude Code feature list | researcher sub-agent fetches Claude Code public docs/changelog via web fetch (PRD 012) or falls back to a bundled snapshot |

The researcher sub-agent (PRD 010) performs the web-facing reads so the
coordinator context stays clean.

The assessment prompt instructs the coordinator to produce a JSON array of
improvement candidates:

```json
[
  {
    "title": "Add Telegram channel adapter",
    "rationale": "beezle-rs has Telegram support; beezle does not. PRD 004 (command bus) is in place.",
    "estimated_size": "medium",
    "priority": 1
  },
  ...
]
```

The coordinator returns this JSON; `self_assess()` deserializes it. One retry
if deserialization fails (standard project pattern).

### Goal sources (bundled snapshots)

To ensure the loop works without web access, two Markdown snapshot files are
bundled in the repository:

- `docs/reference/beezle-rs-features.md` — maintained snapshot of beezle-rs
  capabilities (manually updated or updated by a prior evolve cycle)
- `docs/reference/claude-code-features.md` — maintained snapshot of Claude
  Code features

When web tools (PRD 012) are available, the researcher agent fetches live
content and the snapshot is used only as fallback.

### PRD selection: produce vs. resume

`select_or_resume()` inspects `prd/` for any PRD whose status is `DRAFT` or
`TICKETS READY` with no corresponding `-status.md` file showing `DONE`. If one
exists, it is resumed rather than producing a new PRD, so the loop never
abandons work in progress.

If no unfinished PRD exists, `write_prd()` produces a new one for the
top-ranked candidate using the standard PRD format (same format as this
document).

### Ticket-breaker and orchestrate invocation

These are invoked by constructing a prompt for the coordinator agent with the
appropriate skill instruction, following the pattern established by PRD 010.
The evolve module does not bypass or replicate the pipeline — it calls it
exactly as a human would:

- Ticket-breaker: prompt instructs coordinator to invoke the ticket-breaker
  skill on `prd/NNN-feature-name.md`, producing `prd/NNN-feature-name-tickets.md`.
- Orchestrate: prompt instructs coordinator to invoke the orchestrate skill on
  the tickets file, running the full implementer/reviewer loop.

### QA gate and revert

After orchestration completes, `qa_gate()` runs:

```
cargo test && cargo clippy -- -D warnings
```

On failure:
1. `git diff --name-only HEAD` identifies changed files.
2. `git checkout -- .` reverts unstaged changes.
3. If new files were added (untracked), they are removed.
4. `journal()` records the failure honestly.
5. The process exits with code 1.

On success:
1. `commit()` runs `git add -A && git commit -m "<descriptive message>"`.
   The commit message follows the format: `feat: <title from PRD>`.
2. `journal()` records the success.
3. The process exits with code 0.

### JOURNAL.md format

Entries are appended with a UTC timestamp. Existing entries are never modified.
`JOURNAL.md` is created at repo root if absent.

```markdown
---

## 2026-03-07T14:22:00Z — Add Telegram channel adapter [SUCCESS]

**PRD:** prd/013-telegram-adapter.md
**Outcome:** All tests pass. Committed as `feat: Add Telegram channel adapter` (abc1234).
**Learned:** The command bus pattern from PRD 004 made the adapter straightforward. No edge cases.

---

## 2026-03-07T16:05:00Z — Implement observational memory [FAILURE]

**PRD:** prd/014-observational-memory.md
**Outcome:** QA gate failed — `cargo clippy` reported unused import in `src/memory/observer.rs`. Reverted.
**Learned:** The implementer sub-agent did not run `cargo fmt` before completing. Add this to the coder system prompt.
```

### Safety invariants (enforced in code, not just convention)

- `gate_check()` runs before any agent call. If it fails, the function returns
  `Err` immediately; no changes are made.
- `journal()` only ever appends; it opens `JOURNAL.md` with `OpenOptions::append(true)`.
- `commit()` never uses `--force` or `--amend`.
- `qa_gate()` calls `git stash` before running checks if there are staged
  changes, then `git stash pop` after, to avoid false pass/fail from staged
  work-in-progress. (Simpler: orchestrate completes all writes before
  `qa_gate()` is called — no mid-cycle staged state.)

### Files created or modified

| File | Change |
|------|--------|
| `src/evolve/mod.rs` | New module — full cycle orchestration |
| `src/main.rs` | Add `Evolve` to `Commands` enum; dispatch to `evolve::run()` |
| `docs/reference/beezle-rs-features.md` | New — bundled feature snapshot |
| `docs/reference/claude-code-features.md` | New — bundled feature snapshot |
| `JOURNAL.md` | Created at repo root by first run if absent |
| `Cargo.toml` | No new deps expected (reuses existing agent/config infrastructure) |

## Acceptance Criteria

1. `beezle evolve` is a recognized subcommand: `beezle evolve --help` prints a
   usage line without error.
2. When `cargo build` fails before `beezle evolve` is run, the gate check
   detects this and exits with code 1 and a message containing "gate check
   failed" before any agent calls are made.
3. When no unfinished PRD exists in `prd/`, a new PRD file matching
   `prd/NNN-feature-name.md` is written before tickets are broken.
4. When an unfinished PRD exists (status `DRAFT` or `TICKETS READY`, no
   completed status file), `beezle evolve` resumes it rather than writing a
   new PRD; no duplicate PRD file is created.
5. After a successful cycle, `JOURNAL.md` contains a new entry with a UTC
   timestamp, the word `SUCCESS`, the PRD filename, and a non-empty
   `Learned:` field.
6. After a failed QA gate, `JOURNAL.md` contains a new entry with a UTC
   timestamp, the word `FAILURE`, and a non-empty `Learned:` field; no commit
   is made (verified by `git log --oneline -1` returning the same hash as
   before the run).
7. After a failed QA gate, `git status` shows a clean working tree (all
   changes reverted).
8. After a successful cycle, `git log --oneline -1` shows a new commit whose
   message starts with `feat:`.
9. Existing entries in `JOURNAL.md` are byte-for-byte identical after any run
   (append-only verified by comparing the first N bytes before and after).
10. The self-assessment phase produces a candidate list that references at least
    one item from either the beezle-rs snapshot or the Claude Code snapshot
    (verified in unit tests by asserting the assessment prompt includes both
    source documents).
11. Unit tests for `gate_check()` cover: all pass (returns `Ok`), build fails
    (returns `Err` with message), test fails (returns `Err`), clippy fails
    (returns `Err`).
12. Unit tests for `select_or_resume()` cover: no PRDs present (returns
    `NeedNew`), unfinished DRAFT PRD present (returns `Resume(path)`), all
    PRDs have completed status (returns `NeedNew`).
13. Unit tests for `journal()` cover: file absent (creates file and appends),
    file present (appends without modifying prior content).
14. `cargo clippy -- -D warnings` passes with the new `src/evolve/mod.rs`
    included.
15. All public items in `src/evolve/mod.rs` have doc comments.

## Open Questions

- When the orchestrate skill produces a partial implementation that breaks the
  build, should the revert be at the git level (`git checkout -- .`) or should
  the orchestrator have an undo mechanism? For now: git-level revert. Revisit
  if orchestrate starts using git commits mid-cycle.
- Should `beezle evolve` stream agent output to stdout in real time (like the
  normal REPL) or suppress output and only print a summary? Default to
  streaming so the user can observe progress; suppress with `--quiet` in a
  follow-up.

## Dependencies

- **PRD 002 (clap CLI args)** — done. Provides the `Commands` enum and
  dispatch pattern.
- **PRD 010 (multi-agent system)** — done. Provides `researcher`, `coder`, and
  `explorer` sub-agents used by the self-assessment and implementation phases.
- **PRD 012 (web tools)** — new dependency for live beezle-rs and Claude Code
  research. The loop degrades gracefully to bundled snapshots if PRD 012 is not
  yet implemented.
- Existing ticket-breaker and orchestrate skills must be available in
  `~/.beezle/skills/` or bundled as built-in skills. If not yet implemented,
  this PRD is blocked on their availability.
