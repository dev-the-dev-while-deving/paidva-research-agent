# CONTEXT — V1 research agent

Design docs: `../V1 - Research & Conversational Agent/` (PRD, plan, framework).

## Stack
- Rust 1.99 (edition 2024), one crate `agent`, binary `agent`. Deps: tokio, reqwest (rustls, json), serde/serde_json, schemars, clap, rusqlite (bundled), anyhow, sha2, scraper, quick-xml.
- Model: Groq, OpenAI-compatible, strict `json_schema` output (verified on all 3 models).
  - writer/planner/analyst: `openai/gpt-oss-120b` · router/checker: `openai/gpt-oss-20b` · fallback: `qwen/qwen3.8-27b`
- Python sandbox: uv venv at `~/.agent/py` (numpy, scipy, pandas, statsmodels, sympy, scikit-learn), run under macOS `sandbox-exec` (no network, writes only to a temp dir, 30 s timeout).
- Keys: `GROQ_API_KEY`, `GEMINI_API_KEY`, `OPENROUTER_API_KEY` from `.env` (gitignored, mode 600). Never log or commit them.
- Providers: model spec `openai/gpt-oss-120b` = Groq, `gemini:<id>` = Google AI Studio, `openrouter:<id>` = OpenRouter.
  Limits are per account per model (Groq 200k tokens/day/model; OpenRouter free 50 requests/day total; Gemini free often 503).

## Layout
- `src/llm.rs` client + `structured::<T>()` + SQLite cache + 429 backoff/fallback
- `src/answer.rs` Claim/Tag/Answer + invariant checks
- `src/sandbox.rs` · `src/search.rs` (wikipedia, arxiv, semantic scholar, web) · `src/fetch.rs`
- `src/agent.rs` the loop: assemble → route → plan → act → draft → verify → score → trace
- `src/verify.rs` · `src/stats.rs` · `src/main.rs` (CLI: `init`, `ask`, `chat`, `trace`, `eval`, `report`)
- `src/eval.rs` + `evals/dev.jsonl` (starter set) → `evals/results/<set>-<system>-<model>.jsonl` (append-only, resumes)
- `~/.agent/`: `agent.md`, `rules.toml`, `skills/*/SKILL.md`, `prompts/*.md`, `traces/`, `cache.db`

## Conventions
- Prompts live in `~/.agent/prompts/*.md`, read at runtime (no recompile to tune).
- Model proposes, Rust decides: every check that can be code is code.
- Concrete types, no traits until a 2nd implementation exists. `Arc`/clone freely.
- Every non-trivial module gets one `#[cfg(test)]` check.

## Constraints
- Budget $0/month: Groq free tier only. Laptop only. One user.
- Good-enough bar: (1) no number without a sandbox run or a quoted source, (2) no fabricated citations.

## Frugal decisions
| Need | Chosen | Skipped | Trade-off | Upgrade when |
|---|---|---|---|---|
| Sandbox | sandbox-exec + uv venv | Docker | weaker isolation, macOS-only | Docker installed / Linux |
| Search | Wikipedia + arXiv + Semantic Scholar APIs (no general web) | DDG scraping (dropped: evades bot detection), SearXNG | no news/blogs; S2 429s without `S2_API_KEY` | add Brave/Tavily key or pip-installed SearXNG |
| Ranking | keyword (BM25-style) | fastembed | misses paraphrases | recall visibly poor in traces |
| UI | plain CLI + REPL | ratatui TUI | less polish | after eval proves the system |
| Calibration | raw score, labelled "uncalibrated" | isotonic fit | not yet honest % | eval harness exists |

## Agents must not touch
`.env`, `CONTEXT.md`, `src/agent.rs`, `src/answer.rs`, `src/verify.rs`, `src/llm.rs`.
