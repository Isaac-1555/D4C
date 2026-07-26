# D4C — Terminal Coding Agent
## Product Requirements Document

**Status:** Draft v0.3
**Owner:** _TBD_
**Last updated:** 2026-07-26

---

## 1. Vision

D4C is a lightweight, modular terminal coding agent built from scratch in Rust with a native TUI. It is inspired by the usability of tools like Claude Code, OpenCode, and similar agentic coding tools, but shares no source code with them. It integrates model providers through APIs and emphasizes **guided workflows over open-ended chat** — the planning-first interaction model is the core differentiator.

## 2. Problem Statement

Most terminal coding agents optimize for freeform chat: the user types a request, the model immediately starts editing files, and the user discovers scope or misunderstanding only after changes are made. This creates:

- Wasted iterations when the model misunderstands intent
- Low trust in autonomous edits on real codebases
- No structured record of *why* a change was made

D4C addresses this by making planning a first-class, interactive step — repository context is gathered, ambiguities are resolved through a structured questionnaire, assumptions are surfaced for review, and only an **approved plan** is executed, with checkpoints along the way.

## 3. Target Users

- Individual developers who want an auditable, low-surprise agent for working in existing codebases
- Teams who want a provider-agnostic tool (not locked into one vendor's CLI)
- Users of self-hosted or local models who need first-class support, not an afterthought

## 4. Goals

- Fast startup: **&lt;100ms to first interactive UI frame**
- Built in Rust, rendered with Ratatui
- Provider-agnostic model integration
- Native MCP (Model Context Protocol) client
- Interactive, structured planning UI (not just chat)
- Slash-command architecture as the primary interaction surface
- Automatic, transparent model selection based on task complexity, via a small embedded router — not a separate heavy service or external dependency

### 4.1 Non-Goals (for initial releases)

- Not a full IDE — no GUI, no code-intelligence features beyond what's needed for planning/editing (e.g., LSP-grade refactoring is out of scope initially)
- Not a hosted/multi-user product in v1 — single local user, single machine
- Not attempting feature parity with every competitor on day one; MCP and plugins are explicitly later phases
- Not multi-provider on day one — MVP integrates OpenCode only; OpenAI, Anthropic, OpenRouter, and local-model support are post-MVP (see §9, §15)

## 5. Success Metrics

_(placeholder — needs product input)_

- Cold start time (p50 / p95) under target on reference hardware
- % of `/plan` sessions where the user approves without editing assumptions
- % of `/build` runs completed without manual intervention after approval
- Crash-free session rate
- Time-to-first-token from each supported provider
- % of routed tasks where the user manually overrides the router's model choice (proxy for router accuracy)

## 6. Core Commands

| Command | Description |
|---|---|
| `/plan` | Index repository, analyze, generate interactive questionnaire, present assumptions for review, output an implementation plan |
| `/build` | Execute an approved plan with checkpoints (pause points for review/abort) |
| `/new` | Start a new conversation/session |
| `/review` | Review a diff, plan, or past session output |
| `/config` | View/edit configuration (providers, keys, MCP servers, preferences) |
| `/login` | Authenticate with a provider |
| `/history` | Browse past sessions/conversations |
| `/clear` | Clear current conversation context |
| `/help` | List commands and usage |

**Open question:** should `/plan` and `/build` be separable per-command (e.g. `/build --resume` for interrupted runs), or always run as a pair in one session?

## 7. Interactive Planning UX

1. **Repository scan** — indexes files with a visible progress indicator; respects `.gitignore` and a configurable ignore list
2. **Structured questionnaire** — the model returns a structured question schema (not free text), which the UI renders as:
   - Free text input
   - Single-select
   - Multi-select
   - Yes/No
   - File picker
3. **Assumptions review** — the model's inferred assumptions are shown as an editable list before planning proceeds; the user can accept, edit, or reject each one
4. **Final plan approval** — the generated implementation plan (steps, files touched, estimated scope) is shown for explicit approval before `/build` can run

**Gap to resolve:** what happens if the user rejects the plan outright — does it loop back to the questionnaire, or restart `/plan` from scratch? Recommend: loop back with the rejection reason fed to the model as additional context.

## 8. Architecture

- **Rust core** — application logic, state management
- **Ratatui renderer** — terminal UI rendering
- **Provider abstraction trait** — a common interface all model providers implement (streaming, tool calls, token accounting)
- **Model router** — a small embedded classifier (proposed: Needle on the Cactus inference engine, via its Rust SDK) that scores each task's complexity and selects which model/tier to invoke from the active provider's current catalog (MVP: OpenCode only — see §9, §10)
- **Conversation engine** — manages session state, history, and context window assembly
- **Tool execution layer** — sandboxed execution of file edits, shell commands, and other agent actions
- **MCP manager** — discovers, installs, and manages MCP server connections
- **Repository indexer** — builds a searchable representation of the codebase for planning context
- **Configuration manager** — reads/writes user and per-project config

### 8.1 Additions worth specifying before build starts

- **Tool execution & permissions:** what actions require explicit user confirmation (file writes, shell commands, network access)? Recommend a permission model similar to MCP's own (allow-once / allow-always / deny), scoped per-project.
- **Data storage:** where do conversations, plans, and history live on disk (e.g. `~/.d4c/` or per-project `.d4c/`)? Needs a decision before `/history` and crash recovery can be built.
- **Context window management:** strategy for what gets included from the repo index vs. truncated as conversations grow.

## 9. Provider Layer

**MVP scope: OpenCode only.** The remaining providers below are the target end-state, not the initial release — see §15 for phasing.

- OpenCode-compatible API — **MVP**
- OpenAI — post-MVP
- Anthropic — post-MVP
- OpenRouter — post-MVP
- Local models (e.g. via an OpenAI-compatible local server) — post-MVP

Each provider implements the shared abstraction trait, so features like tool-calling and streaming must be normalized across providers with differing capabilities. Local models in particular may lack full tool-calling support — the abstraction should degrade gracefully rather than assume feature parity. Building the trait against a single provider first (OpenCode) risks under-designing for that normalization — worth sanity-checking the trait's shape against at least one other provider's API shape before locking it in, even if that provider isn't implemented until later.

## 10. Automatic Model Routing

A small, embedded local model classifies each task by complexity and automatically selects which model to invoke — from whichever provider(s) the user has configured — so the user isn't manually switching models between a quick file read and a hard multi-file refactor.

**Proposed implementation:** [Needle](https://github.com/cactus-compute/needle), a 26M-parameter model from Cactus Compute (distilled from Gemini, ~14MB on disk, purpose-built for function-calling/routing-style decisions rather than open-ended generation), served through the [Cactus](https://cactuscompute.com/) on-device inference engine. Cactus ships an official Rust SDK alongside its other platform SDKs, so it embeds directly into D4C's Rust core rather than requiring a separate runtime or Python sidecar. Cactus's own product already includes a "Hybrid Router" concept — run simple requests on-device, hand off complex ones to the cloud — which is directionally the same idea D4C needs, though it's a reference rather than a drop-in.

**MVP scope: OpenCode only** (see §9). The router's first version only needs to reason over a single provider's catalog — no cross-provider merge/normalization logic required yet. That generalization is deferred alongside the other providers.

### 10.1 Router requirements

- Runs fully local and offline; the routing decision itself requires no network call to Needle/Cactus.
- Extremely small footprint, fast inference — Needle reportedly runs at 1,200+ tok/s decode on Cactus, well within a single-digit-millisecond routing decision — so it doesn't threaten the &lt;100ms startup budget (Goal 4) or the "runs on a potato" requirement.
- Embeds via Cactus's Rust SDK rather than a separate service the user has to install, run, or configure.
- Scope is strictly classification/routing — it never generates conversational or coding output itself.

### 10.2 Routing behavior

- Scores each incoming task and maps it to a tier, then picks the cheapest/fastest model in that tier from OpenCode's currently available models.
- **No maintained tier-mapping table.** Instead of hand-curating which OpenCode models count as "cheap" vs. "capable," the router pulls OpenCode's live model catalog fresh on load and derives each model's tier from whatever metadata the catalog exposes at that moment (declared cost, context window, reasoning/tool-call flags). This means tier assignment stays correct as OpenCode adds, removes, renames, or re-prices models, without a D4C release. **Open question:** does "on load" mean once per app/session start, or refreshed per task? Refetching per task keeps the list maximally fresh but adds a network round-trip to every routing decision, which is in tension with the local/fast requirement — recommend defaulting to once per session start, with a manual refresh available via `/config`, unless there's a concrete reason tasks need per-call freshness.
- Illustrative tiers (mechanism, not a fixed list):
  - **Simple** (single-file reads, short factual questions, small isolated edits) → the cheapest/fastest model the live catalog currently reports.
  - **Complex** (multi-file refactors, architecture-level design, ambiguous specs needing extended reasoning) → the most capable model the live catalog currently reports.
- The choice is visible and overridable: the UI shows which model was picked and why, consistent with D4C's low-surprise, auditable positioning (see Problem Statement) — the user can override per-turn or pin a model for the whole session/project.
- `/plan` and `/build` may warrant different default routing — e.g. always route `/plan`'s final synthesis to the top tier since it's the highest-stakes step, while individual `/build` checkpoints route dynamically per sub-task. **Open question**, needs a decision.

### 10.3 Gaps to resolve

- **Needle is trained for function/tool-call selection, not literally "how complex is this coding task."** The architecture and size are right, but task-complexity classification is a different objective than its published benchmarks cover. Needle supports local fine-tuning (`needle finetune` on custom JSONL), so the plan should assume D4C needs its own labeled dataset of (task → tier) examples rather than using Needle's off-the-shelf weights as-is. Needs an eval pass before relying on it.
- **Licensing:** Cactus is currently free for hobbyists, students, non-profits, and small businesses per its published terms; if D4C is or becomes a commercial product, licensing terms need to be confirmed before depending on it — separate from the technical fit.
- **Live-fetching the catalog solves staleness of *what exists*, not necessarily correct tiering.** It only works if OpenCode's actual API (not just third-party mirrors of it) exposes enough signal to tell cost apart from capability — e.g. price alone isn't a reliable proxy. OpenCode's own "Big Pickle" is documented as a reasoning model for deliberate, multi-step problem solving, yet it's currently free during a promo — a router that tiers purely by price would misfile it as "simple" when it's actually a complex-task model. This needs confirming against OpenCode's real API response shape (does it expose a reasoning/capability flag, not just a price?) before the design is locked in.
- **Signals beyond the raw prompt:** should the classifier also weigh repo size, number of files likely touched, whether the `/plan` questionnaire flagged ambiguity, or which of OpenCode's models currently support tool-calling?
- **Fallback behavior:** if the selected tier's model is rate-limited or OpenCode is down, does the router cascade to the next tier automatically, or prompt the user?
- **Auditability:** should routing decisions be recorded as part of the session/plan history so the user can see, after the fact, which model handled which step (depends on the storage decision in §8.1)?
- **Accuracy risk:** misrouting a complex task to a lightweight model risks silent quality loss, which cuts directly against the trust goal in the Problem Statement; needs a labeled eval set before shipping, and likely its own success metric (§5).


## 11. MCP (Model Context Protocol)

- Native MCP client
- Server discovery and installation
- Permission prompts before a server can act
- Per-project configuration (which servers are enabled, with what permissions, in this repo)

**Gap to resolve:** MCP server credentials/secrets storage — needs to align with the configuration manager's approach to secrets in general (see below).

## 12. Configuration & Secrets

_(new section — not in original draft, but required for `/config`, `/login`, and MCP to function)_

- Provider API keys and MCP credentials must be stored securely (OS keychain where available, encrypted file fallback)
- Config should be layered: global user config → per-project overrides
- `/config` should support both an interactive TUI editor and direct file editing (with schema validation on load)

## 13. Non-Functional Requirements

- Cross-platform (Linux, macOS, Windows)
- Memory efficient
- Crash recovery — sessions and in-progress plans should be resumable after a crash
- Structured logging (for debugging and future telemetry, opt-in)
- Extensible modules (clear internal plugin boundaries even before Phase 5 ships the plugin API)

## 14. Risks & Open Questions

- **Provider capability drift:** tool-calling schemas differ across providers and change over time; the abstraction layer needs a versioning/compatibility strategy.
- **MCP security:** installing third-party MCP servers is inherently a supply-chain risk; permission prompts need to be meaningful, not rubber-stamped.
- **Startup time budget:** &lt;100ms is aggressive if the repository indexer, MCP manager, or model router do any blocking I/O or slow model loads at launch — needs a lazy-load strategy.
- **Plan staleness:** if `/build` runs over a long session, the repo may change underneath it (e.g. via other tools) — needs a staleness check before applying edits.
- **Router misclassification:** if the model router under- or over-estimates task complexity, results either silently degrade (weak model on a hard task) or cost/latency rise needlessly (top-tier model on a trivial task); needs a labeled eval set before shipping, plus a fast, obvious manual-override path so misrouting is cheap to correct.

## 15. Roadmap

| Phase | Scope |
|---|---|
| 1 | TUI shell + OpenCode integration (MVP provider — see §9) + basic chat |
| 2 | Slash command architecture |
| 3 | Native MCP support |
| 4 | Interactive planning UX (`/plan` questionnaire, assumptions, approval) |
| 5 | Plugins & self-upgrade |
| — | OpenAI, Anthropic, OpenRouter, and local-model providers — post-MVP, unscheduled (see §9) |

**Suggested addition:** an explicit "Phase 0" — repository indexer + config manager + crash recovery skeleton — since several later phases (planning, MCP, history) depend on these existing first, even though they weren't called out as their own phase.

**Suggested addition:** automatic model routing (§10) depends on the OpenCode integration (Phase 1); since MVP is single-provider, the router's first version doesn't need cross-provider merge logic — recommend landing it in Phase 2 alongside the slash-command architecture, since routing decisions need a visible surface (e.g. `/config` and per-turn UI) as soon as commands exist. Revisit the router's design once additional providers are scheduled, since a live-catalog-per-provider approach (§10.2) needs a merge strategy across providers that single-provider MVP doesn't require.

---

### Summary of changes from v0.1
- Added problem statement, target users, and a non-goals section to frame scope
- Flagged undefined success metrics as a placeholder needing product input
- Called out several open questions (plan rejection flow, tool permissions, secrets storage, staleness checks) that block clean implementation of features already listed
- Added a configuration/secrets section required by `/config`, `/login`, and MCP
- Suggested a "Phase 0" for indexer/config/crash-recovery groundwork

### Summary of changes from v0.2 → v0.3
- Added a new §10, Automatic Model Routing: a small embedded classifier that auto-selects which model/tier to use per task
- Proposed a concrete implementation: Needle (Cactus Compute's 26M-param routing/function-call model) served via the Cactus on-device inference engine's Rust SDK, chosen because it embeds directly into D4C's Rust core and is purpose-built for lightweight decisions rather than generation
- Added "model router" as an architecture component (§8) and as a top-level goal (§4)
- Added a router-accuracy proxy metric to Success Metrics (§5)
- Flagged that Needle's off-the-shelf training target (function-calling) differs from D4C's needed objective (task-complexity classification), and that Cactus's free-tier licensing needs confirming before commercial use
- Added a router-misclassification risk (§14) and a roadmap placement suggestion (§15)
- **Scoped MVP to OpenCode only** — added this to Non-Goals (§4.1) and Provider Layer (§9); OpenAI, Anthropic, OpenRouter, and local-model support are now explicitly post-MVP/unscheduled in the roadmap (§15)
- **Replaced the "maintained tier-mapping table" idea with a live-fetch design:** the router now pulls OpenCode's current model catalog fresh on every load and derives tiers from that catalog's metadata, rather than a hand-curated, staleness-prone table (§10.2). Flagged that this solves catalog staleness but not necessarily tier-*accuracy* — it depends on OpenCode's real API exposing capability signals, not just price (§10.3), using Big Pickle's free-but-complex profile as the illustrating case
- Simplified the router's MVP scope to a single provider (no cross-provider merge logic needed yet); noted the design should be revisited once other providers are scheduled
