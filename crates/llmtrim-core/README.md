# llmtrim-core

The static, deterministic compression engine behind [llmtrim](https://github.com/fkiene/llmtrim) —
as an embeddable Rust library. It takes a provider-shaped LLM request body (OpenAI,
Anthropic or Google JSON), compresses the input with deterministic algorithms only (no
auxiliary model, no embeddings, **no network, no async/tokio**), and reports the token
delta. Cut input tokens 30–90% with zero extra model calls.

```rust
use llmtrim_core::{compress, config::DenseConfig, ir::ProviderKind};

// Use a named workload preset…
let cfg = DenseConfig::preset("aggressive").unwrap();
let out = llmtrim_core::compress_with_config(&request_json, Some(ProviderKind::OpenAi), &cfg)?;
println!("{} -> {} tokens", out.input_tokens_before, out.input_tokens_after);
// send `out.request_json` to the provider unchanged

// …or load configuration from the environment and auto-detect the provider:
let out = compress(&request_json, None)?;
```

## API

- [`compress`] — load config from the environment / config file, optionally auto-detect the
  provider from the body shape.
- [`compress_with_config`] — compress with an explicit [`config::DenseConfig`] (no
  environment access; the deterministic core used by tests and embedders).
- [`route`] — pick the workload preset for a request from its structure alone.
- [`CompressResult`] — the compressed body, the rehydration plan, per-stage reports and the
  measured token deltas.

## Bindings

Python, Ruby, Swift and Kotlin bindings are generated from this engine via UniFFI — see
[`llmtrim-uniffi`](../llmtrim-uniffi). The `llmtrim` CLI and MITM proxy live in the
[`llmtrim`](../llmtrim-cli) crate.

## License

AGPL-3.0-only.
