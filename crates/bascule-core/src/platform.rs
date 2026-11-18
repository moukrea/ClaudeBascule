//! Everything that differs between Linux, macOS and Windows: local IPC, notifications, starting
//! the mini-app, the macOS keychain, finding `claude` and `jaunt`, detached processes.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// A local stream: a Unix socket, or on Windows a TCP connection to 127.0.0.1 with a key.
pub enum Stream {
    #[cfg(unix)]
    Unix(std::os::unix::net::UnixStream),
    Tcp(std::net::TcpStream),
}

impl Stream {
    pub fn try_clone(&self) -> std::io::Result<Stream> {
        Ok(match self {
            #[cfg(unix)]
            Stream::Unix(s) => Stream::Unix(s.try_clone()?),
            Stream::Tcp(s) => Stream::Tcp(s.try_clone()?),
        })
    }
    pub fn set_read_timeout(&self, d: Option<Duration>) -> std::io::Result<()> {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => s.set_read_timeout(d),
            Stream::Tcp(s) => s.set_read_timeout(d),
        }
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => s.read(buf),
            Stream::Tcp(s) => s.read(buf),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => s.write(buf),
            Stream::Tcp(s) => s.write(buf),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => s.flush(),
            Stream::Tcp(s) => s.flush(),
        }
    }
}

/// An endpoint written as `{port, key}` in a JSON file (Windows), or a Unix socket path.
pub struct Endpoint {
    pub stream: Stream,
    /// On Windows, the key every request line must carry.
    pub key: Option<String>,
}

/// Connect to `socket` (Unix) or to the `{port, key}` in `json_file` (Windows, or when no socket).
pub fn connect(socket: &Path, json_file: &Path) -> Result<Endpoint> {
    #[cfg(not(unix))]
    let _ = socket;
    #[cfg(unix)]
    if socket.exists() {
        let s = std::os::unix::net::UnixStream::connect(socket)
            .with_context(|| format!("connect {}", socket.display()))?;
        return Ok(Endpoint {
            stream: Stream::Unix(s),
            key: None,
        });
    }
    let text = std::fs::read_to_string(json_file)
        .with_context(|| format!("read {}", json_file.display()))?;
    let v: Value = serde_json::from_str(&text)?;
    let port = v.get("port").and_then(Value::as_u64).context("no port")? as u16;
    let key = v.get("key").and_then(Value::as_str).map(str::to_string);
    let s = std::net::TcpStream::connect(("127.0.0.1", port)).context("connect 127.0.0.1")?;
    Ok(Endpoint {
        stream: Stream::Tcp(s),
        key,
    })
}

/// A random hex key from the OS-seeded hasher (enough for a local, per-user secret).
pub fn random_key() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    (0..4)
        .map(|i| {
            let mut h = RandomState::new().build_hasher();
            h.write_u64(i ^ std::process::id() as u64);
            format!("{:016x}", h.finish())
        })
        .collect()
}

/// Hide the console of a child on Windows and detach it from ours.
pub fn detach(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
}

/// Hide the console window of a short-lived child on Windows.
pub fn quiet(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

/// `claude`, found on PATH (on Windows also `claude.cmd` through cmd).
pub fn claude() -> Command {
    tool("claude")
}

/// `jaunt`, found on PATH (on Windows it is `jaunt.cmd`).
pub fn jaunt() -> Command {
    tool("jaunt")
}

fn tool(name: &str) -> Command {
    #[cfg(windows)]
    {
        if which(&format!("{name}.exe")).is_none() && which(&format!("{name}.cmd")).is_some() {
            let mut c = Command::new("cmd");
            c.args(["/D", "/C", name]);
            quiet(&mut c);
            return c;
        }
        let mut c = Command::new(name);
        quiet(&mut c);
        c
    }
    #[cfg(not(windows))]
    Command::new(name)
}

pub fn which(file: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(file))
        .find(|p| p.is_file())
}

/// Whether a process with this pid is alive.
pub fn pid_alive(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(all(unix, not(target_os = "linux")))]
    {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        let mut c = Command::new("tasklist");
        c.args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"]);
        quiet(&mut c);
        c.output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&format!("\"{pid}\"")))
            .unwrap_or(false)
    }
}

/// The mini-app's executable next to ours (all binaries are installed together).
pub fn app_path() -> Option<PathBuf> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let name = if cfg!(windows) {
        "bascule-app.exe"
    } else {
        "bascule-app"
    };
    let p = dir.join(name);
    p.is_file().then_some(p)
}

/// Start the mini-app on its own (it outlives the service).
pub fn launch_app() {
    #[cfg(target_os = "macos")]
    {
        // Our binaries live in `Claude Bascule.app/Contents/MacOS/`.
        let bundle = std::env::current_exe()
            .ok()
            .and_then(|p| p.ancestors().nth(3).map(Path::to_path_buf))
            .filter(|p| p.extension().is_some_and(|e| e == "app"));
        let mut c = Command::new("open");
        match bundle {
            Some(b) => c.arg(b),
            None => c.args(["-a", "Claude Bascule"]),
        };
        let _ = detach(&mut c).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let Some(app) = app_path() else { return };
        // Outside the service's cgroup, so restarting the service does not close the window.
        let unit = format!(
            "claude-bascule-app-{}",
            std::process::id() as u64 * 1000
                + (chrono::Utc::now().timestamp_millis() % 1000) as u64
        );
        let ok = detach(
            Command::new("systemd-run")
                .args(["--user", "--collect", "--quiet", "--unit", &unit])
                .arg(&app),
        )
        .spawn()
        .is_ok();
        if !ok {
            let _ = detach(&mut Command::new(app)).spawn();
        }
    }
    #[cfg(windows)]
    {
        if let Some(app) = app_path() {
            let _ = detach(&mut Command::new(app)).spawn();
        }
    }
}

/// A desktop notification. On Linux, with an action button: returns true when it was clicked.
pub fn notify(title: &str, body: &str, action: Option<&str>) -> bool {
    #[cfg(target_os = "linux")]
    {
        let mut cmd = Command::new("notify-send");
        cmd.args(["-a", "Claude Bascule", "-i", "system-switch-user"]);
        if let Some(label) = action {
            cmd.arg(format!("--action=open={label}")).arg("--wait");
        }
        cmd.arg(title).arg(body);
        return cmd
            .output()
            .map(|o| action.is_some() && String::from_utf8_lossy(&o.stdout).trim() == "open")
            .unwrap_or(false);
    }
    #[cfg(target_os = "macos")]
    {
        let _ = action;
        let script = "on run argv\ndisplay notification (item 2 of argv) with title (item 1 of argv)\nend run";
        let _ = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .arg(title)
            .arg(body)
            .output();
        return false;
    }
    #[cfg(windows)]
    {
        let _ = action;
        // A toast through PowerShell's registered app id; texts pass through the environment.
        let script = r#"[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null
$x = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02)
$t = $x.GetElementsByTagName('text'); $t.Item(0).AppendChild($x.CreateTextNode($env:CB_TITLE)) > $null; $t.Item(1).AppendChild($x.CreateTextNode($env:CB_BODY)) > $null
$id = '{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe'
[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier($id).Show([Windows.UI.Notifications.ToastNotification]::new($x))"#;
        let mut c = Command::new("powershell");
        c.args(["-NoProfile", "-NonInteractive", "-Command", script])
            .env("CB_TITLE", title)
            .env("CB_BODY", body);
        quiet(&mut c);
        let _ = c.output();
        return false;
    }
    #[allow(unreachable_code)]
    false
}

/// Open a URL in the default browser.
pub fn open_url(url: &str) -> Result<()> {
    if !url.starts_with("https://") {
        bail!("only https links");
    }
    #[cfg(target_os = "linux")]
    let mut c = Command::new("xdg-open");
    #[cfg(target_os = "macos")]
    let mut c = Command::new("open");
    #[cfg(windows)]
    let mut c = {
        let mut c = Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    };
    detach(c.arg(url))
        .spawn()
        .map(|_| ())
        .context("open the browser")
}

// ---- macOS keychain (where Claude Code keeps its credentials on a Mac) ----

/// Claude Code's keychain service for a config dir: `Claude Code-credentials`, plus
/// `-<first 8 hex of sha256(dir)>` when `CLAUDE_CONFIG_DIR` is set (read in its bundle).
pub fn keychain_service(config_dir: Option<&Path>) -> String {
    use sha2::{Digest, Sha256};
    match config_dir {
        None => "Claude Code-credentials".into(),
        Some(d) => {
            let h = Sha256::digest(d.to_string_lossy().as_bytes());
            let hex: String = h.iter().take(4).map(|b| format!("{b:02x}")).collect();
            format!("Claude Code-credentials-{hex}")
        }
    }
}

pub fn keychain_account() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "claude".into())
}

/// The keychain item's text, None when there is none.
pub fn keychain_read(service: &str, account: &str) -> Result<Option<String>> {
    let out = Command::new("security")
        .args(["find-generic-password", "-a", account, "-s", service, "-w"])
        .output()
        .context("run security")?;
    if out.status.code() == Some(44) {
        return Ok(None);
    }
    if !out.status.success() {
        bail!("security find-generic-password failed ({})", out.status);
    }
    Ok(Some(
        String::from_utf8_lossy(&out.stdout)
            .trim_end_matches('\n')
            .to_string(),
    ))
}

pub fn keychain_write(service: &str, account: &str, text: &str) -> Result<()> {
    let hex: String = text.as_bytes().iter().map(|b| format!("{b:02x}")).collect();
    let st = Command::new("security")
        .args([
            "add-generic-password",
            "-U",
            "-a",
            account,
            "-s",
            service,
            "-X",
            &hex,
        ])
        .status()
        .context("run security")?;
    if !st.success() {
        bail!("security add-generic-password failed ({st})");
    }
    Ok(())
}

pub fn keychain_delete(service: &str, account: &str) {
    let _ = Command::new("security")
        .args(["delete-generic-password", "-a", account, "-s", service])
        .output();
}

/// The jaunt local control endpoint: Unix socket, or `control.json` on Windows.
pub fn jaunt_paths(home: &Path) -> (PathBuf, PathBuf) {
    let state = if let Some(given) = std::env::var_os("jaunt_STATE").filter(|v| !v.is_empty()) {
        PathBuf::from(given)
    } else if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData/Local"))
            .join("jaunt")
    } else {
        home.join(".local/share/jaunt")
    };
    (state.join("control.sock"), state.join("control.json"))
}

/// Whether jaunt looks installed on this machine.
pub fn jaunt_installed(home: &Path) -> bool {
    let (sock, json) = jaunt_paths(home);
    sock.exists()
        || json.exists()
        || which(if cfg!(windows) { "jaunt.cmd" } else { "jaunt" }).is_some()
}

/// A JSON line with the endpoint's key added (Windows).
pub fn keyed(mut v: Value, key: &Option<String>) -> Value {
    if let Some(k) = key {
        v["key"] = json!(k);
    }
    v
}
