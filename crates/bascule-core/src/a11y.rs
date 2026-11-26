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
    "com.google.Chrome.desktop",
    "org.chromium.Chromium.desktop",
    "com.brave.Browser.desktop",
    "com.microsoft.Edge.desktop",
    "com.vivaldi.Vivaldi.desktop",
];

/// How long the user has to sign in to claude.ai when Claude in Chrome's sign-in needs it.
pub const LOGIN_SECS: u64 = 600;

/// Run the AT-SPI helper with a deadline: a desktop accessibility bus that stops answering must
/// not leave helpers piling up. Its progress lines (`{"stage": ...}`) go to `on_stage` as they
/// come; a stage may extend the deadline (`extend` gives the new one from now).
#[cfg(target_os = "linux")]
fn run(
    args: &[&str],
    launcher: Option<&std::path::Path>,
    deadline: std::time::Duration,
    mut on_stage: impl FnMut(&str) -> Option<std::time::Duration>,
) -> Result<Value> {
    use std::io::{BufRead, Write};
    let mut cmd = std::process::Command::new("python3");
    // The helper reopens a page with the browser's own command (a Flatpak browser's is not the
    // program its process runs).
    if let Some(l) = launcher {
        cmd.env("BASCULE_BROWSER", l);
    }
    let mut child = cmd
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
    let out = child.stdout.take().context("stdout")?;
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(out).lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut until = std::time::Instant::now() + deadline;
    let mut last: Option<Value> = None;
    loop {
        match rx.recv_timeout(std::time::Duration::from_millis(200)) {
            Ok(line) => {
                let Ok(v) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                if let Some(stage) = v.get("stage").and_then(Value::as_str) {
                    if let Some(more) = on_stage(stage) {
                        until = std::time::Instant::now() + more;
                    }
                } else {
                    last = Some(v);
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        }
        if std::time::Instant::now() > until {
            let _ = child.kill();
            let _ = child.wait();
            bail!("the desktop's accessibility did not answer in time (restart Chrome, or log out and in)");
        }
    }
    let _ = child.wait();
    last.context("no answer from the accessibility helper")
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
            let user_flatpak =
                crate::paths::home_dir().join(".local/share/flatpak/exports/share/applications");
            let system = [
                std::path::PathBuf::from("/usr/share/applications"),
                std::path::PathBuf::from("/usr/local/share/applications"),
                std::path::PathBuf::from("/var/lib/flatpak/exports/share/applications"),
                user_flatpak,
            ]
            .iter()
            .map(|d| d.join(e))
            .find(|p| p.is_file());
            let Some(src) = system else { continue };
            let text = std::fs::read_to_string(&src)?;
            if text.contains(FLAG) {
                continue;
            }
            let app_id = e.trim_end_matches(".desktop");
            let patched: Vec<String> = text.lines().map(|l| with_flag(l, app_id)).collect();
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

/// An `Exec=` line that starts the browser with the accessibility flag: right after its program,
/// or for a Flatpak app (`flatpak run ... <app id> ...`) right after the app id, among the
/// browser's own arguments.
#[cfg(target_os = "linux")]
fn with_flag(line: &str, app_id: &str) -> String {
    let Some(rest) = line.strip_prefix("Exec=") else {
        return line.to_string();
    };
    let words: Vec<&str> = rest.split(' ').collect();
    let at = if words.first().is_some_and(|w| w.ends_with("flatpak")) {
        words.iter().position(|w| *w == app_id).map(|i| i + 1)
    } else {
        None
    }
    .unwrap_or(1);
    let mut out: Vec<&str> = words[..at.min(words.len())].to_vec();
    out.push(FLAG);
    out.extend_from_slice(&words[at.min(words.len())..]);
    format!("Exec={}", out.join(" "))
}

/// Sign Claude in Chrome out and in again with claude.ai's current session, approving the consent
/// only when the page names `email`. Its options page must be open. When claude.ai turns out to be
/// signed out, `on_stage("sign_in")` is called and the user has `LOGIN_SECS` to sign in there;
/// `on_stage("approving")` follows once they did.
/// `launcher` is the browser's command, used to reopen Claude in Chrome's options.
pub fn resign_cic(
    email: &str,
    launcher: Option<&std::path::Path>,
    mut on_stage: impl FnMut(&str),
) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let login = LOGIN_SECS.to_string();
        let v = run(
            &["resign", email, &login],
            launcher,
            std::time::Duration::from_secs(150),
            |stage| {
                on_stage(stage);
                (stage == "sign_in").then(|| std::time::Duration::from_secs(LOGIN_SECS + 90))
            },
        )?;
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
        let _ = (email, launcher, &mut on_stage);
        bail!("only on Linux for now")
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn flag_goes_among_the_browser_arguments() {
        assert_eq!(
            with_flag("Exec=/usr/bin/google-chrome-stable %U", "google-chrome"),
            "Exec=/usr/bin/google-chrome-stable --force-renderer-accessibility %U"
        );
        assert_eq!(
            with_flag("Exec=/usr/bin/flatpak run --branch=stable --arch=x86_64 --command=/app/bin/chrome --file-forwarding com.google.Chrome @@u %U @@", "com.google.Chrome"),
            "Exec=/usr/bin/flatpak run --branch=stable --arch=x86_64 --command=/app/bin/chrome --file-forwarding com.google.Chrome --force-renderer-accessibility @@u %U @@"
        );
        assert_eq!(with_flag("Name=Chrome", "x"), "Name=Chrome");
    }
}
