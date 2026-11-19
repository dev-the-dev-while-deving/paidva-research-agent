//! Runs model-written Python under macOS sandbox-exec: no network, no reads of $HOME
//! (except the Python install), writes only to a fresh temp dir, 30 s wall clock.
// ponytail: sandbox-exec is deprecated and macOS-only; upgrade to Pyodide-on-Deno (WASM) or Docker on Linux.
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;

pub const TIMEOUT: Duration = Duration::from_secs(30);
const AGENTLIB: &str = include_str!("../home/agentlib.py");
const MAX_OUT: usize = 6000;

#[derive(Debug, Clone, serde::Serialize)]
pub struct Run {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

pub fn python(home: &Path) -> PathBuf {
    home.join(".agent/py/bin/python")
}

fn profile(home: &Path, tmp: &Path, uv_python: &Path) -> String {
    let h = home.display();
    format!(
        r#"(version 1)
(allow default)
(deny network*)
(deny file-read* (subpath "{h}"))
(allow file-read-metadata)
(allow file-read* (subpath "{h}/.agent/py") (subpath "{uv}") (subpath "{t}"))
(deny file-write*)
(allow file-write* (subpath "{t}") (literal "/dev/null"))
"#,
        uv = uv_python.display(),
        t = tmp.display()
    )
}

/// Runs `code` with optional input files copied into the working dir.
pub async fn run(home: &Path, code: &str, files: &[PathBuf]) -> Result<Run> {
    let dir = tempfile::tempdir()?;
    let tmp = dir.path().canonicalize()?;
    for f in files {
        let name = f.file_name().context("bad file name")?;
        std::fs::copy(f, tmp.join(name)).with_context(|| format!("copy {}", f.display()))?;
    }
    // The tested helper library ships with every run. `-I` drops the script dir from sys.path, so add the
    // (sandboxed, temporary) working dir back explicitly.
    std::fs::write(tmp.join("agentlib.py"), AGENTLIB)?;
    std::fs::write(tmp.join("main.py"), format!("import sys, os; sys.path.insert(0, os.getcwd())\n{code}"))?;
    let py = python(home);
    // The venv's python symlinks into uv's interpreter store, which must stay readable.
    let real = std::fs::canonicalize(&py).context("~/.agent/py missing: run `agent init`")?;
    let uv_python = real.ancestors().nth(2).unwrap_or(&real).to_path_buf();

    let mut cmd = Command::new("/usr/bin/sandbox-exec");
    cmd.arg("-p").arg(profile(home, &tmp, &uv_python))
        .arg(&py).arg("-I").arg("-B").arg("main.py")
        .current_dir(&tmp)
        .env_clear()
        .env("HOME", &tmp).env("TMPDIR", &tmp).env("MPLBACKEND", "Agg").env("OMP_NUM_THREADS", "2")
        .kill_on_drop(true);
    let out = match tokio::time::timeout(TIMEOUT, cmd.output()).await {
        Ok(o) => o?,
        Err(_) => return Ok(Run { ok: false, stdout: String::new(), stderr: format!("timed out after {}s", TIMEOUT.as_secs()) }),
    };
    Ok(Run {
        ok: out.status.success(),
        stdout: clip(&String::from_utf8_lossy(&out.stdout)),
        stderr: clip(&String::from_utf8_lossy(&out.stderr)),
    })
}

fn clip(s: &str) -> String {
    if s.len() <= MAX_OUT { return s.to_string(); }
    let mut end = MAX_OUT;
    while !s.is_char_boundary(end) { end -= 1; }
    format!("{}\n…[truncated]", &s[..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn home() -> PathBuf { PathBuf::from(std::env::var("HOME").unwrap()) }

    #[tokio::test]
    async fn computes_and_is_isolated() {
        let h = home();
        if !python(&h).exists() { eprintln!("skip: no ~/.agent/py"); return; }
        let r = run(&h, "import scipy.stats as s; print(round(s.norm.cdf(1.96), 4))", &[]).await.unwrap();
        assert!(r.ok && r.stdout.trim() == "0.975", "{r:?}");
        // The shipped library imports and passes its own tests inside the sandbox.
        let lib = run(&h, "import agentlib; agentlib._selftest()", &[]).await.unwrap();
        assert!(lib.ok && lib.stdout.contains("selftest OK"), "{lib:?}");

        let net = "import urllib.request\ntry:\n urllib.request.urlopen('https://example.com', timeout=5); print('OPEN')\nexcept Exception: print('BLOCKED')";
        assert_eq!(run(&h, net, &[]).await.unwrap().stdout.trim(), "BLOCKED");

        let secret = format!("try:\n open('{}/.zshrc').read(); print('OPEN')\nexcept Exception: print('BLOCKED')", h.display());
        assert_eq!(run(&h, &secret, &[]).await.unwrap().stdout.trim(), "BLOCKED");

        let write = format!("try:\n open('{}/pwned.txt','w').write('x'); print('OPEN')\nexcept Exception: print('BLOCKED')", h.display());
        assert_eq!(run(&h, &write, &[]).await.unwrap().stdout.trim(), "BLOCKED");
    }

    /// Escape attempts a hostile or careless script might make. `cargo test adversarial -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn adversarial() {
        let h = home();
        let cases = [
            ("subprocess network", "import subprocess\nr=subprocess.run(['/usr/bin/curl','-s','-m','5','https://example.com'],capture_output=True)\nprint('OPEN' if r.stdout else 'BLOCKED')".to_string()),
            ("read agent .env", format!("try:\n print('OPEN', len(open('{}/AI AGent/agent/.env').read()))\nexcept Exception: print('BLOCKED')", h.display())),
            ("read ~/.ssh", format!("import os\ntry:\n print('OPEN', os.listdir('{}/.ssh'))\nexcept Exception: print('BLOCKED')", h.display())),
            ("write /tmp", "try:\n open('/tmp/agent_pwned','w').write('x'); print('OPEN')\nexcept Exception: print('BLOCKED')".to_string()),
            ("read /etc/passwd", "try:\n open('/etc/passwd').read(); print('OPEN')\nexcept Exception: print('BLOCKED')".to_string()),
            ("list home names", format!("import os\ntry:\n print('OPEN', len(os.listdir('{}')))\nexcept Exception: print('BLOCKED')", h.display())),
            ("3 GB allocation", "import numpy as np\na=np.ones(3*1024**3//8); print('ALLOCATED', a.nbytes//1024**3, 'GB')".to_string()),
            ("infinite loop", "while True: pass".to_string()),
        ];
        for (name, code) in cases {
            let t = std::time::Instant::now();
            let r = run(&h, &code, &[]).await.unwrap();
            println!("{name:<20} {:<40} ({:.1}s)", format!("{} {}", r.stdout.trim(), r.stderr.lines().last().unwrap_or("")).chars().take(40).collect::<String>(), t.elapsed().as_secs_f64());
        }
    }
}
