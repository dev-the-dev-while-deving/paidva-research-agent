//! First-run onboarding, the settings screen and `paidva doctor`.
//! Keys are typed hidden, checked live against the provider (a free /models call), and stored in ~/.agent/.env (mode 600).
use crate::config::{self, Config, DEFAULT_FAST, DEFAULT_MAIN, KEYS};
use crate::ui::{accent, bad, dim, good};
use anyhow::Result;
use console::style;
use dialoguer::{Confirm, FuzzySelect, Input, Password, Select, theme::ColorfulTheme};
use std::path::PathBuf;

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
const PY_PACKAGES: &[&str] = &["numpy", "scipy", "pandas", "statsmodels", "sympy", "scikit-learn"];

fn theme() -> ColorfulTheme {
    ColorfulTheme { prompt_prefix: accent("◆".to_string()), success_prefix: good("✓".to_string()), ..ColorfulTheme::default() }
}

fn heading(title: &str, sub: &str) {
    println!("\n  {} {}", accent("▌"), style(title).bold());
    if !sub.is_empty() { println!("    {}", dim(sub)); }
    println!();
}

/// Write default prompts, rules and skills into ~/.agent. Returns (written, kept).
pub fn install(force: bool, quiet: bool) -> Result<(usize, usize)> {
    let dir = config::dir();
    let (mut wrote, mut kept) = (0, 0);
    for (rel, body) in DEFAULTS {
        let p = dir.join(rel);
        if p.exists() && !force { kept += 1; if !quiet { println!("  {} {}", dim("kept "), dim(p.display())); } continue; }
        std::fs::create_dir_all(p.parent().unwrap())?;
        std::fs::write(&p, body)?;
        wrote += 1;
        if !quiet { println!("  {} {}", good("wrote"), p.display()); }
    }
    Ok((wrote, kept))
}

/// At least one model provider key is set.
pub fn ready() -> bool { KEYS.iter().any(|(v, _, _, model)| *model && std::env::var(v).is_ok_and(|k| !k.is_empty())) }

fn python() -> PathBuf { crate::sandbox::python(config::dir().parent().unwrap()) }

/// Python sandbox env exists and its packages import.
fn python_ok() -> bool {
    std::process::Command::new(python()).args(["-c", "import numpy, scipy, pandas, statsmodels, sympy, sklearn"])
        .output().is_ok_and(|o| o.status.success())
}

fn has(cmd: &str) -> bool { std::process::Command::new("sh").args(["-c", &format!("command -v {cmd}")]).output().is_ok_and(|o| o.status.success()) }

/// Create ~/.agent/py with uv. Streams uv's own output so the user sees progress.
fn install_python() -> Result<bool> {
    if !has("uv") {
        println!("  {} uv is not installed. Install it with:\n    {}\n  then run {}", bad("✗"), accent("curl -LsSf https://astral.sh/uv/install.sh | sh"), accent("paidva setup"));
        return Ok(false);
    }
    let dir = config::dir();
    let venv = dir.join("py");
    let ok = std::process::Command::new("uv").args(["venv", "py", "--python", "3.13", "--allow-existing"]).current_dir(&dir).status()?.success()
        && std::process::Command::new("uv").arg("pip").arg("install").args(PY_PACKAGES).env("VIRTUAL_ENV", &venv).current_dir(&dir).status()?.success();
    Ok(ok && python_ok())
}

/// Live check of one key. Returns the provider's model list on success.
async fn check(var: &str, key: &str) -> Result<Vec<String>> {
    if var == "S2_API_KEY" {
        let r = reqwest::Client::new().get("https://api.semanticscholar.org/graph/v1/paper/search?query=bayes&limit=1")
            .header("x-api-key", key).timeout(std::time::Duration::from_secs(15)).send().await?;
        anyhow::ensure!(r.status().is_success(), "Semantic Scholar answered {}", r.status());
        return Ok(vec![]);
    }
    crate::llm::list_models(var, key).await
}

async fn check_with_spinner(var: &str, key: &str) -> Result<Vec<String>> {
    let pb = indicatif::ProgressBar::new_spinner();
    pb.set_message("checking the key with the provider…");
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    let r = check(var, key).await;
    pb.finish_and_clear();
    r
}

/// Ask for one key: keep / replace / remove an existing one, or paste a new one (Enter skips). Returns true if a key is set after.
async fn key_flow(var: &str, name: &str, url: &str, model: bool) -> Result<bool> {
    let t = theme();
    let why = if model { "free key" } else { "optional, lifts paper-search limits · free key" };
    println!("  {}  {}", style(name).bold(), dim(format!("{why}: {url}")));
    let current = std::env::var(var).ok().filter(|k| !k.is_empty());
    if let Some(k) = &current {
        let keep = format!("Keep {}", config::mask(k));
        match Select::with_theme(&t).with_prompt(name).items(&[keep.as_str(), "Replace it", "Remove it"]).default(0).interact()? {
            0 => return Ok(true),
            2 => {
                config::set_key(var, None)?;
                println!("  {} removed from ~/.agent/.env {}\n", dim("–"), dim("(if it is also in a project .env or your shell profile, remove it there too)"));
                return Ok(false);
            }
            _ => {}
        }
    }
    loop {
        let key = Password::with_theme(&t).with_prompt(format!("{name} key (hidden; Enter to skip)")).allow_empty_password(true).report(false).interact()?;
        let key = key.trim().to_string();
        if key.is_empty() { println!("  {} skipped\n", dim("–")); return Ok(current.is_some()); }
        match check_with_spinner(var, &key).await {
            Ok(models) => {
                config::set_key(var, Some(&key))?;
                let extra = if models.is_empty() { String::new() } else { format!(" · {} models available", models.len()) };
                println!("  {} {} saved{extra}\n", good("✓"), config::mask(&key));
                return Ok(true);
            }
            Err(e) => {
                println!("  {} {e:#}", bad("✗"));
                match Select::with_theme(&t).items(&["Try again", "Save it anyway (offline?)", "Skip"]).default(0).interact()? {
                    0 => continue,
                    1 => { config::set_key(var, Some(&key))?; println!("  {} saved without a check\n", accent("!")); return Ok(true); }
                    _ => return Ok(current.is_some()),
                }
            }
        }
    }
}

async fn keys_screen() -> Result<()> {
    heading("API keys", "All free tiers. Stored only on this machine, in ~/.agent/.env (readable by you alone).");
    for (var, name, url, model) in KEYS { key_flow(var, name, url, *model).await?; }
    if !ready() { println!("  {} Paidva needs at least one model key (Groq is the easiest).", accent("!")); }
    Ok(())
}

/// Models the configured providers offer right now (one free /models call each).
async fn live_models() -> Vec<String> {
    let mut all = vec![];
    for (var, _, _, model) in KEYS {
        let Some(k) = std::env::var(var).ok().filter(|k| *model && !k.is_empty()) else { continue };
        if let Ok(m) = crate::llm::list_models(var, &k).await { all.extend(m); }
    }
    all
}

fn key_for(spec: &str) -> bool { std::env::var(crate::llm::route(spec).1).is_ok_and(|k| !k.is_empty()) }

/// The default chain, minus models whose provider has no key (all of it if that leaves nothing).
pub fn recommended(defaults: &[&str]) -> Vec<String> {
    let usable: Vec<String> = defaults.iter().filter(|m| key_for(m)).map(|m| m.to_string()).collect();
    if usable.is_empty() { defaults.iter().map(|m| m.to_string()).collect() } else { usable }
}

fn chain_line(chain: &[String]) -> String {
    let short = |m: &String| m.rsplit('/').next().unwrap_or(m).trim_end_matches(":free").to_string();
    let head: Vec<String> = chain.iter().take(2).map(short).collect();
    let more = if chain.len() > 2 { format!(" (+{})", chain.len() - 2) } else { String::new() };
    format!("{}{more}", head.join(" → "))
}

fn pick_model(prompt: &str, live: &[String]) -> Result<Option<String>> {
    let t = theme();
    let mut items: Vec<String> = live.to_vec();
    items.push("Type a model id…".into());
    let i = FuzzySelect::with_theme(&t).with_prompt(format!("{prompt} (type to search)")).items(&items).default(0).max_length(12).interact_opt()?;
    Ok(match i {
        None => None,
        Some(i) if i == items.len() - 1 => {
            let s: String = Input::with_theme(&t).with_prompt("Model id (plain for Groq, gemini:<id>, openrouter:<id>)").interact_text()?;
            Some(s.trim().to_string()).filter(|s| !s.is_empty())
        }
        Some(i) => Some(items[i].clone()),
    })
}

async fn chain_screen(name: &str, what: &str, chain: &mut Vec<String>, defaults: &[&str], live: &mut Option<Vec<String>>) -> Result<()> {
    let t = theme();
    loop {
        heading(&format!("{name} models"), &format!("{what}. Tried in order: if one is rate-limited or missing a key, the next one answers."));
        for (i, m) in chain.iter().enumerate() {
            let k = if key_for(m) { good("✓").to_string() } else { dim("no key").to_string() };
            println!("    {} {m}  {k}", dim(format!("{}.", i + 1)));
        }
        println!();
        let c = Select::with_theme(&t).with_prompt("Change")
            .items(&["Set the first choice…", "Add a fallback…", "Remove one…", "Use the recommended chain", "Back"]).default(0).interact()?;
        if c == 4 { return Ok(()); }
        if c == 3 { *chain = recommended(defaults); continue; }
        if c == 2 {
            if chain.len() <= 1 { println!("  {} keep at least one model", accent("!")); continue; }
            if let Some(i) = Select::with_theme(&t).with_prompt("Remove").items(chain.as_slice()).interact_opt()? { chain.remove(i); }
            continue;
        }
        if live.is_none() {
            let pb = indicatif::ProgressBar::new_spinner();
            pb.set_message("fetching the models your keys can use…");
            pb.enable_steady_tick(std::time::Duration::from_millis(80));
            *live = Some(live_models().await);
            pb.finish_and_clear();
        }
        let Some(m) = pick_model(if c == 0 { "First choice" } else { "Fallback" }, live.as_deref().unwrap_or_default())? else { continue };
        chain.retain(|x| x != &m);
        if c == 0 { chain.insert(0, m) } else { chain.push(m) }
    }
}

fn python_line() -> String { if python_ok() { good("ready").to_string() } else { bad("not installed").to_string() } }

fn keys_line() -> String {
    KEYS.iter().map(|(v, name, _, _)| {
        let n = name.split_whitespace().next().unwrap_or(name).to_lowercase();
        if std::env::var(v).is_ok_and(|k| !k.is_empty()) { format!("{n} {}", good("✓")) } else { format!("{n} {}", dim("–")) }
    }).collect::<Vec<_>>().join("  ")
}

/// The settings screen. Every change is saved as soon as it's made.
pub async fn settings() -> Result<()> {
    let t = theme();
    let mut live: Option<Vec<String>> = None;
    loop {
        let mut c = config::get();
        heading("Settings", &format!("saved to {}", config::Config::path().display()));
        let rows = [
            format!("{:<18}{}", "API keys", keys_line()),
            format!("{:<18}{}", "Main models", chain_line(&c.models.main)),
            format!("{:<18}{}", "Fast models", chain_line(&c.models.fast)),
            format!("{:<18}{}", "Colours", c.ui.color),
            format!("{:<18}{}", "Show live steps", if c.ui.show_steps { "on" } else { "off" }),
            format!("{:<18}{}s", "Sandbox timeout", c.sandbox.timeout_secs),
            format!("{:<18}{}", "Python sandbox", python_line()),
            format!("{:<18}{}", "Prompts & rules", dim("reinstall the defaults (your edits are overwritten)")),
            format!("{:<18}{}", "Reset settings", dim("models, colours and timeout back to defaults")),
            "Done".to_string(),
        ];
        let Some(i) = Select::with_theme(&t).with_prompt("Pick a setting (Esc to leave)").items(&rows).default(0).interact_opt()? else { return Ok(()) };
        match i {
            0 => keys_screen().await?,
            1 => { chain_screen("Main", "Plans, runs the analysis and writes the answer", &mut c.models.main, DEFAULT_MAIN, &mut live).await?; config::set(c)? }
            2 => { chain_screen("Fast", "Routes questions and checks claims", &mut c.models.fast, DEFAULT_FAST, &mut live).await?; config::set(c)? }
            3 => {
                let opts = ["auto", "always", "never"];
                let cur = opts.iter().position(|o| *o == c.ui.color).unwrap_or(0);
                c.ui.color = opts[Select::with_theme(&t).with_prompt("Colours").items(&["auto (when it's a terminal)", "always", "never"]).default(cur).interact()?].into();
                config::set(c)?;
                crate::ui::init();
            }
            4 => { c.ui.show_steps = !c.ui.show_steps; config::set(c)? }
            5 => {
                let v: u64 = Input::with_theme(&t).with_prompt("Seconds a Python run may take (5–600)").default(c.sandbox.timeout_secs)
                    .validate_with(|v: &u64| if (5..=600).contains(v) { Ok(()) } else { Err("between 5 and 600") }).interact_text()?;
                c.sandbox.timeout_secs = v;
                config::set(c)?;
            }
            6 => {
                if python_ok() && !Confirm::with_theme(&t).with_prompt("The sandbox works. Reinstall it anyway?").default(false).interact()? { continue; }
                let ok = install_python()?;
                println!("  {} {}", if ok { good("✓") } else { bad("✗") }, if ok { "Python sandbox ready" } else { "Python sandbox not ready" });
            }
            7 => {
                if Confirm::with_theme(&t).with_prompt("Overwrite ~/.agent prompts, rules and skills with the defaults?").default(false).interact()? {
                    let (w, _) = install(true, true)?;
                    println!("  {} {w} files reinstalled", good("✓"));
                }
            }
            8 => {
                if Confirm::with_theme(&t).with_prompt("Reset models, colours and timeout? (API keys are kept)").default(false).interact()? {
                    config::set(Config::default())?;
                    crate::ui::init();
                    println!("  {} settings reset", good("✓"));
                }
            }
            _ => return Ok(()),
        }
    }
}

/// First-run walkthrough: files, keys, models, Python sandbox, summary.
pub async fn onboard() -> Result<()> {
    let t = theme();
    println!();
    println!("  {}  {}", accent(style("◆ Paidva").bold()), dim("the research agent that shows its work"));
    println!("    {}", dim("Every claim tagged, every number computed, every citation checked."));
    println!("    {}", dim("This takes about two minutes. Everything runs on free tiers."));

    heading("1 · Workspace", "Prompts, rules and skills live in ~/.agent. You can edit them any time.");
    let (w, k) = install(false, true)?;
    println!("  {} {w} installed, {k} already there", good("✓"));

    heading("2 · API keys", "Free keys, stored only on this machine in ~/.agent/.env (mode 600). You need at least one model key.");
    for (var, name, url, model) in KEYS { key_flow(var, name, url, *model).await?; }
    while !ready() {
        println!("  {} No model key yet. Groq takes a minute to sign up for: https://console.groq.com/keys", accent("!"));
        if !Confirm::with_theme(&t).with_prompt("Add a key now?").default(true).interact()? { break; }
        key_flow(KEYS[0].0, KEYS[0].1, KEYS[0].2, true).await?;
    }

    heading("3 · Models", "Paidva uses a strong model to plan and write, and a fast one to route and check.");
    let mut c = config::get();
    c.models.main = recommended(DEFAULT_MAIN);
    c.models.fast = recommended(DEFAULT_FAST);
    println!("    main  {}\n    fast  {}\n", chain_line(&c.models.main), chain_line(&c.models.fast));
    if Select::with_theme(&t).with_prompt("Models").items(&["Use these (recommended)", "Choose my own…"]).default(0).interact()? == 1 {
        let mut live = None;
        chain_screen("Main", "Plans, runs the analysis and writes the answer", &mut c.models.main, DEFAULT_MAIN, &mut live).await?;
        chain_screen("Fast", "Routes questions and checks claims", &mut c.models.fast, DEFAULT_FAST, &mut live).await?;
    }
    config::set(c)?;
    println!("  {} saved", good("✓"));

    heading("4 · Python sandbox", "Every number Paidva reports is computed here: no network, no access to your files, 30 s limit.");
    if python_ok() {
        println!("  {} already installed", good("✓"));
    } else if Confirm::with_theme(&t).with_prompt(format!("Install it now with uv? (~150 MB: {})", PY_PACKAGES.join(", "))).default(true).interact()? {
        let ok = install_python()?;
        println!("  {} {}", if ok { good("✓") } else { bad("✗") }, if ok { "ready" } else { "not ready yet: retry from `paidva settings`" });
    } else {
        println!("  {} skipped: maths questions will fail until you install it from `paidva settings`", accent("!"));
    }

    heading(if ready() { "All set" } else { "Almost there" }, "");
    doctor(false).await;
    println!("\n  Start with {}   ·   change anything with {}   ·   check health with {}\n", accent("paidva"), accent("paidva settings"), accent("paidva doctor"));
    Ok(())
}

/// Health check. `live` also validates each key with its provider.
pub async fn doctor(live: bool) -> bool {
    let row = |ok: Option<bool>, name: &str, detail: String| {
        let mark = match ok { Some(true) => good("✓").to_string(), Some(false) => bad("✗").to_string(), None => dim("–").to_string() };
        println!("  {mark} {:<18}{detail}", name);
    };
    let mut all = true;
    let dir = config::dir();
    let installed = DEFAULTS.iter().filter(|(rel, _)| dir.join(rel).exists()).count();
    let files_ok = installed == DEFAULTS.len();
    all &= files_ok;
    row(Some(files_ok), "workspace", if files_ok { dim(format!("{} files in {}", installed, dir.display())).to_string() } else { format!("{installed}/{} files · fix: paidva setup", DEFAULTS.len()) });
    row(Some(true), "settings", dim(if Config::path().exists() { Config::path().display().to_string() } else { "defaults (no config.toml yet)".into() }).to_string());
    for (var, name, _, model) in KEYS {
                match std::env::var(var).ok().filter(|k| !k.is_empty()) {
            None => row(None, name, dim(if *model { "no key" } else { "no key (optional)" }).to_string()),
            Some(k) if live => match check(var, &k).await {
                Ok(m) if m.is_empty() => row(Some(true), name, format!("{} · valid", config::mask(&k))),
                Ok(m) => row(Some(true), name, format!("{} · valid · {} models", config::mask(&k), m.len())),
                Err(e) => { if *model { all = false; } row(Some(false), name, format!("{} · {e:#}", config::mask(&k))) }
            },
            Some(k) => row(Some(true), name, config::mask(&k)),
        }
    }
    let keys = ready();
    all &= keys;
    if !keys { row(Some(false), "models", "no model key · fix: paidva setup".into()); }
    let cfg = config::get();
    row(Some(true), "main models", dim(chain_line(&cfg.models.main)).to_string());
    let py = python_ok();
    all &= py;
    row(Some(py), "python", if py { dim(format!("{} · {}", python().display(), PY_PACKAGES.join(" "))).to_string() } else { "missing · fix: paidva settings → Python sandbox".into() });
    let sb = std::path::Path::new("/usr/bin/sandbox-exec").exists();
    all &= sb;
    row(Some(sb), "sandbox", if sb { dim("sandbox-exec (macOS)").to_string() } else { "sandbox-exec not found: Paidva's sandbox needs macOS for now".into() });
    all
}
