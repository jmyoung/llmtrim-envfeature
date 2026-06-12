# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1] - 2026-06-12

### Fixed
- **Binary installers verify checksums again**: `install.sh` / `install.ps1` requested
  `<archive>.tar.gz.sha256` while CI uploads `<name>.sha256` — every prebuilt install
  failed (404) at the verification step.
- **`cargo install llmtrim` compiles again**: capped transitive `time` below 0.3.48,
  whose new blanket trait impl collides with `rcgen 0.14` (E0119) on a fresh, un-locked
  resolve. Install docs and the `update` hint now recommend `cargo install --locked`.

### Changed
- Release pipeline hardening: crates.io publishing via Trusted Publishing (OIDC, no
  stored token), Homebrew tap bump as a plain script, least-privilege CI permissions.

## [0.1.0] - 2026-06-12

Initial public release. llmtrim is a static, deterministic LLM prompt/payload compressor —
no auxiliary model calls, every transform measured with the real target tokenizer and
auto-reverted if it doesn't earn its tokens. Worst case is zero savings: never a bigger
bill, never a broken call.

### Compression engine
- **Input stages**: lexical retrieval (BM25/TextRank/MMR + DSLR sentence pruning), code
  skeletonization + minification (tree-sitter), data hygiene (JSON minify — including
  lossless minification of JSON embedded in prose with exact numeric round-trip — numeric
  quantization, base64 stripping), columnar/TOON + CSV serialization, exact + SimHash
  near-duplicate collapse, reversible n-gram abbreviation, tool-schema
  selection/trimming, output-control shaping, DSS output shorthand, and provider
  prefix-cache breakpoints.
- **Read-path tool-output compression (Stage T)**: adaptive compression of `tool_result`
  content — log windowing with severity-aware keeps (errors-only mode under pressure),
  diff/grep/plaintext handling, repeated-template masking. Generated content (lockfiles,
  minified bundles, base64 blobs) is detected and skipped; ANSI/CR sequences are
  normalized before detection. Cache discipline guarantees provider prefix caching is
  never broken mid-conversation.
- **Workload presets** (`auto`, `safe`, `rag`, `agent`, `code`, `aggressive`, `cache`,
  `reasoning`) with structural auto-routing — zero model calls, picked from request shape.
- **Universal text handling**: language detection (whatlang) drives stopwords and the
  BM25 tokenizer; Unicode-aware (UAX#29) tokenization works across scripts including CJK.

### HTTPS interceptor & daemon
- **MITM interceptor** (`serve`): compresses every tool's LLM API calls in flight, with
  streaming (SSE) pass-through. Provider host allowlist + name-constrained CA derived
  from the `llm_providers` registry (OpenAI, Anthropic, Google Gemini adapters; every
  non-LLM connection is blind-tunneled untouched). `llmtrim ca` manages the local CA.
- **Live default preset tuned on real agent traffic** (A/B'd through `claude -p`):
  `hygiene` + exact `dedup` + tool-description trimming (300 chars) cuts ~35% of input on
  Claude Code with tool use intact, verified end-to-end. The `cache` stage is off by
  default and never touches a request that already carries `cache_control`;
  `tool_select` / n-gram / TOON are off (≈0 gain on agent/prose traffic) and opt-in via
  config.
- **Background daemon**: `serve --daemon`, `start`/`stop` (with proper wait-for-exit and
  wait-for-port so restart cycles never race), `autostart` (cross-platform via
  `auto-launch`), crash-restart supervision with restart counting.
- **Response capture**: output tokens measured by teeing the streamed response
  (JSON + SSE), recorded to the ledger for total-spend reporting.

### Observability
- **`monitor`** — live savings dashboard (tokens saved, cost saved, total spend priced
  per-model via `llm_providers`, per-provider breakdown, today anchor next to the
  all-time saving).
- **`status`** — health chain in the header (daemon alive → port accepting → env wired →
  CA present → age of last request), one calm line when healthy and one warning per
  broken link naming its fix. Exit codes follow the `systemctl is-active` convention
  (0 healthy / 1 stopped / 2 degraded); `-q/--quiet` prints one word for scripts;
  `--json/--csv` exports carry daemon state, uptime, restarts, and version skew.
- **`doctor`** — read-only end-to-end diagnosis: binary, daemon, port, persisted env and
  this shell's env, CA, autostart, ledger, daemon-vs-binary version skew — one pass/fail
  row each, non-zero exit when something needs fixing.

### Install & lifecycle
- **`setup`** — one-command bootstrap: ensure the CA, write `HTTPS_PROXY` +
  `NODE_EXTRA_CA_CERTS` to the shell profile (env-level only — no IDE config touched),
  enable autostart, start the daemon. `install.sh` runs it for a true one-liner.
- **`uninstall`** — transparent inverse: stop the daemon, disable autostart, strip the
  shell-profile block, remove CA + state + binary (`--purge` also removes the ledger).
- **`update`** — channel-aware: binary installs self-update via the installer;
  cargo/Homebrew print their command. `setup` restarts the daemon so updates go live.
  A cached (≤once/day), opt-out (`LLMTRIM_NO_UPDATE_CHECK`) release check surfaces a
  "vX.Y available" notice in `monitor`.

### CLI & library
- **CLI**: `compress`, `send`, `serve`, `batch`, `eval`, `bench`, `monitor`
  (`status`/`gain` aliases). Response rehydration is internal.
- **Library surface**: `compress`, `compress_with_config` — the deterministic, tokio-free
  core (`default-features = false` for embedders).
- **Savings ledger** (SQLite) backing all spend/savings reporting.

### Benchmarks & release infrastructure
- Live A/B benchmark harness (`--features live`): every case sent twice (original and
  compressed), answered, scored, and billed at real rates; judge-verdict parsing,
  transient-error skip, and cache-bust nonces for fair runs. Tool-output corpus +
  Headroom head-to-head harness included.
- `install.sh` / `install.ps1`, Homebrew formula, cross-platform release workflow
  (6 targets with SLSA build provenance), CI on Linux/macOS/Windows with secret
  scanning, license compliance, and MSRV gates.

[Unreleased]: https://github.com/fkiene/llmtrim/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/fkiene/llmtrim/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/fkiene/llmtrim/releases/tag/v0.1.0
