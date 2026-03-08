# PRD 012: Web Tools (web_search and web_fetch)

**Status:** DRAFT
**Created:** 2026-03-07
**Author:** PRD Writer Agent

---

## Problem Statement

The agent has no ability to access the internet. Research tasks that require
current documentation, third-party API references, or web content force the
user to manually fetch and paste content. The `researcher` sub-agent is
especially hobbled — it can explore local files but cannot follow an external
link or run a web search. The permissions system (PRD 007) already defines the
`Network` tool category and `WebFetch`/`WebSearch` pattern syntax; the tools
themselves just haven't been built yet.

## Goals

- Implement `web_search` as a `yoagent::AgentTool` in `src/tools/web_search.rs`
  that accepts a query string and returns ranked results (title, URL, snippet)
  from the Tavily search API.
- Implement `web_fetch` as a `yoagent::AgentTool` in `src/tools/web_fetch.rs`
  that fetches a URL, converts HTML to readable plain text, and returns the
  content truncated to a configurable character limit.
- Wire both tools into `tools_for_names()` in `src/agent/sub_agents.rs` so they
  can be assigned to any sub-agent definition by name.
- Add both tools to the `researcher` built-in sub-agent's tool list.
- Integrate both tools with the existing permissions system (PRD 007): both
  default to the `Network` `Ask` category and support `WebFetch(domain:X)` /
  `WebSearch(*)` pattern rules.

## Non-Goals

- Does not add web tools to the `explorer` or `coder` sub-agents (only
  `researcher` gets them by default).
- Does not implement a custom HTML-to-Markdown conversion pipeline. A
  well-maintained crate (`htmd` or `html2text`) is sufficient.
- Does not implement caching of fetched content across sessions.
- Does not implement rate-limiting or robots.txt compliance.
- Does not support authentication (HTTP basic auth, bearer tokens, cookies).
- Does not support non-HTTP/HTTPS protocols (ftp, file://, etc.).
- Does not add a settings-file UI for configuring the Tavily API key; the key
  is read from the `TAVILY_API_KEY` environment variable.
- Does not replace the `agent-browser` skill for complex browser interactions.
  These tools handle simple HTTP fetch/search; `agent-browser` remains the
  right choice for JS-rendered pages, form filling, authenticated sessions,
  and any task requiring a real browser. The coordinator prompt should guide
  the LLM on when to use which.
- Does not add a feature flag gating the tools behind a Cargo feature. Network
  access in an agent CLI is expected; both tools are compiled unconditionally
  and simply return a clear error string when the API key is absent or a network
  error occurs.

## User Stories

- As an agent user, I want the `researcher` sub-agent to search the web for
  third-party documentation so I don't have to copy and paste content manually.
- As an agent user, I want the agent to fetch a specific URL and extract its
  readable content so I can ask questions about pages the agent finds in search
  results.
- As a security-conscious user, I want `web_fetch` and `web_search` to use the
  `Network` permission category (defaulting to `Ask`) so the agent can't make
  outbound network calls without my knowledge.
- As a developer, I want to pre-approve specific domains via
  `WebFetch(domain:docs.rs)` in my settings file so the agent can fetch Rust
  documentation without prompting.

## Technical Approach

### New files

**`src/tools/web_search.rs`** -- `WebSearchTool` struct implementing
`yoagent::AgentTool`. The struct holds a `reqwest::Client` and a Tavily API
key string read from `TAVILY_API_KEY` at construction time. It is constructed
stateless (no `Arc<...>` dependency) following the pattern of yoagent's built-in
tools like `SearchTool`.

Parameters schema:
```json
{
  "type": "object",
  "required": ["query"],
  "properties": {
    "query": { "type": "string" },
    "max_results": { "type": "integer", "default": 5 }
  }
}
```

The tool POSTs to `https://api.tavily.com/search` and formats the response as
a numbered Markdown list: `### N. Title\nURL: ...\n\n<snippet>\n`. If the API
key is absent or the response is non-2xx, it returns a `ToolError::Failed`
with a human-readable message. This matches the approach in `beezle-rs`
`src/tools/web_search.rs`, adapted to the `AgentTool` trait API
(`name()`, `label()`, `description()`, `parameters_schema()`, `execute()`).

**`src/tools/web_fetch.rs`** -- `WebFetchTool` struct implementing
`yoagent::AgentTool`. Holds a `reqwest::Client`. The `beezle-rs` implementation
uses a multi-file pipeline (`fetch.rs`, `clean.rs`, `score.rs`, etc.) with
sophisticated content extraction. For this port, implement a simpler single-file
approach: fetch the URL, check the `Content-Type` header, pass HTML through the
`htmd` crate (or `html2text` if `htmd` isn't available) to produce readable
text, then truncate to 20,000 characters with a trailing `...[truncated]` note
when exceeded.

Parameters schema:
```json
{
  "type": "object",
  "required": ["url"],
  "properties": {
    "url": { "type": "string" },
    "max_chars": { "type": "integer", "default": 20000 }
  }
}
```

Non-HTML responses (plain text, JSON) are returned as-is up to `max_chars`.
Binary responses (images, PDFs) return `ToolError::Failed("unsupported content
type: ...")`. Redirect following uses `reqwest`'s default (up to 10 redirects).
Connection and read timeouts are set to 30 seconds.

### Modified files

**`src/tools/mod.rs`** -- add `pub mod web_search;` and `pub mod web_fetch;`.

**`src/agent/sub_agents.rs`** -- two changes:
1. `tools_for_names()`: add arms for `"web_search"` and `"web_fetch"` that
   construct `Arc::new(WebSearchTool::new())` and `Arc::new(WebFetchTool::new())`
   respectively. These constructors read `TAVILY_API_KEY` from the environment
   at call time; if the variable is absent the tool is still registered but
   returns `ToolError::Failed` on first use.
2. `builtin_sub_agents()`: add `"web_search"` and `"web_fetch"` to the
   `researcher` agent's `tools` vec, and update its `system_prompt` to mention
   the availability of web search and fetch.

**`src/agent/sub_agents.rs`** -- `coordinator_agent_prompt()`: append a
"Web Tools vs Browser" guidance section advising the LLM:
- Use `web_search`/`web_fetch` (via the researcher) for quick lookups, docs,
  and simple page content.
- Use the `agent-browser` skill (if installed) for JS-rendered pages, form
  interactions, authenticated sessions, screenshots, and complex browser tasks.

**`Cargo.toml`** -- add dependencies:
- `reqwest` with `json` and `rustls-tls` features (check if already present;
  add only if absent).
- `htmd` (HTML-to-Markdown) or `html2text` for HTML stripping.

### Permissions integration

No code changes needed in `src/permissions/`. The existing `mod.rs` already:
- Classifies `"web_fetch"` and `"web_search"` under `ToolCategory::Network`
  (defaults to `Ask`).
- Maps `"web_fetch"` to the `WebFetch` rule prefix and extracts the `"url"`
  field for pattern matching against `WebFetch(domain:X)`.
- Maps `"web_search"` to the `WebSearch` rule prefix.

Both tools will automatically be wrapped in `PermissionGuard` at startup via
the existing tool registration loop in `main.rs`.

### Test strategy (TDD)

Following the mandatory red/green TDD workflow, tests for each tool are written
before implementation. Tests that make real network calls are not acceptable;
use `wiremock` (already used in beezle-rs) or a trait abstraction to inject a
mock HTTP client. The `ToolContext` helper pattern from `src/tools/memory.rs`
tests applies directly.

For `WebFetchTool`, the HTML-to-text conversion can be tested with inline HTML
strings without any mock server.

## Acceptance Criteria

1. `cargo build` produces zero warnings and zero errors after all changes.
2. `cargo clippy -- -D warnings` passes with no new violations.
3. `WebSearchTool::name()` returns `"web_search"` and
   `WebFetchTool::name()` returns `"web_fetch"`.
4. `tools_for_names(&["web_search".into()])` returns a vec of length 1 with a
   tool whose `name()` is `"web_search"`; same for `"web_fetch"`.
5. The `researcher` built-in sub-agent returned by `builtin_sub_agents()`
   includes both `"web_search"` and `"web_fetch"` in its `tools` vec.
6. `WebSearchTool::execute()` called with `{"query": "rust async"}` against a
   mock server that returns a two-result Tavily payload produces a string
   containing `### 1.` and `### 2.` with the mocked titles and URLs.
7. `WebSearchTool::execute()` called with a missing `"query"` field returns
   `Err(ToolError::InvalidArgs(_))`.
8. `WebSearchTool::execute()` when the mock server returns HTTP 401 returns
   `Err(ToolError::Failed(_))` with a message containing `"401"`.
9. `WebFetchTool::execute()` called with `{"url": "..."}` against a mock server
   returning `<html><body><h1>Hello</h1><p>World</p></body></html>` produces
   a result whose text content contains "Hello" and "World" with HTML tags
   stripped.
10. `WebFetchTool::execute()` with a response body of 25,000 characters returns
    a result whose text length is at most 20,003 characters (20,000 content +
    `...[truncated]`) and contains the literal string `[truncated]`.
11. `WebFetchTool::execute()` with a missing `"url"` field returns
    `Err(ToolError::InvalidArgs(_))`.
12. `WebFetchTool::execute()` when the mock server returns `Content-Type:
    application/pdf` returns `Err(ToolError::Failed(_))` with a message
    containing `"unsupported content type"`.
13. The coordinator agent prompt (output of `coordinator_agent_prompt()`)
    contains guidance distinguishing simple web tools from the `agent-browser`
    skill, mentioning when to use each.
14. `PermissionPolicy::categorize("web_search")` returns `ToolCategory::Network`
    (verified by existing test coverage in `src/permissions/mod.rs` — confirm
    the test already covers this; no new test needed if it does).

## Open Questions

- **`reqwest` already in `Cargo.toml`?** The `main.rs` REPL and existing tools
  may already pull in `reqwest` transitively via `yoagent`. If not, it must be
  added directly. Implementer should check before adding a duplicate dep.
- **`htmd` vs `html2text` vs `scraper`?** `htmd` produces Markdown which is
  more LLM-friendly; `html2text` produces plainer output. Either is fine.
  Prefer whichever has fewer transitive dependencies and actively maintained
  crates.io presence at implementation time.
- **`wiremock` dependency**: beezle-rs uses it for HTTP mocking. If it is not
  already in `[dev-dependencies]`, it should be added. Alternatively, the tool
  can accept an injected base URL in tests (the pattern beezle-rs uses for
  `MockableWebSearchTool`).

## Dependencies

- **PRD 007 (Permissions System)** -- implemented. `ToolCategory::Network`,
  `WebFetch` and `WebSearch` pattern rule prefixes, and `PermissionGuard`
  wrapping are already in place in `src/permissions/mod.rs`.
- **PRD 010 (Multi-Agent System)** -- implemented. `tools_for_names()` and
  `builtin_sub_agents()` in `src/agent/sub_agents.rs` are the extension points
  for registering the new tools and assigning them to `researcher`.
- **Tavily API** -- external dependency. Requires a `TAVILY_API_KEY` environment
  variable. Free tier is available at tavily.com. No account is needed to build
  or run tests (tests use mock servers).
- **`reqwest`** -- HTTP client. May already be present transitively; verify
  before adding.
- **`htmd` or `html2text`** -- HTML stripping crate. New direct dependency.
