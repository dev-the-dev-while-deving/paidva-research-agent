# Paidva: Research Agent V1

**A research and statistics agent that shows its work.** Every claim it makes is tagged with where it came from. Every number is computed in a sandbox, not recalled. Every citation is checked word for word against the source. Every answer gets a confidence score you can actually rank by.

It's a Rust CLI running open-weight models on free API tiers, and it's the first of three versions. The long-term question is whether a small agent can **keep improving itself**, round after round, without stalling.

> **Status:** V1 is working end to end and has a first eval round. The early results are promising, but they are **not yet statistically significant**. See [Results](#results).

---

## Why this exists

Today's research assistants have three documented problems:

1. **Citations that exist but don't back the claim.** Deep-research systems keep their links working, but studies find large shares of their statements aren't supported by the sources they cite.
2. **Confidence that means nothing.** A model says "95% sure" and is wrong. Few products give per-claim confidence at all.
3. **Statistics nobody checks.** Tools that run code show you the code, but they don't verify it.

Paidva tackles all three in code, not in the prompt. The rule is: **the model proposes, Rust decides.** Any check that can be written as code is written as code.

## What an answer looks like

Each claim in an answer carries one of these tags:

| Tag | Meaning | How it's checked |
|---|---|---|
| `observed` | Came out of code the agent ran | The number must appear in the sandbox output |
| `sourced` | Came from a paper or page | The quoted passage must appear verbatim in the fetched source, and a separate checker that sees only the claim and the passage must agree that the passage supports it |
| `inferred` | Reasoned from other claims | Must cite the claims it depends on |
| `knowledge` | Model background knowledge | Allowed, but labelled as such |
| `speculation` | A guess | Allowed, but labelled as such |

Claims that fail verification are either fixed or downgraded to `speculation`, and the answer's confidence drops. Ask `/why` in chat to see the full trace: rules loaded, tools called, sources read and checks run.

## How it works

```
question
  → assemble   load rules, skills, memory
  → route      quick answer · maths/stats · research · abstain
  → plan
  → act        search (Wikipedia, arXiv, Semantic Scholar) · fetch full text · run Python in the sandbox
  → draft      structured JSON answer with tagged claims
  → verify     recompute numbers, check quotes verbatim, cross-check exact maths against simulation,
               flag false premises, rewrite a headline that says more than the verified claims
  → score      confidence from the verification results
  → trace      everything saved for /why and for evals
```

- **Sandboxed Python** (numpy, scipy, pandas, statsmodels, sympy, scikit-learn) under macOS `sandbox-exec`: no network, no access to your home folder or keys, writes only to a temp dir, 30 s timeout.
- **Exact vs. simulated cross-checks.** A probability answer must agree with a Monte Carlo run to within 3 standard errors, at 0.25% precision.
- **Bayesian updates and study pooling** are done in code, never by the model.
- **Editable rulebook and skills** (`~/.agent/rules.toml`, `skills/*/SKILL.md`, `prompts/*.md`) are read at runtime, so you can tune it without recompiling.
- **Provider router** across Groq, Google AI Studio and OpenRouter, with structured output, a SQLite response cache, and back-off and fallback on rate limits.

## Results

First eval round (2026-11-19): 42 questions (a 26-question dev set and a 16-question hard set with fresh numbers the model can't have memorised). Each comparison runs the **same pinned model**, either alone (`plain`) or inside the agent (`full`).

| Set · model | System | Accuracy | Most-confident half right | Calibration error | Dead citations |
|---|---|---|---|---|---|
| Dev · qwen3.8-27b | plain | 65% | 78% | 0.34 | 2/10 |
| Dev · qwen3.8-27b | **full** | **87%** | **100%** | **0.13** | **0/17** |
| Hard · qwen3.8-27b | plain | 33% | 38% | 0.64 | 0/1 |
| Hard · qwen3.8-27b | **full** | **44%** | **80%** | **0.24** | **0/27** |
| Dev · gpt-oss-120b | plain | 91% | 100% | 0.14 | 4/36 |
| Dev · gpt-oss-120b | **full** | **94%** | 100% | **0.07** | **0/111** |

**What this shows**

- **Zero dead citations** across every full-agent run. The plain models had 11–20% dead links.
- **On a weaker model, the framework gives a large lift:** better accuracy and much better calibration. The agent never lost a question the plain model got right.
- **Confidence becomes useful.** On the hard set, plain qwen's most-confident answers were right 38% of the time, so its confidence carried no information. The agent's most-confident answers were right 80% of the time, so you know what to check first.
- **On a strong model, accuracy barely moves.** The gain there is calibration and clean citations.

**What it doesn't show yet**

- **Nothing is statistically significant yet** (paired McNemar p ≥ 0.25). Free-tier rate limits left only 8–16 shared questions per comparison.
- **It's expensive:** 10–25× the tokens and 30–70 s per question.
- **Research answers are faithful but thin** (rubric 0.42–0.62), because retrieval has no general web search.
- **Confidence weights are hand-set**, so the score ranks well but "0.8" doesn't yet mean 80%.

`agent report evals/results/*.jsonl` reproduces every number above. Answer keys were computed in Python, not recalled.

## Quick start

Requirements: Rust (edition 2024), macOS (for `sandbox-exec`), [uv](https://docs.astral.sh/uv/), and a free [Groq](https://console.groq.com) API key.

```bash
echo "GROQ_API_KEY=..." > .env  # optional: GEMINI_API_KEY, OPENROUTER_API_KEY
cargo build --release
./target/release/agent init     # writes prompts, rules and skills into ~/.agent
./target/release/agent ask "If a test is 90% sensitive and 95% specific and prevalence is 2%, what's P(disease | positive)?"
./target/release/agent ask "Is the treatment effect significant?" --file examples/trial.csv
./target/release/agent chat     # /why shows the last trace, /quit exits
```

Run evals:

```bash
./target/release/agent eval --set evals/hard.jsonl --system plain --model openai/gpt-oss-120b
./target/release/agent eval --set evals/hard.jsonl --system full  --model openai/gpt-oss-120b
./target/release/agent report evals/results/hard-*.jsonl
```

## Layout

```
src/
  main.rs      CLI: init · ask · chat · trace · eval · report
  agent.rs     the loop: assemble → route → plan → act → draft → verify → score → trace
  answer.rs    Claim / Tag / Answer types and their invariants
  verify.rs    quote, number and inference checks
  llm.rs       OpenAI-compatible client, structured output, cache, fallback
  sandbox.rs   sandboxed Python runner
  search.rs    Wikipedia, arXiv, Semantic Scholar
  papers.rs    paper full text
  stats.rs     Bayes updates, pooling, cross-checks
  eval.rs      graders, calibration, paired comparisons
home/          default prompts, rules and skills (installed to ~/.agent)
evals/         question sets, data files, results
research/      market survey, eval-method notes, V1 field report
```

## Roadmap

| Version | Goal | Key idea |
|---|---|---|
| **V1** *(this repo)* | Prove the system beats the same model on its own | Evidence tags, sandboxed maths, claim-level citation checks, calibrated confidence |
| **V2** | The agent proposes its own improvements; I approve them | Nightly loop: failed traces → grouped by cause → one proposed rule, skill or prompt change → tested on the dev set → reviewed by me. Also an A/B test of a **Nyaya-logic layer** (evidence by *pramana*, a *hetvabhasa* fallacy check, *vada*-style debate) |
| **V3** | The agent keeps improving itself, round after round | An archive of agent variants (Darwin Gödel Machine-style) evolving small, testable units such as rules, skills and examples rather than code, plus a self-play curriculum. The question: do improvements **compound** or stall? |

**Next for V1:** a tested statistics library inside the sandbox, a deterministic audit of attached data, honest failure messages, a rate-limit-aware scheduler, and growing the eval to 150+ questions with generated maths variants.

## Constraints

- **$0/month:** free API tiers only, everything else on a laptop.
- **Two non-negotiables:** no number without a sandbox run or a quoted source, and no fabricated citations.

---

Built by [Devarshee Mude](https://github.com/dev-the-dev-while-deving).
