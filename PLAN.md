# D4C Implementation Plan

**Created:** 2026-07-26
**Status:** In Progress

---

## Design Decisions (locked)

| Decision | Choice |
|---|---|
| Data storage | `~/.d4c/` global + `.d4c/` per-project |
| Plan rejection | Loop back to questionnaire with rejection reason |
| Tool permissions | Allow-once / allow-always / deny, per-project |
| Router refresh | Per-session catalog fetch, manual via `/config` |

---

## Cargo Workspace Layout

```
d4c/
├── Cargo.toml           # workspace root
├── crates/
│   ├── d4c-core/        # business logic, providers, indexer, config
│   ├── d4c-tui/         # Ratatui rendering, event loop, widgets
│   └── d4c-cli/         # thin binary, wires core + tui together
└── PLAN.md
```

---

## Build Order

### Phase 0 — Foundation

- [x] **Step 1:** Cargo workspace setup
- [x] **Step 2:** Configuration manager (global + per-project, secrets)
- [x] **Step 3:** Repository indexer (file scan, .gitignore, searchable index)
- [x] **Step 4:** Session + crash recovery

### Phase 1 — TUI Shell + OpenCode

- [x] **Step 5:** TUI shell (Ratatui: input, output, status bar)
- [x] **Step 6:** Provider trait + OpenCode implementation
- [x] **Step 7:** Basic chat loop

### Phase 2 — Slash Commands + Router

- [x] **Step 8:** Slash command framework
- [x] **Step 9:** MVP commands (/help, /new, /clear, /history, /config, /login, /review)
- [x] **Step 10:** Model router (Needle/Cactus or heuristic stub + live catalog)

### Phase 3 — Native MCP

- [x] **Step 11:** MCP client (JSON-RPC, server lifecycle, permissions)

### Phase 4 — Interactive Planning UX

- [x] **Step 12:** /plan (repo scan, questionnaire, assumptions, approval)
- [x] **Step 13:** /build (plan execution, checkpoints, staleness detection)
- [x] **Step 14:** Tool permissions system (allow-once/always/deny)

### Phase 5 — Plugins & Self-Upgrade (deferred)

Not planned in detail yet.

---

## Key Architecture Decisions

### Provider Trait

```rust
#[async_trait]
trait Provider: Send + Sync {
    async fn chat(&self, messages: &[Message], tools: &[Tool]) -> Result<ChatResponse>;
    async fn chat_stream(&self, messages: &[Message], tools: &[Tool]) -> Result<Stream>;
    async fn list_models(&self) -> Result<Vec<ModelInfo>>;
    fn name(&self) -> &str;
    fn capabilities(&self) -> ProviderCapabilities;
}
```

### Storage Layout

```
~/.d4c/
├── config.toml          # global config
├── sessions/            # session history + crash recovery
└── keys/                # encrypted secrets fallback

.d4c/                    # per-project
├── config.toml          # project overrides
├── plans/               # saved plans
└── mcp/                 # MCP server configs
```

### Slash Commands

| Command | Description |
|---|---|
| `/plan` | Interactive planning workflow |
| `/build` | Execute approved plan |
| `/new` | New session |
| `/review` | Review diff/plan/past output |
| `/config` | View/edit config |
| `/login` | Authenticate with provider |
| `/history` | Browse past sessions |
| `/clear` | Clear context |
| `/help` | List commands |

### Model Router

- Embed Needle (26M params, Cactus SDK) or start with heuristic stub
- Fetch OpenCode catalog on session start, derive tiers from metadata
- Per task: classify → pick cheapest tier model → show in UI → user can override
- Audit trail recorded in session data
