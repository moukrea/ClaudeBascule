//! Driving the Claude in Chrome extension through the desktop's accessibility layer, like a screen
//! reader: no Chrome security prompt is involved, so it runs unattended on the user's own browser
//! session. Linux (AT-SPI) for now: Chrome must run with its accessibility on, which Claude Bascule
//! sets once (Chrome's launcher gets `--force-renderer-accessibility`; the session's accessibility
//! is turned on, without any screen reader).

#[cfg(target_os = "linux")]
use anyhow::Context;
use anyhow::{bail, Result};
#[cfg(target_os = "linux")]
use serde_json::Value;

#[cfg(target_os = "linux")]
const SCRIPT: &str = include_str!("a11y_linux.py");
#[cfg(target_os = "linux")]
const FLAG: &str = "--force-renderer-accessibility";
/// Desktop entries of Chromium-based browsers that get the flag.
#[cfg(target_os = "linux")]
const ENTRIES: &[&str] = &[
    "google-chrome.desktop",
    "google-chrome-beta.desktop",
    "chromium.desktop",
    "chromium-browser.desktop",
    "brave-browser.desktop",
    "microsoft-edge.desktop",
    "vivaldi-stable.desktop",
];

/// Run the AT-SPI helper with a deadline: a desktop accessibility bus that stops answering must
/// not leave helpers piling up.
#[cfg(target_os = "linux")]
fn run(args: &[&str], deadline: std::time::Duration) -> Result<Value> {
    use std::io::{Read, Write};
    let mut child = std::process::Command::new("python3")
        .arg("-I")
        .arg("-")
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("python3 is needed to drive Chrome's accessibility")?;
    child
        .stdin
        .take()
        .context("stdin")?
        .write_all(SCRIPT.as_bytes())?;
    let started = std::time::Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if started.elapsed() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("the desktop's accessibility did not answer in time (restart Chrome, or log out and in)");
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    let mut out = String::new();
    if let Some(mut o) = child.stdout.take() {
        o.read_to_string(&mut out)?;
    }
    serde_json::from_str(out.lines().last().unwrap_or(""))
        .context("no answer from the accessibility helper")
}

/// "unsupported", "off" (not prepared), "restart" (prepared, the running browser started without
/// the flag), "ready". Read from the launchers and the browsers' command lines only: it never
/// queries the accessibility bus (a periodic tree walk over a big browser can wedge it).
pub fn state() -> String {
    #[cfg(target_os = "linux")]
    {
        if !prepared() {
            return "off".into();
        }
        let mut running = false;
        let mut flagged = false;
        if let Ok(dir) = std::fs::read_dir("/proc") {
            for e in dir.flatten() {
                let Ok(cmd) = std::fs::read(e.path().join("cmdline")) else {
                    continue;
                };
                let mut parts = cmd.split(|b| *b == 0).filter(|p| !p.is_empty());
                let Some(prog) = parts.next() else { continue };
                let prog = String::from_utf8_lossy(prog);
                let base = prog.rsplit('/').next().unwrap_or("");
                if !matches!(
                    base,
                    "chrome"
                        | "google-chrome"
                        | "chromium"
                        | "chromium-browser"
                        | "brave"
                        | "msedge"
                        | "vivaldi-bin"
                ) {
                    continue;
                }
                // The browser's main process (renderers and helpers carry --type=...).
                let args: Vec<_> = parts.map(String::from_utf8_lossy).collect();
                if args.iter().any(|a| a.starts_with("--type=")) {
                    continue;
                }
                running = true;
                flagged |= args.iter().any(|a| a == FLAG);
            }
        }
        if running && !flagged {
            "restart".into()
        } else {
            "ready".into()
        }
    }
    #[cfg(not(target_os = "linux"))]
    "unsupported".into()
}

#[cfg(target_os = "linux")]
fn user_entries() -> std::path::PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| crate::paths::home_dir().join(".local").join("share"))
        .join("applications")
}

/// Whether a browser launcher already carries the flag.
pub fn prepared() -> bool {
    #[cfg(target_os = "linux")]
    {
        ENTRIES.iter().any(|e| {
            std::fs::read_to_string(user_entries().join(e)).is_ok_and(|t| t.contains(FLAG))
        })
    }
    #[cfg(not(target_os = "linux"))]
    false
}

/// Turn the session's accessibility on (no screen reader) and give the installed browsers'
/// launchers the accessibility flag. Takes effect when the browser next starts.
pub fn prepare() -> Result<usize> {
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("gsettings")
            .args([
                "set",
                "org.gnome.desktop.interface",
                "toolkit-accessibility",
                "true",
            ])
            .status();
        let dest = user_entries();
        std::fs::create_dir_all(&dest)?;
        let mut done = 0;
        for e in ENTRIES {
            let system = [
                "/usr/share/applications",
                "/usr/local/share/applications",
                "/var/lib/flatpak/exports/share/applications",
            ]
            .iter()
            .map(|d| std::path::Path::new(d).join(e))
            .find(|p| p.is_file());
            let Some(src) = system else { continue };
            let text = std::fs::read_to_string(&src)?;
            if text.contains(FLAG) {
                continue;
            }
            // Every Exec= line starts the browser with the flag right after its program.
            let patched: Vec<String> = text
                .lines()
                .map(|l| match l.strip_prefix("Exec=") {
                    Some(rest) => {
                        let mut parts = rest.splitn(2, ' ');
                        let prog = parts.next().unwrap_or("");
                        match parts.next() {
                            Some(args) => format!("Exec={prog} {FLAG} {args}"),
                            None => format!("Exec={prog} {FLAG}"),
                        }
                    }
                    None => l.to_string(),
                })
                .collect();
            std::fs::write(dest.join(e), patched.join("\n") + "\n")?;
            done += 1;
        }
        let _ = std::process::Command::new("update-desktop-database")
            .arg(&dest)
            .status();
        if done == 0 && !prepared() {
            bail!("no Chromium-based browser launcher found");
        }
        Ok(done)
    }
    #[cfg(not(target_os = "linux"))]
    bail!("only on Linux for now")
}

/// Sign Claude in Chrome out and in again with claude.ai's current session, approving the consent
/// only when the page names `email`. Its options page must be open.
pub fn resign_cic(email: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let v = run(&["resign", email], std::time::Duration::from_secs(150))?;
        if v["ok"].as_bool() == Some(true) {
            return Ok(());
        }
        bail!(
            "{}",
            v["error"]
                .as_str()
                .unwrap_or("Claude in Chrome was not moved")
        );
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = email;
        bail!("only on Linux for now")
    }
}
