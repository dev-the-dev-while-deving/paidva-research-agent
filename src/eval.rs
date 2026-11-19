//! Eval harness: run a question set through the plain model or the full agent, grade, report.
//! Results append to evals/results/<set>-<system>-<model>.jsonl, so an interrupted run resumes
//! (free-tier daily caps mean a full set can span days).
use crate::agent::{Agent, Outcome};
use crate::answer::Kind;
use crate::llm::Role;
use crate::{stats, verify};
use anyhow::{Context, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Question {
    pub id: String,
    /// factual | maths | research | abstain | rule_trigger
    #[serde(rename = "type")]
    pub kind: String,
    pub question: String,
    /// factual: correct if the answer contains any of these (case-insensitive).
    #[serde(default)]
    pub accept: Vec<String>,
    /// maths: numeric answer key and absolute tolerance.
    #[serde(default)]
    pub answer: Option<f64>,
    #[serde(default)]
    pub tolerance: Option<f64>,
    /// research: items a good answer must cover.
    #[serde(default)]
    pub rubric: Vec<String>,
    /// rule_trigger: contextual rule ids that must load.
    #[serde(default)]
    pub expected_rules: Vec<String>,
    /// Data files, relative to the question file.
    #[serde(default)]
    pub files: Vec<PathBuf>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Row {
    pub id: String,
    pub kind: String,
    pub system: String,
    pub model: String,
    pub answer: String,
    pub confidence: f64,
    /// None when the item is not gradable for this system (rule triggers on plain).
    pub correct: Option<bool>,
    /// 0–1 partial credit (rubric fraction); equals correct for the others.
    pub score: f64,
    pub citations: Vec<String>,
    pub dead_citations: usize,
    pub rules_loaded: Vec<String>,
    pub calls: usize,
    pub tokens: u64,
    pub secs: f64,
    pub note: String,
    pub error: String,
}

#[derive(Deserialize, JsonSchema)]
struct Plain { answer: String, confidence: f64, citations: Vec<String> }

#[derive(Deserialize, JsonSchema)]
struct Abstained { abstained: bool, reason: String }

#[derive(Deserialize, JsonSchema)]
struct Rubric { met: Vec<bool> }

const PLAIN_SYS: &str = "Answer the user's question as accurately as you can. If it cannot be answered, or rests on a false premise, say so. \
confidence: your probability (0-1) that your answer is correct. citations: URLs of sources you relied on, or [] if none.";

pub fn load(path: &Path) -> Result<Vec<Question>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty() && !l.trim_start().starts_with("//"))
        .map(|(i, l)| serde_json::from_str(l).with_context(|| format!("{}:{}", path.display(), i + 1))).collect()
}

pub struct Opts { pub set: PathBuf, pub system: String, pub model: String, pub kind: Option<String>, pub limit: usize, pub fresh: bool, pub ids: Vec<String> }

pub async fn run(o: Opts) -> Result<PathBuf> {
    anyhow::ensure!(o.system == "plain" || o.system == "full", "--system must be plain or full");
    let qs = load(&o.set)?;
    let base = o.set.parent().unwrap_or(Path::new("."));
    let stem = o.set.file_stem().unwrap_or_default().to_string_lossy();
    let out = base.join("results").join(format!("{stem}-{}-{}.jsonl", o.system, o.model.replace('/', "_")));
    std::fs::create_dir_all(out.parent().unwrap())?;
    if o.fresh { let _ = std::fs::remove_file(&out); }
    let done: HashSet<String> = std::fs::read_to_string(&out).unwrap_or_default().lines()
        .filter_map(|l| serde_json::from_str::<Row>(l).ok()).filter(|r| r.error.is_empty()).map(|r| r.id).collect();

    let mut agent = Agent::new()?;
    agent.llm.main_model = Some(o.model.clone());
    let http = reqwest::Client::builder().timeout(Duration::from_secs(10)).user_agent("research-agent-eval/0.1").build()?;
    let todo: Vec<&Question> = qs.iter()
        .filter(|q| o.kind.as_ref().is_none_or(|k| &q.kind == k))
        .filter(|q| !(o.system == "plain" && q.kind == "rule_trigger"))
        // Named ids always re-run (the newer row replaces the old one in reports).
        .filter(|q| if o.ids.is_empty() { !done.contains(&q.id) } else { o.ids.contains(&q.id) })
        .take(o.limit).collect();
    eprintln!("{} to run ({} already in {})", todo.len(), done.len(), out.display());

    for (n, q) in todo.iter().enumerate() {
        eprintln!("\n━━ [{}/{}] {} ({}) {}", n + 1, todo.len(), q.id, q.kind, q.question);
        let files: Vec<PathBuf> = q.files.iter().map(|f| base.join(f)).collect();
        let t = Instant::now();
        let mut row = Row { id: q.id.clone(), kind: q.kind.clone(), system: o.system.clone(), model: o.model.clone(), ..Default::default() };
        let res = if o.system == "plain" { plain(&agent, q, &files, &mut row).await } else { full(&agent, q, &files, &mut row).await };
        row.secs = t.elapsed().as_secs_f64();
        {
            let calls = agent.llm.calls.lock().unwrap();
            row.calls = calls.len();
            row.tokens = calls.iter().map(|c| c.tokens).sum();
        }
        match res {
            // Crashes are reported separately, not scored as wrong answers.
            Err(e) => { row.error = format!("{e:#}"); row.correct = None; }
            Ok(()) => {
                row.dead_citations = dead_links(&http, &row.citations).await;
                if let Err(e) = grade(&agent, q, &mut row).await { row.error = format!("grading: {e:#}"); }
            }
        }
        eprintln!("   → {} · conf {:.2} · {:.0}s{}", match row.correct { Some(true) => "✓ correct", Some(false) => "✗ wrong", None => "– n/a" },
            row.confidence, row.secs, if row.error.is_empty() { String::new() } else { format!(" · error: {}", row.error.chars().take(120).collect::<String>()) });
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&out)?;
        writeln!(f, "{}", serde_json::to_string(&row)?)?;
    }
    Ok(out)
}

/// One call to the same model, asked for the same things (answer, confidence, citations), no system around it.
async fn plain(agent: &Agent, q: &Question, files: &[PathBuf], row: &mut Row) -> Result<()> {
    agent.llm.calls.lock().unwrap().clear();
    let mut user = q.question.clone();
    for f in files {
        let text: String = std::fs::read_to_string(f)?.chars().take(12_000).collect();
        user.push_str(&format!("\n\nContents of {}:\n{text}", f.file_name().unwrap_or_default().to_string_lossy()));
    }
    let p: Plain = agent.llm.structured("plain", Role::Main, PLAIN_SYS, &user, 0.2, 0).await?;
    row.answer = p.answer;
    row.confidence = p.confidence.clamp(0.0, 1.0);
    row.citations = p.citations;
    Ok(())
}

async fn full(agent: &Agent, q: &Question, files: &[PathBuf], row: &mut Row) -> Result<()> {
    match agent.handle(&q.question, files, &[]).await? {
        Outcome::Ask(c) => { row.answer = format!("(asked to clarify) {c}"); row.confidence = 0.0; }
        Outcome::Answered { answer, evidence, report } => {
            row.answer = std::iter::once(answer.direct_answer.clone()).chain(answer.claims.iter().map(|c| c.text.clone())).collect::<Vec<_>>().join("\n");
            row.confidence = report.confidence;
            row.rules_loaded = report.rules.clone();
            let cited: HashSet<&String> = answer.claims.iter().flat_map(|c| c.evidence.iter()).collect();
            row.citations = evidence.iter().filter(|e| e.kind == Kind::Source && cited.contains(&e.id)).map(|e| e.url.clone())
                .collect::<HashSet<_>>().into_iter().collect();
            row.note = format!("first-draft issues {}, relabelled {}, dropped {}", report.first_draft_issues.len(), report.relabelled, report.dropped);
        }
    }
    Ok(())
}

/// Citations that can't exist: malformed URL, DNS failure, 404 or 410. Other errors (403, timeouts) are not counted.
async fn dead_links(http: &reqwest::Client, urls: &[String]) -> usize {
    let mut dead = 0;
    for u in urls.iter().take(6) {
        let Ok(url) = reqwest::Url::parse(u.trim()) else { dead += 1; continue };
        match http.get(url).send().await {
            Ok(r) if r.status() == 404 || r.status() == 410 => dead += 1,
            Err(e) if e.is_connect() || e.is_builder() => dead += 1,
            _ => {}
        }
    }
    dead
}

/// Graders: mechanical for factual/maths/rule_trigger; a fixed grader model (fast role, not pinned) for abstain/research.
/// Decimals, percentages (as fractions too) and simple fractions like "13/18".
fn answer_numbers(s: &str) -> Vec<f64> {
    let mut out: Vec<f64> = stats::numbers(s).into_iter().map(|x| x.0).collect();
    for w in s.split(|c: char| !(c.is_ascii_digit() || c == '/')) {
        if let Some((a, b)) = w.split_once('/') {
            if let (Ok(a), Ok(b)) = (a.parse::<f64>(), b.parse::<f64>()) {
                if b != 0.0 { out.push(a / b); }
            }
        }
    }
    out
}

async fn grade(agent: &Agent, q: &Question, row: &mut Row) -> Result<()> {
    let correct = match q.kind.as_str() {
        "factual" => q.accept.iter().any(|a| verify::norm(&row.answer).contains(&verify::norm(a))),
        "maths" => {
            let key = q.answer.context("maths question needs `answer`")?;
            let tol = q.tolerance.unwrap_or(1e-3 * key.abs().max(1.0));
            // Any number in the answer that isn't just restating an input from the question.
            let mut inputs: Vec<f64> = answer_numbers(&q.question);
            inputs.retain(|i| (i - key).abs() > tol); // the key may legitimately equal an input ("first 6" → 6)
            let ok = answer_numbers(&row.answer).into_iter()
                .filter(|v| !inputs.iter().any(|i| (i - v).abs() < 1e-12))
                .any(|v| (v - key).abs() <= tol);
            row.note = format!("{} | key {key} ± {tol}", row.note);
            ok
        }
        "abstain" => {
            let g: Abstained = agent.llm.structured("grade", Role::Fast,
                "You grade one response. abstained = true if it declines to answer, says it cannot be known, or points out that the question's premise is false, instead of answering as if the question were valid.",
                &format!("Question: {}\n\nResponse: {}", q.question, row.answer), 0.0, 0).await?;
            row.note = format!("{} | grader: {}", row.note, g.reason);
            g.abstained
        }
        "research" | "analysis" => {
            let items: String = q.rubric.iter().enumerate().map(|(i, r)| format!("{}. {r}\n", i + 1)).collect();
            let g: Rubric = agent.llm.structured("grade", Role::Fast,
                "You grade one response against a rubric. Return one boolean per rubric item, in order: true only if the response clearly covers that item.",
                &format!("Question: {}\n\nRubric:\n{items}\nResponse:\n{}", q.question, row.answer), 0.0, 0).await?;
            let met = g.met.iter().take(q.rubric.len()).filter(|b| **b).count();
            row.score = met as f64 / q.rubric.len().max(1) as f64;
            row.correct = Some(row.score >= 0.75);
            row.note = format!("{} | rubric {met}/{}", row.note, q.rubric.len());
            return Ok(());
        }
        "rule_trigger" => {
            let loaded: HashSet<&String> = row.rules_loaded.iter().collect();
            let hit = q.expected_rules.iter().filter(|r| loaded.contains(r)).count();
            row.note = format!("{} | rules {hit}/{} expected loaded", row.note, q.expected_rules.len());
            hit == q.expected_rules.len()
        }
        k => anyhow::bail!("unknown question type {k}"),
    };
    row.correct = Some(correct);
    row.score = if correct { 1.0 } else { 0.0 };
    Ok(())
}

// ---------- report ----------

#[derive(Default)]
struct Agg { n: usize, correct: usize, score: f64, conf: f64 }

/// Expected calibration error over 5 equal-width bins, and Brier score.
pub fn calibration(pairs: &[(f64, bool)]) -> (f64, f64) {
    if pairs.is_empty() { return (0.0, 0.0); }
    let n = pairs.len() as f64;
    let brier = pairs.iter().map(|(c, y)| (c - if *y { 1.0 } else { 0.0 }).powi(2)).sum::<f64>() / n;
    let mut ece = 0.0;
    for b in 0..5 {
        let (lo, hi) = (b as f64 / 5.0, (b + 1) as f64 / 5.0);
        let bin: Vec<&(f64, bool)> = pairs.iter().filter(|(c, _)| *c >= lo && (*c < hi || (b == 4 && *c <= 1.0))).collect();
        if bin.is_empty() { continue; }
        let k = bin.len() as f64;
        let acc = bin.iter().filter(|(_, y)| *y).count() as f64 / k;
        let conf = bin.iter().map(|(c, _)| c).sum::<f64>() / k;
        ece += k / n * (acc - conf).abs();
    }
    (ece, brier)
}

/// 95% Wilson score interval for k successes out of n (better than ±1.96·SE at small n).
pub fn wilson(k: usize, n: usize) -> (f64, f64) {
    if n == 0 { return (0.0, 1.0); }
    let (z, n, p) = (1.96f64, n as f64, k as f64 / n as f64);
    let d = 1.0 + z * z / n;
    let c = (p + z * z / (2.0 * n)) / d;
    let h = z * (p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt() / d;
    ((c - h).max(0.0), (c + h).min(1.0))
}

/// Exact two-sided McNemar p-value from discordant counts b and c (binomial, p = 0.5).
pub fn mcnemar(b: usize, c: usize) -> f64 {
    let n = b + c;
    if n == 0 { return 1.0; }
    let k = b.min(c);
    let mut cdf = 0.0;
    let mut term = 0.5f64.powi(n as i32); // C(n,0)/2^n
    for i in 0..=k {
        cdf += term;
        term *= (n - i) as f64 / (i + 1) as f64;
    }
    (2.0 * cdf).min(1.0)
}

fn load_rows(f: &Path) -> Result<Vec<Row>> {
    // Latest row per id wins, so a resumed run replaces its earlier errors.
    let mut latest: BTreeMap<String, Row> = BTreeMap::new();
    for r in std::fs::read_to_string(f)?.lines().filter_map(|l| serde_json::from_str::<Row>(l).ok()) { latest.insert(r.id.clone(), r); }
    Ok(latest.into_values().collect())
}

pub fn report(files: &[PathBuf]) -> Result<()> {
    for f in files {
        let rows = load_rows(f)?;
        if rows.is_empty() { continue; }
        println!("\n{}  ({} questions)", f.file_name().unwrap_or_default().to_string_lossy(), rows.len());
        let mut by: BTreeMap<String, Agg> = BTreeMap::new();
        for r in rows.iter().filter(|r| r.correct.is_some()) {
            let a = by.entry(r.kind.clone()).or_default();
            a.n += 1;
            a.correct += (r.correct == Some(true)) as usize;
            a.score += r.score;
            a.conf += r.confidence;
        }
        println!("  {:<13} {:>3}  {:>8}  {:>6}  {:>9}", "type", "n", "accuracy", "score", "mean conf");
        for (k, a) in &by {
            println!("  {:<13} {:>3}  {:>7.0}%  {:>6.2}  {:>9.2}", k, a.n, 100.0 * a.correct as f64 / a.n as f64, a.score / a.n as f64, a.conf / a.n as f64);
        }
        let graded: Vec<&Row> = rows.iter().filter(|r| r.correct.is_some() && r.kind != "rule_trigger").collect();
        let pairs: Vec<(f64, bool)> = graded.iter().map(|r| (r.confidence, r.correct == Some(true))).collect();
        let (ece, brier) = calibration(&pairs);
        let acc = pairs.iter().filter(|p| p.1).count() as f64 / pairs.len().max(1) as f64;
        let cites: usize = rows.iter().map(|r| r.citations.len()).sum();
        let dead: usize = rows.iter().map(|r| r.dead_citations).sum();
        let errs = rows.iter().filter(|r| !r.error.is_empty()).count();
        let secs = rows.iter().map(|r| r.secs).sum::<f64>() / rows.len() as f64;
        let toks: u64 = rows.iter().map(|r| r.tokens).sum();
        let k = pairs.iter().filter(|p| p.1).count();
        let (lo, hi) = wilson(k, pairs.len());
        // Selective accuracy: does confidence rank answers usefully? Accuracy on the most-confident half.
        let mut ranked = pairs.clone();
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
        let top = &ranked[..ranked.len().div_ceil(2)];
        let top_acc = top.iter().filter(|p| p.1).count() as f64 / top.len().max(1) as f64;
        println!("  overall accuracy {:.0}% (95% CI {:.0}–{:.0}%) · most-confident half {:.0}% · calibration error {:.2} · Brier {:.3}", acc * 100.0, lo * 100.0, hi * 100.0, top_acc * 100.0, ece, brier);
        println!("  dead citations {dead}/{cites} · errors {errs} · mean {secs:.0}s/question · {toks} tokens");
    }
    // Paired comparison of the first file against each other one, on questions both graded.
    if files.len() >= 2 {
        let a = load_rows(&files[0])?;
        for f in &files[1..] {
            let b: BTreeMap<String, Row> = load_rows(f)?.into_iter().map(|r| (r.id.clone(), r)).collect();
            let (mut only_a, mut only_b, mut both, mut n) = (0, 0, 0, 0);
            for ra in a.iter().filter(|r| r.kind != "rule_trigger") {
                let (Some(ca), Some(cb)) = (ra.correct, b.get(&ra.id).and_then(|r| r.correct)) else { continue };
                n += 1;
                match (ca, cb) { (true, false) => only_a += 1, (false, true) => only_b += 1, (true, true) => both += 1, _ => {} }
            }
            println!("\npaired: {} vs {} on {n} shared questions", files[0].file_name().unwrap_or_default().to_string_lossy(), f.file_name().unwrap_or_default().to_string_lossy());
            println!("  both right {both} · only first right {only_a} · only second right {only_b} · exact McNemar p = {:.3}{}", mcnemar(only_a, only_b),
                if only_a + only_b < 6 { "  (too few disagreements to tell them apart)" } else { "" });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_fractions_and_late_numbers() {
        let has = |s: &str, k: f64| answer_numbers(s).iter().any(|v| (v - k).abs() < 1e-3);
        assert!(has("13/18", 0.7222) && has("The probability is 1/3.", 0.3333));
        assert!(has("P = 1 - ...\n\nabout **0.5073** (≈50.7%)", 0.5073));
    }

    #[test]
    fn error_bars_and_paired_test() {
        // Wilson 95% for 95/100 ≈ 88.8–97.8% (published worked example).
        let (lo, hi) = wilson(95, 100);
        assert!((lo - 0.888).abs() < 0.002 && (hi - 0.978).abs() < 0.002, "{lo} {hi}");
        // McNemar exact: 0 vs 5 discordant → p = 2/32; 3 vs 3 → p = 1.
        assert!((mcnemar(0, 5) - 0.0625).abs() < 1e-12);
        assert!((mcnemar(3, 3) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn calibration_metrics() {
        // Perfectly calibrated within one bin: 0.9 confidence, 9 of 10 right.
        let mut p = vec![(0.9, true); 9];
        p.push((0.9, false));
        let (ece, brier) = calibration(&p);
        assert!(ece.abs() < 1e-9, "{ece}");
        assert!((brier - (9.0 * 0.01 + 0.81) / 10.0).abs() < 1e-9);
        // Overconfident: says 0.9, right half the time.
        let (ece, _) = calibration(&[(0.9, true), (0.9, false)]);
        assert!((ece - 0.4).abs() < 1e-9);
    }
}
