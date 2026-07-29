# TODO: /plan and /build feature implementation

- [ ] Step 1 — Agent config / prompt system (new agent.rs module)
- [ ] Step 2 — Plan persistence (save/load plan + todo to .agent/)
- [ ] Step 3 — Wire plan approval to disk
- [ ] Step 4 — Wire /build to load from disk
- [ ] Step 5 — Build execution engine (new build.rs module)
- [ ] Step 6 — Wire build execution in TUI (replace stub execute_build_step)
- [ ] Step 7 — Verification (auto-detect + run lint/test after build)
- [ ] Step 8 — Build reporting (structured summary output)
- [ ] Step 9 — System prompt integration (inject .md contents into model calls)
- [ ] Step 10 — Verify: cargo build + cargo test
