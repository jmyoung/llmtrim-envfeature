# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **`status` is now a real status command**: a health chain in the header (daemon alive →
  port accepting via TCP probe → env wired to the daemon's port → CA present → age of the
  last recorded request), one calm line when healthy and one warning per broken link
  naming its fix. Stopped-but-still-wired (every LLM call failing) is now the loudest
  state instead of a calm "stopped". The empty-ledger message diagnoses *why* (not set up /
  set up but unwired / wired and waiting).
- **Health exit codes + `status -q`**: the snapshot exits 0 healthy / 1 stopped /
  2 degraded (`systemctl is-active` convention); `-q/--quiet` prints one word
  (`healthy|degraded|stopped`) for scripts. Exports (`--json/--csv`) stay exit 0 and carry
  the health in data instead.
- **`llmtrim doctor`**: read-only end-to-end diagnosis — binary, daemon, port, persisted
  env *and* this shell's env (catches terminals that predate `setup`), CA, autostart,
  ledger, version skew — one pass/fail row each, exits non-zero when something needs fixing.
- **`status --json` now includes the daemon**: `running`, derived `health`, pid/port,
  `uptime_secs`, `port_accepting`, `env_port`, `autostart`, `restarts`, daemon vs binary
  version, plus a top-level `last_request_ts`.
- **Version-skew detection**: the pidfile records the daemon's version; `status`/`doctor`
  warn when an updated binary hasn't been restarted into ("daemon is vX, binary is vY").
- **Crash visibility**: the supervisor counts crash-restarts in the pidfile; `status` and
  `doctor` surface "restarted N× — check log".
- **Today anchor in the hero**: `· today $X` next to the all-time saving (hidden when
  nothing priced ran today).
- **Run-at-login probe**: `autostart on` in the status header; autostart state checked in
  `doctor`.
- **Read-path tool-output compression (Stage T)**: adaptive compression of `tool_result`
  content — log windowing with severity-aware keeps (errors-only mode under pressure),
  diff/grep/plaintext handling, repeated-template masking — with cache discipline so
  provider prefix caching is never broken mid-conversation. Generated content (lockfiles,
  minified bundles, base64 blobs) is detected and skipped; ANSI/CR sequences are
  normalized before detection.
- **Shared terminal UI** module (width-aware panels and tables) used by `monitor`,
  `setup`, and `update` output.
- Lossless **embedded-JSON minification** in prose (exact numeric round-trip).
- Bench: tool-output corpus + Headroom head-to-head harness; judge-verdict parsing,
  transient-error skip, and cache-bust nonces for fair A/B runs.
- **MITM HTTPS interceptor** (`serve`): a man-in-the-middle proxy (hudsucker) that compresses
  every tool's LLM API calls in flight, with **streaming (SSE) pass-through**. Provider host
  allowlist + name-constrained CA derived from the `llm_providers` registry. `llmtrim ca`
  manages the local CA.
- **Interceptor preset tuned on real agent traffic** (A/B'd through `claude -p`): the live
  default is `hygiene` + exact `dedup` + **tool-description trimming (300 chars)**, which cuts
  **~35% of input on Claude Code with tool use intact** (verified end-to-end). The `cache`
  stage is **off by default and guarded** — it no longer touches a request that already has
  `cache_control` (fixes a real 400 against Anthropic). `tool_select` / n-gram / TOON are off
  (≈0 gain on agent/prose traffic); opt in via a config file.
- **Background daemon + live status**: `serve --daemon`, `stop`, `status` (tokens saved, cost
  saved, total spend priced per-model via `llm_providers`, per-provider), `autostart`
  (cross-platform via `auto-launch`).
- **Response capture**: output tokens measured by teeing the streamed response (JSON + SSE),
  recorded to the ledger for total-spend reporting.
- **Google Gemini** provider adapter (`contents`/`parts` wire shape, `generationConfig`).
- `llmtrim setup` — one-command bootstrap: ensure the CA, write `HTTPS_PROXY` +
  `NODE_EXTRA_CA_CERTS` to the shell profile (no IDE config touched), enable autostart, start
  the daemon. `install.sh` runs it for a true one-liner.
- `llmtrim uninstall` — transparent one-command inverse: stop the daemon, disable
  autostart, strip the shell-profile block, remove the CA + state + binary (`--purge` also
  removes the savings ledger).
- `llmtrim update` — channel-aware end-user updates: binary installs self-update via the
  installer; cargo/Homebrew print their command. Crucially, `setup` now **stops the old
  daemon before restarting**, so updates actually go live (a binary swap alone left the old
  version serving). A cached (≤once/day), opt-out (`LLMTRIM_NO_UPDATE_CHECK`) release check
  surfaces a "vX.Y available" notice in `monitor`.
- Release scaffolding: `install.sh`, Homebrew formula, cross-platform release workflow, CI.

### Fixed
- `stop` now waits (≤5s) for the daemon to actually exit, and `start`/`serve --daemon`
  wait (≤10s) for the port to accept before reporting success — a `stop && start` cycle
  no longer races the dying daemon into a spurious EADDRINUSE crash-restart, and `status`
  right after `start` reads healthy instead of degraded-during-warm-up.

## [0.1.0] - Unreleased

Initial public release. Static, deterministic prompt/payload compression for the OpenAI and
Anthropic APIs — no auxiliary model calls, every transform measured with the real target
tokenizer and reverted if it doesn't earn its tokens.

### Added
- **Compression stages**: lexical retrieval (BM25/TextRank/MMR + DSLR sentence pruning),
  code skeletonization + minification (tree-sitter), data hygiene (JSON minify, numeric
  quantization, base64 stripping), columnar/TOON + CSV serialization, exact + SimHash
  near-duplicate collapse, reversible n-gram abbreviation, tool-schema selection/trimming,
  output-control shaping, DSS output shorthand, and provider prefix-cache breakpoints.
- **Workload presets** (`auto`, `safe`, `rag`, `agent`, `code`, `aggressive`, `cache`,
  `reasoning`) with structural auto-routing — zero model, picked from request shape.
- **Universal text handling**: language detection (whatlang) drives stopwords and the BM25
  tokenizer; Unicode-aware (UAX#29) tokenization works across scripts including CJK.
- **CLI**: `compress`, `send`, `serve` (MITM interceptor), `batch`, `eval`, `bench`, and
  `monitor` (savings dashboard; `status`/`gain` aliases). Response rehydration is internal.
- **Library surface**: `compress`, `compress_with_config` (the deterministic, tokio-free
  core; `default-features = false` for embedders).
- **Savings ledger** (SQLite) and live A/B benchmark harness (`--features live`).

[Unreleased]: https://github.com/fkiene/llmtrim/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/fkiene/llmtrim/releases/tag/v0.1.0
