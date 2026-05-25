# D4C — Personal Coding Agent

D4C is a personal coding agent I built by cloning the [pi coding agent](https://github.com/earendil-works/pi-mono) and making core changes that make it mine and personalized for my coding preferences and workflow.

## Quick Start

```bash
npm install -g --ignore-scripts @earendil-works/pi-coding-agent
export ANTHROPIC_API_KEY=sk-ant-...
d4c
```

The model has access to `read`, `write`, `edit`, and `bash` tools by default.

## Features

- **30+ LLM providers** — Anthropic, OpenAI, Google, Bedrock, Groq, DeepSeek, Mistral, OpenRouter, xAI, Together AI, GitHub Copilot, and more. Use API keys or OAuth subscriptions. Switch models mid-session.
- **Interactive TUI** — Custom terminal UI with markdown rendering, inline images, fuzzy file search, multi-line editor, full keyboard navigation.
- **Session branching** — JSONL tree sessions. Branch, fork, clone, jump to any point with `/tree`. All history preserved in a single file.
- **Extensions** — TypeScript modules adding custom tools, commands, event handlers, UI components.
- **Skills** — On-demand capability packages following the [Agent Skills](https://agentskills.io) standard.
- **Prompt templates** — Reusable markdown prompts with `{{variables}}`.
- **Packages** — Bundle and share extensions, skills, prompts, themes via npm or git.
- **Session compaction** — Automatic context window management.
- **Multi-mode** — Interactive, print (`-p`), JSON, RPC, and SDK.

## Philosophy

D4C ships with opinionated built-ins that I need for my daily software engineering work:

- **MCP** — Context7 and Playwright MCP servers for documentation retrieval and browser automation.
- **Plan mode** — Deterministic output from the agent before making any irreversible changes.
- **TODO tool** — Track tasks during a session without confusing the model.
- **websearch tool** — Real-time web access integrated into agent workflows.

Everything else is extensible. Add or replace capabilities via extensions, skills, and packages.

## Monorepo Packages

| Package | Description |
|---------|-------------|
| **[@earendil-works/pi-coding-agent](packages/coding-agent)** | D4C CLI |
| **[@earendil-works/pi-ai](packages/ai)** | Unified multi-provider LLM API |
| **[@earendil-works/pi-agent-core](packages/agent)** | Agent runtime with tool calling and state management |
| **[@earendil-works/pi-tui](packages/tui)** | Terminal UI library with differential rendering |

For Slack/chat automation and workflows see [earendil-works/pi-chat](https://github.com/earendil-works/pi-chat).

## Permissions & Containerization

Pi does not include a built-in permission system for restricting filesystem, process, network, or credential access. By default, it runs with the permissions of the user and process that launched it.

If you need stronger boundaries, containerize or sandbox Pi. See [packages/coding-agent/docs/containerization.md](packages/coding-agent/docs/containerization.md) for three patterns:

- **OpenShell**: run the whole `pi` process in a policy-controlled sandbox.
- **Gondolin extension**: keep `pi` and provider auth on the host while routing built-in tools and `!` commands into a local Linux micro-VM.
- **Plain Docker**: run the whole `pi` process in a local container for simple isolation.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for contribution guidelines and [AGENTS.md](AGENTS.md) for project-specific rules (for both humans and agents).
## Development

```bash
npm install --ignore-scripts  # Install all dependencies without running lifecycle scripts
npm run build        # Build all packages
npm run check        # Lint, format, and type check
./test.sh            # Run tests
./pi-test.sh         # Run d4c from sources
```

## Supply-chain hardening

We treat npm dependency changes as reviewed code changes.

- Direct external dependencies are pinned to exact versions. Internal workspace packages remain version-ranged.
- `.npmrc` sets `save-exact=true` and `min-release-age=2` to avoid same-day dependency releases during npm resolution.
- `package-lock.json` is the dependency ground truth. Pre-commit blocks accidental lockfile commits unless `PI_ALLOW_LOCKFILE_CHANGE=1` is set.
- `npm run check` verifies pinned direct deps, native TypeScript import compatibility, and the generated coding-agent shrinkwrap.
- The published CLI package includes `packages/coding-agent/npm-shrinkwrap.json`, generated from the root lockfile, to pin transitive deps for npm users.
- Release smoke tests use `npm run release:local` to build, pack, and create isolated npm and Bun installs outside the repo before tagging a release.
- Local release installs, documented npm installs, and `pi update --self` use `--ignore-scripts` where supported.
- CI installs with `npm ci --ignore-scripts`, and a scheduled GitHub workflow runs `npm audit --omit=dev` plus `npm audit signatures --omit=dev`.
- Shrinkwrap generation has an explicit allowlist for dependency lifecycle scripts; new lifecycle-script deps fail checks until reviewed.

## License

MIT
