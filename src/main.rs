mod agent;
mod answer;
mod config;
mod eval;
mod llm;
mod papers;
mod sandbox;
mod search;
mod setup;
mod stats;
mod ui;
mod verify;

use agent::{Agent, Outcome, Report};
use answer::{Answer, Evidence};
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Instant;
use ui::{accent, bad, dim, good};

#[derive(Parser)]
#[command(name = "paidva", version, about = "Paidva: the research agent that shows its work.\n\nRun with no command to chat (first run starts the setup).")]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Guided setup: API keys (checked live), models, Python sandbox.
    Setup,
    /// Change keys, models, colours, sandbox and more.
    Settings,
    /// Check that keys, models, Python and the sandbox all work.
    Doctor {
        /// Skip the live key checks (no network).
        #[arg(long)]
        offline: bool,
    },
    /// Install the default prompts, rules and skills into ~/.agent (existing files are kept unless --force).
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
    /// Interactive chat (the default). Type /help inside for commands.
    Chat {
        #[arg(long = "file")]
        files: Vec<PathBuf>,
    },
    /// Show the last trace: plan, evidence, code, what verification caught.
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

type Last = (Answer, Vec<Evidence>, Report);

/// Run one question with the live spinner, print the answer, and return it.
async fn ask(agent: &Agent, q: &str, files: &[PathBuf], history: &[(String, String)]) -> Result<Option<Last>> {
    let t = Instant::now();
    ui::busy_start("thinking");
    let res = agent.handle(q, files, history).await;
    ui::busy_stop();
    match res? {
        Outcome::Ask(c) => { println!("\n  {} {}", accent("?"), ui::wrap(&c, ui::width(), 4)); Ok(None) }
        Outcome::Answered { answer, evidence, report } => {
            let calls = agent.llm.calls.lock().unwrap();
            let st = ui::Stats { calls: calls.len(), tokens: calls.iter().map(|c| c.tokens).sum(), secs: t.elapsed().as_secs_f64() };
            drop(calls);
            ui::answer(&answer, &evidence, &report, &st);
            Ok(Some((answer, evidence, report)))
        }
    }
}

fn banner(files: &[PathBuf]) {
    let c = config::get();
    let short = |v: &[String]| v.first().map(|m| m.rsplit('/').next().unwrap_or(m).trim_end_matches(":free").to_string()).unwrap_or_default();
    println!();
    println!("  {}  {}", accent(console::style("◆ Paidva").bold()), dim(format!("v{} · the research agent that shows its work", env!("CARGO_PKG_VERSION"))));
    println!("    {}", dim(format!("main {} · fast {} · sandbox {}s", short(&c.models.main), short(&c.models.fast), c.sandbox.timeout_secs)));
    if !files.is_empty() { println!("    {}", dim(format!("attached: {}", names(files)))); }
    println!("    {}", dim("Ask anything. /help for commands · ↑ for history · Ctrl-D to quit"));
}

fn names(files: &[PathBuf]) -> String {
    files.iter().map(|f| f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()).collect::<Vec<_>>().join(", ")
}

fn help() {
    let rows = [
        ("/why", "how the last answer was built: plan, evidence, code, what verification caught"),
        ("/sources", "every source read, with the passage"),
        ("/code", "the Python behind every number"),
        ("/json", "the last answer as JSON"),
        ("/file <path>", "attach a data file (CSV etc.) for the analyst; /file alone lists them"),
        ("/drop", "detach all files"),
        ("/clear", "clear the screen and forget the conversation"),
        ("/settings", "keys, models, colours, sandbox"),
        ("/doctor", "health check"),
        ("/quit", "leave (or Ctrl-D)"),
    ];
    println!();
    for (c, d) in rows { println!("  {}  {}", accent(format!("{c:<14}")), dim(d)); }
}

async fn chat(mut files: Vec<PathBuf>) -> Result<()> {
    banner(&files);
    let agent = Agent::new()?;
    let mut rl = rustyline::DefaultEditor::new()?;
    let hist = config::dir().join("history.txt");
    let _ = rl.load_history(&hist);
    let mut history: Vec<(String, String)> = vec![];
    let mut last: Option<Last> = None;
    let prompt = format!("\n{} ", accent("›"));
    loop {
        let line = match rl.readline(&prompt) {
            Ok(l) => l,
            Err(rustyline::error::ReadlineError::Interrupted) => continue,
            Err(rustyline::error::ReadlineError::Eof) => break,
            Err(e) => return Err(e.into()),
        };
        let q = line.trim();
        if q.is_empty() { continue; }
        let _ = rl.add_history_entry(q);
        let _ = rl.save_history(&hist);
        let (cmd, arg) = q.split_once(' ').map(|(a, b)| (a, b.trim())).unwrap_or((q, ""));
        let none = || println!("  {}", dim("nothing yet: ask a question first"));
        match cmd {
            "/quit" | "/exit" | "/q" => break,
            "/help" | "/?" => help(),
            "/why" => ui::trace().unwrap_or_else(|e| println!("  {}", dim(e))),
            "/sources" => match &last { Some((_, ev, _)) => ui::sources(ev), None => none() },
            "/code" => match &last { Some((_, ev, _)) => ui::code(ev), None => none() },
            "/json" => match &last {
                Some((a, ev, r)) => println!("{}", serde_json::to_string_pretty(&serde_json::json!({"answer": a, "evidence": ev, "report": r}))?),
                None => none(),
            },
            "/file" if arg.is_empty() => println!("  {}", dim(if files.is_empty() { "no files attached".into() } else { format!("attached: {}", names(&files)) })),
            "/file" => {
                let p = PathBuf::from(shellish(arg));
                if p.is_file() { println!("  {} attached {}", good("✓"), p.display()); files.push(p); }
                else { println!("  {} no such file: {}", bad("✗"), p.display()); }
            }
            "/drop" => { files.clear(); println!("  {} files detached", good("✓")); }
            "/clear" => { let _ = console::Term::stdout().clear_screen(); history.clear(); last = None; banner(&files); }
            "/settings" => { setup::settings().await?; banner(&files); }
            "/doctor" => { println!(); setup::doctor(true).await; }
            c if c.starts_with('/') => println!("  {} unknown command {c} · /help lists them", bad("✗")),
            _ => match ask(&agent, q, &files, &history).await {
                Ok(Some(l)) => { history.push((q.into(), l.0.direct_answer.clone())); last = Some(l); }
                Ok(None) => {}
                Err(e) => println!("\n  {} {}", bad("error"), ui::wrap(&format!("{e:#}"), ui::width(), 8)),
            },
        }
    }
    println!("  {}", dim("bye"));
    Ok(())
}

/// Expand a leading ~ and strip surrounding quotes from a pasted path.
fn shellish(p: &str) -> String {
    let p = p.trim_matches(|c| c == '"' || c == '\'');
    match p.strip_prefix("~/") { Some(rest) => format!("{}/{rest}", std::env::var("HOME").unwrap_or_default()), None => p.to_string() }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Exit quietly when piped into `head` and friends, instead of panicking on a closed pipe.
    // SAFETY: called before any other thread starts.
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
    config::load_env();
    ui::init();
    match Cli::parse().cmd {
        None => {
            if !setup::ready() { setup::onboard().await?; }
            if setup::ready() { chat(vec![]).await?; }
        }
        Some(Cmd::Setup) => setup::onboard().await?,
        Some(Cmd::Settings) => setup::settings().await?,
        Some(Cmd::Doctor { offline }) => { println!(); if !setup::doctor(!offline).await { std::process::exit(1); } }
        Some(Cmd::Init { force }) => { setup::install(force, false)?; }
        Some(Cmd::Trace) => ui::trace()?,
        Some(Cmd::Eval { set, system, model, kind, limit, fresh, ids }) => {
            let out = eval::run(eval::Opts { set, system, model, kind, limit, fresh, ids }).await?;
            eval::report(&[out])?;
        }
        Some(Cmd::Report { files }) => eval::report(&files)?,
        Some(Cmd::Ask { question, files, json }) => {
            let agent = Agent::new()?;
            if json {
                if let Outcome::Answered { answer, evidence, report } = agent.handle(&question, &files, &[]).await? {
                    println!("{}", serde_json::to_string_pretty(&serde_json::json!({"answer": answer, "evidence": evidence, "report": report}))?);
                }
            } else {
                ask(&agent, &question, &files, &[]).await?;
            }
        }
        Some(Cmd::Chat { files }) => chat(files).await?,
    }
    Ok(())
}
