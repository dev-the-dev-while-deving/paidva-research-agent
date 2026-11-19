mod agent;
mod answer;
mod eval;
mod llm;
mod papers;
mod sandbox;
mod search;
mod stats;
mod verify;

use agent::{Agent, Outcome};
use answer::{Answer, Evidence, Tag};
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::io::{BufRead, Write};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "agent", about = "Research and statistics agent (V1)")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Write default prompts, rules and skills into ~/.agent (existing files are kept unless --force).
    Init { #[arg(long)] force: bool },
    /// Ask one question and exit.
    Ask {
        question: String,
        /// Data files the Python analyst can read (e.g. --file data.csv).
        #[arg(long = "file")]
        files: Vec<PathBuf>,
        /// Print the answer, evidence and report as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Interactive chat. /why shows the last trace, /quit exits.
    Chat {
        #[arg(long = "file")]
        files: Vec<PathBuf>,
    },
    /// Show the last trace.
    Trace,
    /// Run a question set through the plain model or the full agent and grade it (resumes if interrupted).
    Eval {
        #[arg(long, default_value = "evals/dev.jsonl")]
        set: PathBuf,
        /// plain (one call, same model) or full (the agent)
        #[arg(long, default_value = "full")]
        system: String,
        /// Main model, pinned with no fallback so comparisons are fair.
        #[arg(long, default_value = "openai/gpt-oss-120b")]
        model: String,
        /// Only this question type: factual, maths, research, abstain, rule_trigger.
        #[arg(long = "type")]
        kind: Option<String>,
        #[arg(long, default_value_t = usize::MAX)]
        limit: usize,
        /// Discard earlier results for this set/system/model and start over.
        #[arg(long)]
        fresh: bool,
        /// Re-run only these question ids (comma-separated), even if already done.
        #[arg(long, value_delimiter = ',')]
        ids: Vec<String>,
    },
    /// Summarise one or more eval result files side by side.
    Report { files: Vec<PathBuf> },
}

const DEFAULTS: &[(&str, &str)] = &[
    ("agent.md", include_str!("../home/agent.md")),
    ("rules.toml", include_str!("../home/rules.toml")),
    ("skills/statistics/SKILL.md", include_str!("../home/skills/statistics/SKILL.md")),
    ("skills/probability/SKILL.md", include_str!("../home/skills/probability/SKILL.md")),
    ("prompts/router.md", include_str!("../home/prompts/router.md")),
    ("prompts/planner.md", include_str!("../home/prompts/planner.md")),
    ("prompts/researcher.md", include_str!("../home/prompts/researcher.md")),
    ("prompts/analyst.md", include_str!("../home/prompts/analyst.md")),
    ("prompts/answer.md", include_str!("../home/prompts/answer.md")),
    ("prompts/check.md", include_str!("../home/prompts/check.md")),
    ("prompts/fix.md", include_str!("../home/prompts/fix.md")),
    ("prompts/effects.md", include_str!("../home/prompts/effects.md")),
    ("prompts/compare.md", include_str!("../home/prompts/compare.md")),
    ("prompts/answer_check.md", include_str!("../home/prompts/answer_check.md")),
];

/// GROQ_API_KEY from the environment, else ./.env, else ~/.agent/.env.
fn load_env() {
    if std::env::var("GROQ_API_KEY").is_ok() { return; }
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    for p in [PathBuf::from(".env"), home.join(".agent/.env")] {
        if let Ok(text) = std::fs::read_to_string(&p) {
            for line in text.lines() {
                if let Some((k, v)) = line.split_once('=') {
                    // SAFETY: single-threaded at this point (before the tokio runtime spawns work).
                    unsafe { std::env::set_var(k.trim(), v.trim()) };
                }
            }
            return;
        }
    }
}

fn init(force: bool) -> Result<()> {
    let dir = PathBuf::from(std::env::var("HOME")?).join(".agent");
    for (rel, body) in DEFAULTS {
        let p = dir.join(rel);
        if p.exists() && !force { println!("kept     {}", p.display()); continue; }
        std::fs::create_dir_all(p.parent().unwrap())?;
        std::fs::write(&p, body)?;
        println!("wrote    {}", p.display());
    }
    if !sandbox::python(&dir.parent().unwrap().to_path_buf()).exists() {
        println!("\nPython env missing. Create it with:\n  cd ~/.agent && uv venv py --python 3.13 && VIRTUAL_ENV=~/.agent/py uv pip install numpy scipy pandas statsmodels sympy scikit-learn");
    }
    Ok(())
}

fn render(a: &Answer, ev: &[Evidence], r: &agent::Report) {
    println!("\n{}\n", a.direct_answer);
    for c in &a.claims {
        let tag = match c.tag { Tag::Observed => "observed", Tag::Sourced => "sourced", Tag::Inferred => "inferred", Tag::Knowledge => "knowledge", Tag::Speculation => "speculation" };
        let ids = if c.evidence.is_empty() { String::new() } else { format!(" {}", c.evidence.join(",")) };
        println!("  • {} [{tag}{ids}]", c.text);
    }
    for (b, (hyp, trail)) in a.bayes.iter().zip(&r.bayes) {
        println!("\nBelief update — {hyp}");
        println!("  prior {:.2}  ({})", trail[0], b.prior_reason);
        for (s, p) in b.steps.iter().zip(&trail[1..]) {
            println!("  {} LR {:.2} → {:.2}  ({})", s.evidence, s.likelihood_ratio.clamp(0.05, 20.0), p, s.reason);
        }
    }
    for p in &r.pooled {
        println!("\nPooled ({} studies, random effects): {} {:.3} [95% CI {:.3}–{:.3}], I² {:.0}%", p.k, p.measure, p.estimate, p.ci_low, p.ci_high, p.i2 * 100.0);
    }
    if !r.cross_checks.is_empty() {
        println!("\nExact vs simulation:");
        for c in &r.cross_checks {
            println!("  {} {}: exact {:.6} · simulated {:.6} ± {:.6}", if c.agree { "✓" } else { "✗" }, c.name, c.exact, c.sim, c.se);
        }
    }
    if !a.unknowns.is_empty() {
        println!("\nStill uncertain:");
        for u in &a.unknowns { println!("  - {u}"); }
    }
    // Only sources a claim or belief step actually cites, one line per URL.
    let cited: std::collections::HashSet<&str> = a.claims.iter().flat_map(|c| c.evidence.iter())
        .chain(a.bayes.iter().flat_map(|b| b.steps.iter().map(|s| &s.evidence))).map(String::as_str).collect();
    let mut by_url: Vec<(&str, &str, Vec<&str>)> = vec![];
    for e in ev.iter().filter(|e| !e.url.is_empty() && cited.contains(e.id.as_str())) {
        match by_url.iter_mut().find(|x| x.0 == e.url) {
            Some(x) => x.2.push(&e.id),
            None => by_url.push((&e.url, &e.title, vec![&e.id])),
        }
    }
    if !by_url.is_empty() {
        println!("\nSources:");
        for (url, title, ids) in by_url { println!("  [{}] {title} — {url}", ids.join(",")); }
    }
    let warn = if r.confidence < 0.45 { "  ⚠ low confidence: treat as a lead, not an answer" } else { "" };
    println!("\nconfidence {:.2} (uncalibrated) · {} fixed round(s), {} relabelled, {} dropped{warn}", r.confidence, r.fix_rounds, r.relabelled, r.dropped);
    for e in &r.errors { eprintln!("  note: {e}"); }
}

fn last_trace() -> Result<()> {
    let p = PathBuf::from(std::env::var("HOME")?).join(".agent/traces/trace.jsonl");
    let text = std::fs::read_to_string(&p)?;
    let v: serde_json::Value = serde_json::from_str(text.lines().last().unwrap_or("{}"))?;
    let r = &v["report"];
    println!("question: {}", v["question"].as_str().unwrap_or(""));
    println!("mode: {} · hard: {} · rules: {} · skills: {}", r["mode"], r["hard"], r["rules"], r["skills"]);
    println!("plan: {}", serde_json::to_string_pretty(&r["plan"])?);
    println!("first-draft issues: {}", serde_json::to_string_pretty(&r["first_draft_issues"])?);
    for e in v["evidence"].as_array().into_iter().flatten() {
        println!("\n[{}] {}\n{}", e["id"].as_str().unwrap_or(""), e["title"].as_str().unwrap_or(""), e["text"].as_str().unwrap_or("").chars().take(400).collect::<String>());
        if let Some(code) = e["code"].as_str().filter(|c| !c.is_empty()) { println!("--- code ---\n{code}"); }
    }
    let calls = v["calls"].as_array().cloned().unwrap_or_default();
    let tokens: u64 = calls.iter().filter_map(|c| c["tokens"].as_u64()).sum();
    println!("\n{} model calls · {tokens} tokens · confidence {}", calls.len(), r["confidence"]);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    load_env();
    match Cli::parse().cmd {
        Cmd::Init { force } => init(force)?,
        Cmd::Trace => last_trace()?,
        Cmd::Eval { set, system, model, kind, limit, fresh, ids } => {
            let out = eval::run(eval::Opts { set, system, model, kind, limit, fresh, ids }).await?;
            eval::report(&[out])?;
        }
        Cmd::Report { files } => eval::report(&files)?,
        Cmd::Ask { question, files, json } => {
            let agent = Agent::new()?;
            match agent.handle(&question, &files, &[]).await? {
                Outcome::Ask(q) => println!("{q}"),
                Outcome::Answered { answer, evidence, report } => {
                    if json { println!("{}", serde_json::to_string_pretty(&serde_json::json!({"answer": answer, "evidence": evidence, "report": report}))?); }
                    else { render(&answer, &evidence, &report); }
                }
            }
        }
        Cmd::Chat { files } => {
            let agent = Agent::new()?;
            let mut history: Vec<(String, String)> = vec![];
            let stdin = std::io::stdin();
            loop {
                print!("\n> ");
                std::io::stdout().flush()?;
                let mut line = String::new();
                if stdin.lock().read_line(&mut line)? == 0 { break; }
                let q = line.trim();
                match q {
                    "" => continue,
                    "/quit" | "/exit" => break,
                    "/why" => { last_trace().unwrap_or_else(|e| eprintln!("{e}")); continue; }
                    _ => {}
                }
                match agent.handle(q, &files, &history).await {
                    Ok(Outcome::Ask(c)) => { println!("{c}"); history.push((q.into(), c)); }
                    Ok(Outcome::Answered { answer, evidence, report }) => { render(&answer, &evidence, &report); history.push((q.into(), answer.direct_answer)); }
                    Err(e) => eprintln!("error: {e:#}"),
                }
            }
        }
    }
    Ok(())
}
