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

/// Where the installer puts the browser extension (Claude Bascule Connector), to load unpacked.
pub fn extension_dir() -> Option<PathBuf> {
    let home = crate::paths::home_dir();
    let env = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let dir = if cfg!(windows) {
        env("LOCALAPPDATA")
            .unwrap_or_else(|| home.join("AppData").join("Local"))
            .join("Programs")
            .join("ClaudeBascule")
            .join("chrome-extension")
    } else if cfg!(target_os = "macos") {
        home.join("Library")
            .join("Application Support")
            .join("ClaudeBascule")
            .join("chrome-extension")
    } else {
        env("XDG_DATA_HOME")
            .unwrap_or_else(|| home.join(".local").join("share"))
            .join("claude-bascule")
            .join("chrome-extension")
    };
    dir.join("manifest.json").is_file().then_some(dir)
}

/// Open a folder in the file manager.
pub fn open_folder(path: &Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    let mut c = Command::new("xdg-open");
    #[cfg(target_os = "macos")]
    let mut c = Command::new("open");
    #[cfg(windows)]
    let mut c = Command::new("explorer");
    detach(c.arg(path))
        .spawn()
        .map(|_| ())
        .context("open the folder")
}

/// This computer and user, as a short hash (the system's machine id and the home folder): account
/// backups put Claude Code sign-ins back only where they were made. Empty when unknown.
pub fn machine_key(home: &Path) -> String {
    let id = machine_id().unwrap_or_default();
    if id.trim().is_empty() {
        return String::new();
    }
    use sha2::{Digest, Sha256};
    let h = Sha256::digest(format!("{}\n{}", id.trim(), home.display()).as_bytes());
    h.iter().take(12).map(|b| format!("{b:02x}")).collect()
}

fn machine_id() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        ["/etc/machine-id", "/var/lib/dbus/machine-id"]
            .iter()
            .find_map(|f| std::fs::read_to_string(f).ok())
    }
    #[cfg(target_os = "macos")]
    {
        let out = Command::new("ioreg")
            .args(["-rd1", "-c", "IOPlatformExpertDevice"])
            .output()
            .ok()?;
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .find(|l| l.contains("IOPlatformUUID"))
            .and_then(|l| l.split('"').nth(3))
            .map(str::to_string)
    }
    #[cfg(windows)]
    {
        let out = quiet(Command::new("reg").args([
            "query",
            r"HKLM\SOFTWARE\Microsoft\Cryptography",
            "/v",
            "MachineGuid",
        ]))
        .output()
        .ok()?;
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .find(|l| l.contains("MachineGuid"))
            .and_then(|l| l.split_whitespace().last())
            .map(str::to_string)
    }
}

/// The user's downloads folder (where account backups go), else the home folder.
pub fn downloads_dir(home: &Path) -> PathBuf {
    // Linux: the localized folder from xdg-user-dirs (`XDG_DOWNLOAD_DIR="$HOME/Téléchargements"`).
    #[cfg(target_os = "linux")]
    {
        let conf = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        if let Some(dir) = std::fs::read_to_string(conf.join("user-dirs.dirs"))
            .ok()
            .and_then(|t| xdg_download_dir(&t, home))
        {
            if dir.is_dir() {
                return dir;
            }
        }
    }
    let d = home.join("Downloads");
    if d.is_dir() {
        d
    } else {
        home.to_path_buf()
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn xdg_download_dir(text: &str, home: &Path) -> Option<PathBuf> {
    let line = text
        .lines()
        .find(|l| l.trim_start().starts_with("XDG_DOWNLOAD_DIR="))?;
    let v = line.split_once('=')?.1.trim().trim_matches('"');
    let v = v.strip_prefix("$HOME").map_or_else(
        || PathBuf::from(v),
        |rest| home.join(rest.trim_start_matches('/')),
    );
    (v != home).then_some(v)
}

/// A Chromium-based browser found on this machine.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Browser {
    /// `chrome`, `chromium`, `brave`, `edge`, `vivaldi`; with `-flatpak` for a Flatpak install.
    pub id: String,
    pub name: String,
    #[serde(skip)]
    pub exe: PathBuf,
    /// Its extensions page (`chrome://extensions`, `edge://extensions`...).
    pub page: String,
    /// The Connector's folder this browser loads (a Flatpak browser has its own copy).
    pub extension_dir: Option<PathBuf>,
}

impl Browser {
    /// `chrome` for both `chrome` and `chrome-flatpak`.
    pub fn family(&self) -> &str {
        self.id.trim_end_matches("-flatpak")
    }
}

/// Chrome, Chromium, Brave, Edge and Vivaldi, when installed (Chrome first; on Linux, Flatpak
/// installs too).
pub fn browsers() -> Vec<Browser> {
    let mut out = Vec::new();
    let ext = extension_dir();
    let mut add = |id: &str, name: &str, page: &str, exe: Option<PathBuf>| {
        if let Some(exe) = exe {
            out.push(Browser {
                id: id.into(),
                name: name.into(),
                exe,
                page: page.into(),
                extension_dir: ext.clone(),
            });
        }
    };
    #[cfg(target_os = "linux")]
    {
        let find = |names: &[&str]| names.iter().find_map(|n| which(n));
        add(
            "chrome",
            "Google Chrome",
            "chrome://extensions/",
            find(&["google-chrome", "google-chrome-stable"]),
        );
        add(
            "chromium",
            "Chromium",
            "chrome://extensions/",
            find(&["chromium", "chromium-browser"]),
        );
        add(
            "brave",
            "Brave",
            "brave://extensions/",
            find(&["brave-browser", "brave"]),
        );
        add(
            "edge",
            "Microsoft Edge",
            "edge://extensions/",
            find(&["microsoft-edge", "microsoft-edge-stable"]),
        );
        add(
            "vivaldi",
            "Vivaldi",
            "vivaldi://extensions/",
            find(&["vivaldi", "vivaldi-stable"]),
        );
        let home = crate::paths::home_dir();
        for app in crate::flatpak::APPS {
            if let Some(exe) = crate::flatpak::launcher(&home, app) {
                let page = match app.family {
                    "brave" => "brave://extensions/",
                    "edge" => "edge://extensions/",
                    "vivaldi" => "vivaldi://extensions/",
                    _ => "chrome://extensions/",
                };
                let dir = crate::flatpak::extension_dir(&home, app);
                out.push(Browser {
                    id: format!("{}-flatpak", app.family),
                    name: format!("{} (Flatpak)", app.name),
                    exe,
                    page: page.into(),
                    extension_dir: dir.join("manifest.json").is_file().then_some(dir),
                });
            }
        }
        // Chrome first, whichever way it is installed.
        out.sort_by_key(|b| b.family() != "chrome");
    }
    #[cfg(target_os = "macos")]
    {
        let home = crate::paths::home_dir();
        let find = |app: &str, bin: &str| {
            [PathBuf::from("/Applications"), home.join("Applications")]
                .into_iter()
                .map(|d| {
                    d.join(format!("{app}.app"))
                        .join("Contents")
                        .join("MacOS")
                        .join(bin)
                })
                .find(|p| p.is_file())
        };
        add(
            "chrome",
            "Google Chrome",
            "chrome://extensions/",
            find("Google Chrome", "Google Chrome"),
        );
        add(
            "chromium",
            "Chromium",
            "chrome://extensions/",
            find("Chromium", "Chromium"),
        );
        add(
            "brave",
            "Brave",
            "brave://extensions/",
            find("Brave Browser", "Brave Browser"),
        );
        add(
            "edge",
            "Microsoft Edge",
            "edge://extensions/",
            find("Microsoft Edge", "Microsoft Edge"),
        );
        add(
            "vivaldi",
            "Vivaldi",
            "vivaldi://extensions/",
            find("Vivaldi", "Vivaldi"),
        );
    }
    #[cfg(windows)]
    {
        let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
        let roots: Vec<PathBuf> = ["LOCALAPPDATA", "ProgramFiles", "ProgramFiles(x86)"]
            .iter()
            .filter_map(|k| env(k))
            .collect();
        let find = |rel: &[&str]| {
            roots.iter().find_map(|r| {
                let p = rel.iter().fold(r.clone(), |p, s| p.join(s));
                p.is_file().then_some(p)
            })
        };
        add(
            "chrome",
            "Google Chrome",
            "chrome://extensions/",
            find(&["Google", "Chrome", "Application", "chrome.exe"]),
        );
        add(
            "brave",
            "Brave",
            "brave://extensions/",
            find(&["BraveSoftware", "Brave-Browser", "Application", "brave.exe"]),
        );
        add(
            "edge",
            "Microsoft Edge",
            "edge://extensions/",
            find(&["Microsoft", "Edge", "Application", "msedge.exe"]),
        );
        add(
            "vivaldi",
            "Vivaldi",
            "vivaldi://extensions/",
            find(&["Vivaldi", "Application", "vivaldi.exe"]),
        );
        add(
            "chromium",
            "Chromium",
            "chrome://extensions/",
            find(&["Chromium", "Application", "chrome.exe"]),
        );
    }
    out
}

/// Open a browser's extensions page (the browser starts, or opens a tab if it runs).
/// Open a browser's extensions page (the browser starts, or opens a tab if it runs).
pub fn open_extensions_page(id: &str) -> Result<()> {
    let b = browsers()
        .into_iter()
        .find(|b| b.id == id)
        .context("browser not found")?;
    let page = b.page.clone();
    open_in_browser(id, &page)
}

/// The browser chosen in the settings (by id, else of the same family), else the first one found.
pub fn chosen_browser(paths: &crate::paths::Paths) -> Option<Browser> {
    let id = crate::config::Settings::load(&paths.settings_file())
        .map(|s| s.browser)
        .unwrap_or_default();
    let all = browsers();
    let family = id.trim_end_matches("-flatpak").to_string();
    all.iter()
        .find(|b| b.id == id)
        .or_else(|| all.iter().find(|b| !id.is_empty() && b.family() == family))
        .or(all.first())
        .cloned()
}

/// Open a page (also chrome:// pages) in a given browser (by id, else of the same family), or the
/// first one found.
pub fn open_in_browser(id: &str, url: &str) -> Result<()> {
    let all = browsers();
    let family = id.trim_end_matches("-flatpak");
    let b = all
        .iter()
        .find(|b| b.id == id)
        .or_else(|| all.iter().find(|b| b.family() == family))
        .or(all.first())
        .context("no Chromium-based browser found")?;
    let url = if b.family() == "edge" {
        url.replacen("chrome://", "edge://", 1)
    } else if b.family() == "brave" {
        url.replacen("chrome://", "brave://", 1)
    } else {
        url.to_string()
    };
    let mut c = Command::new(&b.exe);
    detach(c.arg(url))
        .spawn()
        .map(|_| ())
        .context("start the browser")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xdg_downloads() {
        let home = Path::new("/home/u");
        let t = "# comment\nXDG_DESKTOP_DIR=\"$HOME/Bureau\"\nXDG_DOWNLOAD_DIR=\"$HOME/Téléchargements\"\n";
        assert_eq!(
            xdg_download_dir(t, home),
            Some(PathBuf::from("/home/u/Téléchargements"))
        );
        assert_eq!(xdg_download_dir("XDG_DOWNLOAD_DIR=\"$HOME/\"", home), None);
        assert_eq!(
            xdg_download_dir("XDG_DOWNLOAD_DIR=\"/data/dl\"", home),
            Some(PathBuf::from("/data/dl"))
        );
    }
}
