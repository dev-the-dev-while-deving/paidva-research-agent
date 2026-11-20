//! User settings: `~/.agent/config.toml` (models, look, sandbox) and API keys in `~/.agent/.env` (mode 600).
//! Missing file or missing fields fall back to the defaults below, so an old install keeps working.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

/// Every key Paidva reads: (env var, provider name, where to get one, needed for models?).
pub const KEYS: &[(&str, &str, &str, bool)] = &[
    ("GROQ_API_KEY", "Groq", "https://console.groq.com/keys", true),
    ("GEMINI_API_KEY", "Google AI Studio", "https://aistudio.google.com/apikey", true),
    ("OPENROUTER_API_KEY", "OpenRouter", "https://openrouter.ai/settings/keys", true),
    ("S2_API_KEY", "Semantic Scholar", "https://www.semanticscholar.org/product/api", false),
];

// Each Groq model has its own daily token cap (200k). Gemini's free tier is often overloaded (503s), so it
// comes after Groq; OpenRouter free models share 50 requests/day, so they sit last as an emergency fallback.
pub const DEFAULT_MAIN: &[&str] = &["openai/gpt-oss-120b", "qwen/qwen3.8-27b", "gemini:gemini-3.8-flash", "openrouter:google/gemma-4-31b-it:free"];
pub const DEFAULT_FAST: &[&str] = &["openai/gpt-oss-20b", "openai/gpt-oss-120b", "gemini:gemini-3.5-flash-lite", "openrouter:google/gemma-4-26b-a4b-it:free"];

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Config {
    pub models: Models,
    pub ui: Ui,
    pub sandbox: Sandbox,
}

/// Fallback chains: the first model with a key and quota wins.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Models {
    /// Planner, analyst, writer.
    pub main: Vec<String>,
    /// Router, checker, consistency samples.
    pub fast: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Ui {
    /// "auto" (colour when the output is a terminal), "always" or "never".
    pub color: String,
    /// Show each step (route, search, python, verify …) while an answer is being built.
    pub show_steps: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Sandbox {
    pub timeout_secs: u64,
}

impl Default for Models {
    fn default() -> Self {
        Self { main: DEFAULT_MAIN.iter().map(|s| s.to_string()).collect(), fast: DEFAULT_FAST.iter().map(|s| s.to_string()).collect() }
    }
}
impl Default for Ui {
    fn default() -> Self { Self { color: "auto".into(), show_steps: true } }
}
impl Default for Sandbox {
    fn default() -> Self { Self { timeout_secs: 30 } }
}

pub fn dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".agent")
}

impl Config {
    pub fn path() -> PathBuf { dir().join("config.toml") }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path()).ok().and_then(|t| toml::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        std::fs::create_dir_all(dir())?;
        let body = format!("# Paidva settings. Edit here or run `paidva settings`.\n\n{}", toml::to_string_pretty(self)?);
        std::fs::write(Self::path(), body).context("writing config.toml")
    }
}

fn cell() -> &'static RwLock<Config> {
    static C: OnceLock<RwLock<Config>> = OnceLock::new();
    C.get_or_init(|| RwLock::new(Config::load()))
}

/// Current settings (a cheap clone, so a change made in `/settings` applies to the next call).
pub fn get() -> Config { cell().read().unwrap().clone() }

/// Save settings to disk and apply them to this process.
pub fn set(c: Config) -> Result<()> {
    c.save()?;
    *cell().write().unwrap() = c;
    Ok(())
}

pub fn env_path() -> PathBuf { dir().join(".env") }

/// Keys from the environment, else ./.env, else ~/.agent/.env. Never overrides a variable that is already set.
pub fn load_env() {
    for p in [PathBuf::from(".env"), env_path()] {
        let Ok(text) = std::fs::read_to_string(&p) else { continue };
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim().trim_matches('"'));
            if k.starts_with('#') || v.is_empty() || std::env::var_os(k).is_some() { continue; }
            // SAFETY: called once at startup, before the tokio runtime spawns work.
            unsafe { std::env::set_var(k, v) };
        }
    }
}

/// Set (or with `None`, remove) one key in ~/.agent/.env, keeping other lines; the file stays mode 600.
pub fn set_key(var: &str, value: Option<&str>) -> Result<()> {
    let p = env_path();
    let old = std::fs::read_to_string(&p).unwrap_or_default();
    let mut lines: Vec<String> = old.lines().filter(|l| l.split_once('=').map(|(k, _)| k.trim()) != Some(var)).map(str::to_string).collect();
    if let Some(v) = value { lines.push(format!("{var}={v}")); }
    std::fs::create_dir_all(dir())?;
    write_private(&p, &(lines.join("\n") + "\n"))?;
    // SAFETY: the settings screens run on the main thread with no other tasks touching the environment.
    unsafe { match value { Some(v) => std::env::set_var(var, v), None => std::env::remove_var(var) } }
    Ok(())
}

fn write_private(p: &std::path::Path, body: &str) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(p)?;
    f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    f.write_all(body.as_bytes())?;
    Ok(())
}

/// "gsk_…a1B2": enough to tell keys apart, never enough to use one.
pub fn mask(key: &str) -> String {
    let n = key.chars().count();
    if n <= 10 { return "•".repeat(n.max(4)); }
    let head: String = key.chars().take(4).collect();
    let tail: String = key.chars().skip(n - 4).collect();
    format!("{head}…{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_round_trips_and_fills_missing_fields() {
        let c = Config::default();
        assert_eq!(toml::from_str::<Config>(&toml::to_string_pretty(&c).unwrap()).unwrap(), c);
        let partial: Config = toml::from_str("[ui]\ncolor = \"never\"\n").unwrap();
        assert_eq!(partial.ui.color, "never");
        assert!(partial.ui.show_steps);
        assert_eq!(partial.models, Models::default());
        assert_eq!(partial.sandbox.timeout_secs, 30);
    }

    #[test]
    fn mask_hides_the_middle() {
        assert_eq!(mask("gsk_abcdefghijklmnop1234"), "gsk_…1234");
        assert_eq!(mask("short"), "•••••");
    }
}
