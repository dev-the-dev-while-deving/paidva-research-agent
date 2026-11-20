//! Everything the terminal sees: colours, live progress, the answer view and the trace view.
//! Answers go to stdout; progress and notes go to stderr, so `paidva ask … > out.txt` stays clean.
use crate::agent::Report;
use crate::answer::{Answer, Evidence, Kind, Tag};
use console::{Style, Term, measure_text_width, style};
use indicatif::{ProgressBar, ProgressStyle};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const AMBER: u8 = 215;
const OK: u8 = 114;
const BAD: u8 = 203;
const LABEL_W: usize = 11;

/// Colour per `ui.color` (auto | always | never). NO_COLOR always wins.
pub fn init() {
    let cfg = crate::config::get();
    let mode = cfg.ui.color.as_str();
    let on = match mode {
        _ if std::env::var_os("NO_COLOR").is_some() => false,
        "always" => true,
        "never" => false,
        _ => return, // auto: console already checks for a terminal
    };
    console::set_colors_enabled(on);
    console::set_colors_enabled_stderr(on);
}

pub fn accent<D>(d: D) -> console::StyledObject<D> { style(d).color256(AMBER) }
pub fn good<D>(d: D) -> console::StyledObject<D> { style(d).color256(OK) }
pub fn bad<D>(d: D) -> console::StyledObject<D> { style(d).color256(BAD) }
pub fn dim<D>(d: D) -> console::StyledObject<D> { style(d).dim() }

pub fn tag_name(t: Tag) -> &'static str {
    match t { Tag::Observed => "observed", Tag::Sourced => "sourced", Tag::Inferred => "inferred", Tag::Knowledge => "knowledge", Tag::Speculation => "speculation" }
}

/// Coloured chip, same colours as the README art.
pub fn chip(t: Tag) -> String {
    let bg = match t { Tag::Observed => 116, Tag::Sourced => 150, Tag::Inferred => 183, Tag::Knowledge => 222, Tag::Speculation => 250 };
    let s = Style::new().on_color256(bg).color256(16);
    if console::colors_enabled() { s.apply_to(format!(" {:<11} ", tag_name(t))).to_string() } else { format!("[{:<11}]", tag_name(t)) }
}

/// Usable text width: the terminal width, capped at 100 for readability, 80 when not a terminal.
pub fn width() -> usize {
    Term::stdout().size_checked().map(|(_, w)| w as usize).unwrap_or(80).clamp(40, 100)
}

/// Word-wrap `text` to `width`, indenting every line after the first by `indent` spaces.
// ponytail: wraps by display width of whole words; a single word longer than the line is left to overflow.
pub fn wrap(text: &str, width: usize, indent: usize) -> String {
    let mut out = String::new();
    for (pi, para) in text.split('\n').enumerate() {
        if pi > 0 { out.push('\n'); out.push_str(&" ".repeat(indent)); }
        let mut col = indent;
        for (i, word) in para.split_whitespace().enumerate() {
            let w = measure_text_width(word);
            if i > 0 && col + 1 + w > width { out.push('\n'); out.push_str(&" ".repeat(indent)); col = indent; }
            else if i > 0 { out.push(' '); col += 1; }
            out.push_str(word);
            col += w;
        }
    }
    out
}

fn label(s: &str) -> String { format!("  {}", style(format!("{s:<LABEL_W$}")).dim().bold()) }

// ───────────────────────── live progress (stderr) ─────────────────────────

static BUSY: Mutex<Option<(ProgressBar, Instant)>> = Mutex::new(None);

/// Start the spinner for one question. Steps print above it as they finish.
pub fn busy_start(msg: &str) {
    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::with_template("  {spinner:.214} {msg} {elapsed:.dim}").unwrap()
        .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", "·"]));
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(Duration::from_millis(80));
    *BUSY.lock().unwrap() = Some((pb, Instant::now()));
}

pub fn busy_stop() {
    if let Some((pb, _)) = BUSY.lock().unwrap().take() { pb.finish_and_clear(); }
}

fn emit(line: String) {
    match BUSY.lock().unwrap().as_ref() {
        Some((pb, _)) => pb.println(line),
        None => eprintln!("{line}"),
    }
}

/// Progress for the full-screen UI. While `SINK` holds a sender, steps and notes go there instead of the spinner.
pub enum Event { Step(String, String), Note(String) }
pub static SINK: Mutex<Option<std::sync::mpsc::Sender<Event>>> = Mutex::new(None);

/// True when the event went to the full-screen UI.
fn sent(e: impl FnOnce() -> Event) -> bool {
    SINK.lock().unwrap().as_ref().map(|tx| tx.send(e()).is_ok()).unwrap_or(false)
}

/// One finished pipeline step, e.g. "  ◆ python      compute ppv            3.1s".
pub fn step(name: &str, detail: &str) {
    if sent(|| Event::Step(name.into(), detail.into())) { return; }
    let secs = BUSY.lock().unwrap().as_ref().map(|(pb, t)| { pb.set_message(format!("{name} done · working")); t.elapsed().as_secs_f64() });
    if !crate::config::get().ui.show_steps { return; }
    let detail: String = detail.chars().take(width().saturating_sub(LABEL_W + 16)).collect();
    let time = secs.map(|s| format!("  {}", dim(format!("{s:.1}s")))).unwrap_or_default();
    emit(format!("  {} {} {}{time}", accent("◆"), style(format!("{name:<LABEL_W$}")).bold(), dim(detail)));
}

/// A side note (rate-limit waits and the like).
pub fn note(msg: &str) { if sent(|| Event::Note(msg.into())) { return; } emit(format!("  {} {}", dim("…"), dim(msg))); }

// ───────────────────────── answer view (stdout) ─────────────────────────

pub struct Stats { pub calls: usize, pub tokens: u64, pub secs: f64 }

fn meter(conf: f64) -> String {
    let cells = 24;
    let filled = (conf.clamp(0.0, 1.0) * cells as f64).round() as usize;
    let col = if conf < 0.45 { BAD } else if conf < 0.7 { AMBER } else { OK };
    if !console::colors_enabled() { return format!("{}{}", "█".repeat(filled), "░".repeat(cells - filled)); }
    format!("{}{}", style("━".repeat(filled)).color256(col), dim("━".repeat(cells - filled)))
}

/// ▁▃▅▇ for a belief trail.
fn spark(trail: &[f64]) -> String {
    const B: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    trail.iter().map(|p| B[((p.clamp(0.0, 1.0) * 7.0).round()) as usize]).collect()
}

/// The answer view, wrapped to `w` columns (`r.errors` are left to the caller).
pub fn answer(a: &Answer, ev: &[Evidence], r: &Report, st: &Stats, w: usize) -> String {
    let ind = 2 + LABEL_W;
    let mut o = String::from("\n");
    o += &format!("  {} {}\n", accent("▌"), style(wrap(&a.direct_answer, w, 4)).bold());

    if !a.claims.is_empty() {
        o += &format!("\n{}\n", label("CLAIMS"));
        for c in &a.claims {
            let ids = if c.evidence.is_empty() { String::new() } else { format!("  {}", dim(c.evidence.join(" "))) };
            o += &format!("  {} {}{ids}\n", chip(c.tag), wrap(&c.text, w, 16));
            if !c.quote.is_empty() {
                o += &format!("{}{}\n", " ".repeat(16), style(wrap(&format!("“{}”", c.quote.trim()), w, 16)).dim().italic());
            }
        }
    }
    for (b, (hyp, trail)) in a.bayes.iter().zip(&r.bayes) {
        o += &format!("\n{}{}\n", label("BELIEF"), wrap(hyp, w, ind));
        let path: Vec<String> = trail.iter().map(|p| format!("{p:.2}")).collect();
        o += &format!("{}{}  {}\n", " ".repeat(ind), accent(spark(trail)), path.join(&dim(" → ").to_string()));
        o += &format!("{}{}\n", " ".repeat(ind), dim(wrap(&format!("prior: {}", b.prior_reason), w, ind)));
        for (s, p) in b.steps.iter().zip(&trail[1..]) {
            o += &format!("{}{}\n", " ".repeat(ind), dim(wrap(&format!("×{:.2} {} → {p:.2}  ({})", s.likelihood_ratio.clamp(0.05, 20.0), s.evidence, s.reason), w, ind + 2)));
        }
    }
    for p in &r.pooled {
        o += &format!("\n{}{} {:.3} {} I² {:.0}% · {} studies, random effects\n", label("POOLED"), p.measure, p.estimate,
            dim(format!("[95% CI {:.3}–{:.3}]", p.ci_low, p.ci_high)), p.i2 * 100.0, p.k);
    }
    if !r.cross_checks.is_empty() {
        o += &format!("\n{}", label("CHECKS"));
        for (i, c) in r.cross_checks.iter().enumerate() {
            let mark = if c.agree { good("✓") } else { bad("✗") };
            let pad = if i == 0 { String::new() } else { " ".repeat(ind) };
            o += &format!("{pad}{mark} {}  {}\n", c.name, dim(format!("exact {:.6} · simulated {:.6} ± {:.6}", c.exact, c.sim, c.se)));
        }
    }
    if !a.unknowns.is_empty() {
        o += &format!("\n{}", label("UNSURE"));
        for (i, u) in a.unknowns.iter().enumerate() {
            o += &format!("{}· {}\n", if i == 0 { String::new() } else { " ".repeat(ind) }, wrap(u, w, ind + 2));
        }
    }
    let srcs = cited_sources(a, ev);
    if !srcs.is_empty() {
        o += &format!("\n{}", label("SOURCES"));
        for (i, (url, title, ids)) in srcs.iter().enumerate() {
            o += &format!("{}{} {}\n", if i == 0 { String::new() } else { " ".repeat(ind) }, accent(format!("[{}]", ids.join(","))), wrap(title, w, ind));
            o += &format!("{}{}\n", " ".repeat(ind), style(url).dim().underlined());
        }
    }

    o += &format!("\n  {}  {} {}  {}\n", meter(r.confidence), style(format!("{:.2}", r.confidence)).bold(), "confidence", dim("· uncalibrated"));
    let mut facts = vec![format!("{} fix round{}", r.fix_rounds, if r.fix_rounds == 1 { "" } else { "s" })];
    if r.relabelled > 0 { facts.push(format!("{} relabelled", r.relabelled)); }
    if r.dropped > 0 { facts.push(format!("{} dropped", r.dropped)); }
    facts.push(format!("{} model calls", st.calls));
    facts.push(format!("{:.1}k tokens", st.tokens as f64 / 1000.0));
    facts.push(format!("{:.1}s", st.secs));
    o += &format!("  {}\n", dim(facts.join(" · ")));
    if r.confidence < 0.45 { o += &format!("  {} {}\n", accent("⚠"), accent("low confidence: treat this as a lead, not an answer")); }
    o
}

/// Sources a claim or belief step actually cites, one entry per URL: (url, title, ids).
/// (url, title, evidence ids)
pub type Cited<'a> = (&'a str, &'a str, Vec<&'a str>);

pub fn cited_sources<'a>(a: &Answer, ev: &'a [Evidence]) -> Vec<Cited<'a>> {
    let cited: std::collections::HashSet<&str> = a.claims.iter().flat_map(|c| c.evidence.iter())
        .chain(a.bayes.iter().flat_map(|b| b.steps.iter().map(|s| &s.evidence))).map(String::as_str).collect();
    let mut by_url: Vec<Cited> = vec![];
    for e in ev.iter().filter(|e| !e.url.is_empty() && cited.contains(e.id.as_str())) {
        match by_url.iter_mut().find(|x| x.0 == e.url) {
            Some(x) => x.2.push(&e.id),
            None => by_url.push((&e.url, &e.title, vec![&e.id])),
        }
    }
    by_url
}

/// Every source read (cited or not), with the passage text.
pub fn sources(ev: &[Evidence], w: usize) -> String {
    let src: Vec<&Evidence> = ev.iter().filter(|e| e.kind == Kind::Source).collect();
    if src.is_empty() { return format!("  {}\n", dim("no sources were read for the last answer")); }
    let mut o = String::new();
    for e in src {
        o += &format!("\n  {} {}{}\n", accent(format!("[{}]", e.id)), style(wrap(&e.title, w, 3 + e.id.len() + 3)).bold(), if e.partial { dim("  (abstract only)").to_string() } else { String::new() });
        if !e.url.is_empty() { o += &format!("      {}\n", style(&e.url).dim().underlined()); }
        o += &format!("      {}\n", dim(wrap(&e.text.chars().take(500).collect::<String>(), w, 6)));
    }
    o
}

/// The Python behind every computation, with its output.
pub fn code(ev: &[Evidence], w: usize) -> String {
    let comps: Vec<&Evidence> = ev.iter().filter(|e| e.kind == Kind::Computation).collect();
    if comps.is_empty() { return format!("  {}\n", dim("no code ran for the last answer")); }
    let mut o = String::new();
    for e in comps {
        o += &format!("\n  {} {}\n", accent(format!("[{}]", e.id)), style(wrap(&e.title, w, 3 + e.id.len() + 3)).bold());
        for l in e.code.lines() { o += &format!("  {} {}\n", dim("│"), l); }
        o += &format!("  {} {}\n", dim("└─ output"), dim(e.text.lines().take(12).collect::<Vec<_>>().join(" ⏎ ")));
    }
    o
}

// ───────────────────────── trace view (/why) ─────────────────────────

pub fn trace() -> anyhow::Result<()> {
    let p = crate::config::dir().join("traces/trace.jsonl");
    let text = std::fs::read_to_string(&p).map_err(|_| anyhow::anyhow!("no trace yet: ask something first"))?;
    let v: serde_json::Value = serde_json::from_str(text.lines().last().unwrap_or("{}"))?;
    let r = &v["report"];
    let w = width();
    let list = |x: &serde_json::Value| x.as_array().map(|a| a.iter().filter_map(|s| s.as_str()).collect::<Vec<_>>().join(", ")).filter(|s| !s.is_empty()).unwrap_or_else(|| "none".into());
    println!("\n  {} {}", accent("▌"), style(wrap(v["question"].as_str().unwrap_or(""), w, 4)).bold());
    println!("\n{}{} {}", label("MODE"), r["mode"].as_str().unwrap_or("?"), if r["hard"].as_bool() == Some(true) { "· hard" } else { "" });
    println!("{}{}", label("RULES"), wrap(&list(&r["rules"]), w, 2 + LABEL_W));
    println!("{}{}", label("SKILLS"), list(&r["skills"]));
    let plan = &r["plan"];
    let steps: Vec<String> = plan["searches"].as_array().into_iter().flatten()
        .map(|s| format!("search {}: {}", s["source"].as_str().unwrap_or("?").to_lowercase(), s["query"].as_str().unwrap_or("")))
        .chain(plan["computations"].as_array().into_iter().flatten().filter_map(|c| c.as_str()).map(|c| format!("compute: {c}")))
        .collect();
    if !steps.is_empty() {
        print!("{}", label("PLAN"));
        for (i, line) in steps.iter().enumerate() {
            println!("{}{} {}", if i == 0 { String::new() } else { " ".repeat(2 + LABEL_W) }, dim(format!("{}.", i + 1)), wrap(line, w, 2 + LABEL_W + 3));
        }
    }
    let issues = r["first_draft_issues"].as_array().cloned().unwrap_or_default();
    if !issues.is_empty() {
        print!("{}", label("CAUGHT"));
        for (i, s) in issues.iter().enumerate() {
            println!("{}{} {}", if i == 0 { String::new() } else { " ".repeat(2 + LABEL_W) }, bad("✗"), wrap(s.as_str().unwrap_or(""), w, 2 + LABEL_W + 2));
        }
    }
    let ev: Vec<Evidence> = serde_json::from_value(v["evidence"].clone()).unwrap_or_default();
    if !ev.is_empty() { println!("\n{}{} item{}", label("EVIDENCE"), ev.len(), if ev.len() == 1 { "" } else { "s" }); print!("{}{}", sources(&ev, w), code(&ev, w)); }
    let calls = v["calls"].as_array().cloned().unwrap_or_default();
    let tokens: u64 = calls.iter().filter_map(|c| c["tokens"].as_u64()).sum();
    let cached = calls.iter().filter(|c| c["cached"].as_bool() == Some(true)).count();
    println!("\n  {}", dim(format!("{} model calls ({cached} cached) · {:.1}k tokens · confidence {:.2}", calls.len(), tokens as f64 / 1000.0, r["confidence"].as_f64().unwrap_or(0.0))));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_breaks_on_words_and_indents() {
        assert_eq!(wrap("aaa bbb ccc", 9, 2), "aaa bbb\n  ccc");
        assert_eq!(wrap("one\ntwo", 80, 3), "one\n   two");
        assert_eq!(spark(&[0.0, 0.5, 1.0]), "▁▅█");
    }
}
