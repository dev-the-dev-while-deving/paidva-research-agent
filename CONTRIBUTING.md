# Contributing to Paidva

Thanks for helping. Paidva is small on purpose, so the bar is: **does this make answers more correct or easier to check?**

## Ground rules

1. **The model proposes, code decides.** If a check can be written as code, write it as code, not as a prompt instruction.
2. **No number without a sandbox run or a quoted source. No fabricated citations.** Every change must keep both.
3. **Measure it.** A behaviour change comes with a before/after from `paidva eval` on the dev set (`--system full`, same pinned model).
4. **Keep it lean.** Prefer the standard library and existing crates. Use concrete types, and add no new trait until a second implementation exists.
5. **One runnable check per non-trivial module**, as a `#[cfg(test)]` test.

## Setup

```bash
cp .env.example .env     # one model key is enough
cargo build
cargo test
./target/debug/paidva init --force
```

## Good first contributions

- **Eval questions** for `evals/dev.jsonl`. The answer key must be computed in Python, not recalled; put the code in the PR description.
- **Sandbox:** a Linux backend (bubblewrap or landlock), and memory caps.
- **Retrieval:** new sources with an API (no scraping that evades bot detection).
- **Verifier loopholes:** find a claim that passes verification but shouldn't, and add a failing test.

## Pull requests

- Keep each PR to one change. The description says what it does, why, and the eval delta.
- Don't commit `.env`, API keys, or anything under `~/.agent/`.
- By contributing, you agree your work is licensed under [AGPL-3.0](LICENSE).

## Reporting security issues

If you find a sandbox escape or a way to leak keys, please open a private security advisory on GitHub instead of a public issue.
