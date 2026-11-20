//! Driving Chrome through its DevTools protocol, to move the Claude in Chrome extension from one
//! account to another without the user: it cannot be reached by another extension (Chrome isolates
//! extensions), but the browser's remote debugging, once the user allows it in
//! `chrome://inspect/#remote-debugging`, can open its options page and click like a person would.
//! Chrome asks the user to allow each new connection: one connection is kept open while the
//! browser runs. Cookies only pass through and are never logged.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tungstenite::{Message, WebSocket};

pub const CIC_ID: &str = "fcoeoabgfenejglbffodgkkbkcdhcgfn";
pub const CIC_CLIENT: &str = "dae2cad8-15c5-43d2-9046-fcaecc135fa4";

/// Claude in Chrome's buttons in its own languages (its i18n ids PlBReUqqzW and odXlk858Gb).
const LOGOUT: &[&str] = &[
    "Abmelden",
    "Cerrar sesión",
    "Esci",
    "Keluar",
    "Log out",
    "Sair",
    "Se déconnecter",
    "लॉग आउट करें",
    "ログアウト",
    "로그아웃",
];
const SIGNIN: &[&str] = &[
    "Accedi",
    "Anmelden",
    "Fazer login",
    "Iniciar sesión",
    "Log in",
    "Masuk",
    "Se connecter",
    "लॉग इन करें",
    "ログイン",
    "로그인",
];
/// claude.ai's consent page.
const APPROVE: &[&str] = &[
    "Autoriser",
    "Authorize",
    "Allow",
    "Autorisieren",
    "Zulassen",
    "Autorizar",
    "Permitir",
    "Autorizza",
    "Consenti",
    "Izinkan",
    "許可",
    "허용",
    "अनुमति दें",
];

/// Profile folders of Chromium-based browsers, where a running browser with remote debugging
/// writes `DevToolsActivePort` (port, then the browser's WebSocket path).
fn profile_dirs() -> Vec<PathBuf> {
    let home = crate::paths::home_dir();
    if cfg!(windows) {
        let local = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData").join("Local"));
        [
            "Google\\Chrome",
            "Microsoft\\Edge",
            "BraveSoftware\\Brave-Browser",
            "Chromium",
            "Vivaldi",
        ]
        .iter()
        .map(|p| local.join(p).join("User Data"))
        .collect()
    } else if cfg!(target_os = "macos") {
        let sup = home.join("Library").join("Application Support");
        [
            "Google/Chrome",
            "Microsoft Edge",
            "BraveSoftware/Brave-Browser",
            "Chromium",
            "Vivaldi",
        ]
        .iter()
        .map(|p| sup.join(p))
        .collect()
    } else {
        let conf = home.join(".config");
        [
            "google-chrome",
            "microsoft-edge",
            "BraveSoftware/Brave-Browser",
            "chromium",
            "vivaldi",
        ]
        .iter()
        .map(|p| conf.join(p))
        .collect()
    }
}

/// The browser endpoint (port, path) when remote debugging is on.
pub fn endpoint() -> Option<(u16, String)> {
    for d in profile_dirs() {
        if let Ok(text) = std::fs::read_to_string(d.join("DevToolsActivePort")) {
            let mut lines = text.lines();
            let port = lines.next()?.trim().parse().ok()?;
            let path = lines.next().unwrap_or("").trim().to_string();
            if TcpStream::connect_timeout(
                &(std::net::Ipv4Addr::LOCALHOST, port).into(),
                Duration::from_millis(300),
            )
            .is_ok()
            {
                return Some((port, path));
            }
        }
    }
    None
}

/// The connection to Chrome lives in a small process of its own (`bascule cdp-keeper`), so that
/// restarting or updating the service does not make Chrome ask again: one "Allow" per browser run.
/// The keeper relays one JSON request per line: {id, method, params, sessionId?} → {id, result|error}.
pub fn keeper_paths(paths: &crate::paths::Paths) -> (PathBuf, PathBuf) {
    (paths.state.join("cdp.sock"), paths.state.join("cdp.json"))
}

/// Run the keeper: connect to the browser (Chrome asks the user), serve until the browser closes.
pub fn run_keeper(paths: &crate::paths::Paths) -> Result<()> {
    let (port, path) = endpoint().context("Chrome remote debugging is off")?;
    let stream = TcpStream::connect(("127.0.0.1", port)).context("connect to Chrome")?;
    stream.set_read_timeout(Some(Duration::from_secs(180)))?;
    let (ws, _) = tungstenite::client(format!("ws://127.0.0.1:{port}{path}"), stream)
        .map_err(|e| anyhow::anyhow!("Chrome refused the connection: {e}"))?;
    ws.get_ref()
        .set_read_timeout(Some(Duration::from_secs(60)))?;
    let ws = std::sync::Arc::new(std::sync::Mutex::new((ws, 0u64)));
    // Chrome answered: we are allowed.
    relay(&ws, json!({"method": "Browser.getVersion", "params": {}}))?;
    let (sock, json_file) = keeper_paths(paths);
    crate::store::private_dir(&paths.state)?;
    let handle =
        |stream: crate::platform::Stream,
         key: Option<String>,
         ws: std::sync::Arc<std::sync::Mutex<(WebSocket<TcpStream>, u64)>>| {
            std::thread::spawn(move || {
                use std::io::{BufRead, BufReader, Write};
                let Ok(mut w) = stream.try_clone() else {
                    return;
                };
                for line in BufReader::new(stream).lines() {
                    let Ok(line) = line else { break };
                    let Ok(req) = serde_json::from_str::<Value>(&line) else {
                        continue;
                    };
                    if key.is_some() && req.get("key").and_then(Value::as_str) != key.as_deref() {
                        let _ = writeln!(w, "{}", json!({"id": req.get("id"), "error": "bad key"}));
                        continue;
                    }
                    let out = match relay(&ws, req.clone()) {
                        Ok(v) => json!({"id": req.get("id"), "result": v}),
                        Err(e) => json!({"id": req.get("id"), "error": format!("{e:#}")}),
                    };
                    if writeln!(w, "{out}").is_err() {
                        break;
                    }
                }
            });
        };
    #[cfg(unix)]
    {
        let _ = std::fs::remove_file(&sock);
        let listener = std::os::unix::net::UnixListener::bind(&sock)?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&sock, std::fs::Permissions::from_mode(0o600))?;
        crate::store::atomic_write(&json_file, &json!({"pid": std::process::id()}))?;
        let ws2 = ws.clone();
        std::thread::spawn(move || watch_browser(ws2));
        for s in listener.incoming().flatten() {
            handle(crate::platform::Stream::Unix(s), None, ws.clone());
        }
    }
    #[cfg(not(unix))]
    {
        let _ = &sock;
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
        let key = crate::platform::random_key();
        crate::store::atomic_write(
            &json_file,
            &json!({"port": listener.local_addr()?.port(), "key": key, "pid": std::process::id()}),
        )?;
        let ws2 = ws.clone();
        std::thread::spawn(move || watch_browser(ws2));
        for s in listener.incoming().flatten() {
            handle(
                crate::platform::Stream::Tcp(s),
                Some(key.clone()),
                ws.clone(),
            );
        }
    }
    Ok(())
}

/// The keeper ends with the browser: checked every 20 s.
fn watch_browser(ws: std::sync::Arc<std::sync::Mutex<(WebSocket<TcpStream>, u64)>>) {
    loop {
        std::thread::sleep(Duration::from_secs(20));
        if relay(&ws, json!({"method": "Browser.getVersion", "params": {}})).is_err() {
            std::process::exit(0);
        }
    }
}

fn relay(ws: &std::sync::Mutex<(WebSocket<TcpStream>, u64)>, req: Value) -> Result<Value> {
    let mut g = ws.lock().unwrap();
    g.1 += 1;
    let id = g.1;
    let mut msg = json!({"id": id, "method": req["method"], "params": req.get("params").cloned().unwrap_or(json!({}))});
    if let Some(s) = req.get("sessionId").filter(|v| !v.is_null()) {
        msg["sessionId"] = s.clone();
    }
    g.0.send(Message::Text(msg.to_string()))?;
    loop {
        let Message::Text(t) = g.0.read()? else {
            continue;
        };
        let v: Value = serde_json::from_str(&t)?;
        if v.get("id").and_then(Value::as_u64) == Some(id) {
            if let Some(e) = v.get("error") {
                bail!(
                    "{}: {}",
                    req["method"].as_str().unwrap_or("?"),
                    e.get("message").and_then(Value::as_str).unwrap_or("error")
                );
            }
            return Ok(v.get("result").cloned().unwrap_or(Value::Null));
        }
    }
}

/// Start the keeper on its own (outside the service, so it outlives its restarts).
fn spawn_keeper() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    // The service may be bascule-app (Windows): the keeper is the CLI next to it.
    let cli = exe.with_file_name(if cfg!(windows) {
        "bascule.exe"
    } else {
        "bascule"
    });
    let bin = if cli.is_file() { cli } else { exe };
    #[cfg(target_os = "linux")]
    {
        let ok = std::process::Command::new("systemd-run")
            .args([
                "--user",
                "--collect",
                "--quiet",
                "--unit",
                &format!("claude-bascule-cdp-{}", std::process::id()),
            ])
            .arg(&bin)
            .arg("cdp-keeper")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return;
        }
    }
    let mut c = std::process::Command::new(&bin);
    c.arg("cdp-keeper");
    let _ = crate::platform::detach(&mut c).spawn();
}

pub struct Cdp {
    stream: crate::platform::Stream,
    reader: std::io::BufReader<crate::platform::Stream>,
    key: Option<String>,
    next: u64,
}

impl Cdp {
    /// Reach the keeper, starting it when needed (Chrome then asks the user to allow it; this
    /// waits up to 3 minutes).
    pub fn connect_with(paths: &crate::paths::Paths) -> Result<Cdp> {
        endpoint().context("Chrome remote debugging is off")?;
        let (sock, json_file) = keeper_paths(paths);
        let mut started = false;
        let end = Instant::now() + Duration::from_secs(180);
        loop {
            if let Ok(ep) = crate::platform::connect(&sock, &json_file) {
                let stream = ep.stream;
                stream.set_read_timeout(Some(Duration::from_secs(90)))?;
                let mut c = Cdp {
                    reader: std::io::BufReader::new(stream.try_clone()?),
                    stream,
                    key: ep.key,
                    next: 0,
                };
                if c.call("Browser.getVersion", json!({}), None).is_ok() {
                    return Ok(c);
                }
            }
            if !started {
                spawn_keeper();
                started = true;
            }
            if Instant::now() > end {
                bail!("Chrome did not allow the connection");
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    }

    /// The keeper's connection when it already runs (never starts one: that would make Chrome ask).
    pub fn existing(paths: &crate::paths::Paths) -> Option<Cdp> {
        let (sock, json_file) = keeper_paths(paths);
        let ep = crate::platform::connect(&sock, &json_file).ok()?;
        let stream = ep.stream;
        stream
            .set_read_timeout(Some(Duration::from_secs(90)))
            .ok()?;
        let mut c = Cdp {
            reader: std::io::BufReader::new(stream.try_clone().ok()?),
            stream,
            key: ep.key,
            next: 0,
        };
        c.call("Browser.getVersion", json!({}), None).ok()?;
        Some(c)
    }

    pub fn connect() -> Result<Cdp> {
        Cdp::connect_with(&crate::paths::Paths::from_env())
    }

    pub fn call(&mut self, method: &str, params: Value, session: Option<&str>) -> Result<Value> {
        use std::io::{BufRead, Write};
        self.next += 1;
        let id = self.next;
        let mut msg = json!({"id": id, "method": method, "params": params});
        if let Some(s) = session {
            msg["sessionId"] = json!(s);
        }
        let msg = crate::platform::keyed(msg, &self.key);
        writeln!(self.stream, "{msg}")?;
        loop {
            let mut line = String::new();
            if self.reader.read_line(&mut line)? == 0 {
                bail!("the connection to Chrome closed");
            }
            let v: Value = serde_json::from_str(line.trim_end())?;
            if v.get("id").and_then(Value::as_u64) == Some(id) {
                if let Some(e) = v.get("error").and_then(Value::as_str) {
                    bail!("{e}");
                }
                return Ok(v.get("result").cloned().unwrap_or(Value::Null));
            }
        }
    }

    pub fn alive(&mut self) -> bool {
        self.call("Browser.getVersion", json!({}), None).is_ok()
    }

    fn eval(&mut self, session: &str, expr: &str) -> Result<Value> {
        let r = self.call(
            "Runtime.evaluate",
            json!({"expression": expr, "returnByValue": true}),
            Some(session),
        )?;
        Ok(r.pointer("/result/value").cloned().unwrap_or(Value::Null))
    }

    fn pages(&mut self) -> Result<Vec<Value>> {
        let r = self.call("Target.getTargets", json!({}), None)?;
        Ok(r["targetInfos"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|t| t["type"] == "page")
            .collect())
    }

    fn open(&mut self, url: &str) -> Result<(String, String)> {
        let t = self.call(
            "Target.createTarget",
            json!({"url": url, "background": true}),
            None,
        )?;
        let tid = t["targetId"].as_str().context("no target")?.to_string();
        let sid = self.attach(&tid)?;
        Ok((tid, sid))
    }

    fn attach(&mut self, tid: &str) -> Result<String> {
        let s = self.call(
            "Target.attachToTarget",
            json!({"targetId": tid, "flatten": true}),
            None,
        )?;
        Ok(s["sessionId"].as_str().context("no session")?.to_string())
    }

    fn front(&mut self, tid: &str) {
        let _ = self.call("Target.activateTarget", json!({"targetId": tid}), None);
    }

    /// A scripted click (enough for Claude in Chrome's own buttons, whatever the window state).
    fn js_click(&mut self, session: &str, labels: &[&str]) -> bool {
        let expr = format!(
            "(() => {{ const L = {}; const b = [...document.querySelectorAll('button')].find(b => L.includes(b.innerText.trim()) && !b.disabled); if (!b) return false; b.click(); return true; }})()",
            serde_json::to_string(labels).unwrap_or_default()
        );
        self.eval(session, &expr)
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    /// Wait until `cond` holds, polling.
    fn until(&mut self, wait: Duration, mut cond: impl FnMut(&mut Self) -> bool) -> bool {
        let end = Instant::now() + wait;
        while Instant::now() < end {
            if cond(self) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(300));
        }
        false
    }

    fn close(&mut self, tid: &str) {
        let _ = self.call("Target.closeTarget", json!({"targetId": tid}), None);
    }

    /// The centre of the first enabled button whose text is one of `labels`.
    fn find(&mut self, session: &str, labels: &[&str]) -> Option<(f64, f64)> {
        let expr = format!(
            "(() => {{ const L = {}; const b = [...document.querySelectorAll('button')].find(b => L.includes(b.innerText.trim()) && !b.disabled); if (!b) return null; b.scrollIntoView({{block:'center'}}); const r = b.getBoundingClientRect(); return r.width ? [r.x + r.width/2, r.y + r.height/2] : null; }})()",
            serde_json::to_string(labels).unwrap_or_default()
        );
        let v = self.eval(session, &expr).ok()?;
        Some((v.get(0)?.as_f64()?, v.get(1)?.as_f64()?))
    }

    /// A real click (pages may ignore scripted ones) on a button, waiting for it to appear.
    fn click(&mut self, session: &str, labels: &[&str], wait: Duration) -> Result<bool> {
        let end = Instant::now() + wait;
        while Instant::now() < end {
            if self.find(session, labels).is_some() {
                std::thread::sleep(Duration::from_millis(400));
                if let Some((x, y)) = self.find(session, labels) {
                    for t in ["mouseMoved", "mousePressed", "mouseReleased"] {
                        self.call(
                            "Input.dispatchMouseEvent",
                            json!({"type": t, "x": x, "y": y, "button": "left", "clickCount": 1}),
                            Some(session),
                        )?;
                    }
                    return Ok(true);
                }
            }
            std::thread::sleep(Duration::from_millis(300));
        }
        Ok(false)
    }

    fn text(&mut self, session: &str) -> String {
        self.eval(session, "document.body ? document.body.innerText : ''")
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default()
    }

    // ---- claude.ai cookies ----

    /// The browser's current claude.ai session cookies (CDP format).
    pub fn session_cookies(&mut self) -> Result<Vec<Value>> {
        let r = self.call("Storage.getCookies", json!({}), None)?;
        Ok(r["cookies"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|c| {
                c["domain"]
                    .as_str()
                    .is_some_and(|d| d.trim_start_matches('.') == "claude.ai")
                    && c["name"]
                        .as_str()
                        .is_some_and(|n| crate::web::SESSION_COOKIES.contains(&n))
            })
            .collect())
    }

    /// Replace the claude.ai session cookies by `cookies` (CDP format), touching only those that
    /// differ (an unchanged session stays untouched for claude.ai and Claude in Chrome).
    pub fn set_session(&mut self, cookies: &[Value]) -> Result<()> {
        let key = |c: &Value| {
            format!(
                "{}|{}",
                c["name"].as_str().unwrap_or(""),
                c["domain"].as_str().unwrap_or("").trim_start_matches('.')
            )
        };
        let current = self.session_cookies()?;
        let wanted: std::collections::HashMap<String, &Value> =
            cookies.iter().map(|c| (key(c), c)).collect();
        let gone: Vec<Value> = current
            .iter()
            .filter(|c| !wanted.contains_key(&key(c)))
            .cloned()
            .map(|mut c| {
                c["expires"] = json!(1);
                c["value"] = json!("");
                clean(c)
            })
            .collect();
        if !gone.is_empty() {
            self.call("Storage.setCookies", json!({"cookies": gone}), None)?;
        }
        let set: Vec<Value> = cookies
            .iter()
            .filter(|c| {
                !current
                    .iter()
                    .any(|x| key(x) == key(c) && x["value"] == c["value"])
            })
            .cloned()
            .map(clean)
            .collect();
        if !set.is_empty() {
            self.call("Storage.setCookies", json!({"cookies": set}), None)?;
        }
        Ok(())
    }

    /// The visible buttons of a page, for an error message (no addresses).
    fn buttons(&mut self, session: &str) -> String {
        self.eval(session, "JSON.stringify([...document.querySelectorAll('button')].map(b => b.innerText.trim()).filter(Boolean).slice(0, 8))")
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default()
    }

    /// Reload the claude.ai tabs (after their session changed).
    pub fn reload_claude_tabs(&mut self) -> Result<()> {
        for t in self.pages()? {
            if t["url"]
                .as_str()
                .is_some_and(|u| u.starts_with("https://claude.ai/"))
            {
                if let Some(tid) = t["targetId"].as_str().map(str::to_string) {
                    if let Ok(sid) = self.attach(&tid) {
                        let _ = self.call("Page.reload", json!({}), Some(&sid));
                    }
                }
            }
        }
        Ok(())
    }

    /// Open claude.ai's sign-in page in front.
    pub fn open_login(&mut self) -> Result<()> {
        let t = self.call(
            "Target.createTarget",
            json!({"url": "https://claude.ai/login"}),
            None,
        )?;
        if let Some(tid) = t["targetId"].as_str() {
            self.front(tid);
        }
        Ok(())
    }

    // ---- Claude in Chrome ----

    /// Sign Claude in Chrome out and in again with claude.ai's current session, approving the
    /// consent for `email` only. Returns when its options page shows that account.
    pub fn resign_claude_in_chrome(&mut self, email: &str) -> Result<()> {
        let options = format!("chrome-extension://{CIC_ID}/options.html");
        let (otid, osid) = self.open(&options)?;
        let result = (|| -> Result<()> {
            self.front(&otid);
            std::thread::sleep(Duration::from_millis(1500));
            // The page shows either its sign-out or its sign-in button once loaded.
            if !self.until(Duration::from_secs(15), |c| {
                c.find(&osid, LOGOUT).is_some() || c.find(&osid, SIGNIN).is_some()
            }) {
                let shown = self.buttons(&osid);
                bail!("Claude in Chrome's options did not load (its page shows {shown})");
            }
            // Sign out: a scripted click, then a real one if the page did not change.
            if self.find(&osid, LOGOUT).is_some() {
                self.js_click(&osid, LOGOUT);
                if !self.until(Duration::from_secs(6), |c| c.find(&osid, SIGNIN).is_some()) {
                    let _ = self.call("Page.bringToFront", json!({}), Some(&osid));
                    self.click(&osid, LOGOUT, Duration::from_secs(5))?;
                    self.until(Duration::from_secs(8), |c| c.find(&osid, SIGNIN).is_some());
                }
            }
            if !self.until(Duration::from_secs(10), |c| c.find(&osid, SIGNIN).is_some()) {
                let shown = self.buttons(&osid);
                bail!("Claude in Chrome did not offer to sign in (its page shows {shown})");
            }
            // Sign in: it opens claude.ai's consent page.
            let consent_open = |c: &mut Self| {
                c.pages()
                    .map(|p| {
                        p.iter().any(|t| {
                            t["url"].as_str().is_some_and(|u| {
                                u.contains("/oauth/authorize") && u.contains(CIC_CLIENT)
                            })
                        })
                    })
                    .unwrap_or(false)
            };
            self.js_click(&osid, SIGNIN);
            if !self.until(Duration::from_secs(8), consent_open) {
                let _ = self.call("Page.bringToFront", json!({}), Some(&osid));
                self.click(&osid, SIGNIN, Duration::from_secs(5))?;
            }
            let mut done = false;
            for _attempt in 0..2 {
                let consent = self.wait_page(
                    |u| u.contains("/oauth/authorize") && u.contains(CIC_CLIENT),
                    Duration::from_secs(20),
                )?;
                let Some(ctid) = consent else {
                    bail!("the sign-in page did not open")
                };
                let csid = self.attach(&ctid)?;
                self.front(&ctid);
                let _ = self.call("Page.bringToFront", json!({}), Some(&csid));
                std::thread::sleep(Duration::from_millis(1000));
                // Approve only for the expected account (the page names it).
                let end = Instant::now() + Duration::from_secs(15);
                while self.find(&csid, APPROVE).is_none() && Instant::now() < end {
                    std::thread::sleep(Duration::from_millis(300));
                }
                if !self
                    .text(&csid)
                    .to_lowercase()
                    .contains(&email.to_lowercase())
                {
                    self.close(&ctid);
                    bail!("claude.ai is not signed in with {email} in this browser");
                }
                self.click(&csid, APPROVE, Duration::from_secs(10))?;
                let end = Instant::now() + Duration::from_secs(15);
                while Instant::now() < end {
                    std::thread::sleep(Duration::from_millis(500));
                    if !self.pages()?.iter().any(|t| {
                        t["targetId"] == ctid.as_str()
                            && t["url"]
                                .as_str()
                                .is_some_and(|u| u.contains("/oauth/authorize"))
                    }) {
                        done = true;
                        break;
                    }
                }
                if done {
                    break;
                }
            }
            if !done {
                bail!("the sign-in did not finish");
            }
            self.call("Page.reload", json!({}), Some(&osid))?;
            std::thread::sleep(Duration::from_millis(800));
            let lower = email.to_lowercase();
            self.until(Duration::from_secs(15), |c| {
                c.find(&osid, LOGOUT).is_some() && c.text(&osid).to_lowercase().contains(&lower)
            });
            let text = self.text(&osid).to_lowercase();
            if self.find(&osid, LOGOUT).is_none() || !text.contains(&email.to_lowercase()) {
                let shown = self.buttons(&osid);
                bail!("Claude in Chrome does not show {email} after signing in (its page shows {shown})");
            }
            Ok(())
        })();
        self.close(&otid);
        result
    }

    fn wait_page(&mut self, pred: impl Fn(&str) -> bool, wait: Duration) -> Result<Option<String>> {
        let end = Instant::now() + wait;
        while Instant::now() < end {
            if let Some(t) = self
                .pages()?
                .into_iter()
                .find(|t| t["url"].as_str().is_some_and(&pred))
            {
                return Ok(t["targetId"].as_str().map(str::to_string));
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        Ok(None)
    }
}

/// A cookie as `Storage.setCookies` takes it.
fn clean(c: Value) -> Value {
    let mut o = json!({
        "name": c["name"], "value": c["value"], "domain": c["domain"], "path": c.get("path").cloned().unwrap_or(json!("/")),
        "secure": c.get("secure").cloned().unwrap_or(json!(true)), "httpOnly": c.get("httpOnly").cloned().unwrap_or(json!(false)),
    });
    if let Some(s) = c.get("sameSite").and_then(Value::as_str) {
        o["sameSite"] = json!(s);
    }
    if let Some(e) = c
        .get("expires")
        .and_then(Value::as_f64)
        .filter(|e| *e > 0.0)
    {
        o["expires"] = json!(e);
    }
    o
}

/// CDP's cookie format to the extension's (how sessions are kept in the vault).
pub fn to_extension(c: &Value) -> Value {
    let same = match c["sameSite"].as_str() {
        Some("Lax") => "lax",
        Some("Strict") => "strict",
        Some("None") => "no_restriction",
        _ => "unspecified",
    };
    let expires = c["expires"].as_f64().unwrap_or(-1.0);
    let domain = c["domain"].as_str().unwrap_or("claude.ai");
    json!({"name": c["name"], "value": c["value"], "domain": domain, "path": c.get("path").cloned().unwrap_or(json!("/")),
        "secure": c["secure"], "httpOnly": c["httpOnly"], "sameSite": same, "hostOnly": !domain.starts_with('.'),
        "session": expires <= 0.0, "expirationDate": if expires > 0.0 { json!(expires) } else { Value::Null }})
}

/// The extension's cookie format (chrome.cookies) to CDP's.
pub fn from_extension(c: &Value) -> Value {
    let same = match c["sameSite"].as_str() {
        Some("lax") => Some("Lax"),
        Some("strict") => Some("Strict"),
        Some("no_restriction") => Some("None"),
        _ => None,
    };
    let mut o = json!({
        "name": c["name"], "value": c["value"], "path": c.get("path").cloned().unwrap_or(json!("/")),
        "secure": c.get("secure").cloned().unwrap_or(json!(true)), "httpOnly": c.get("httpOnly").cloned().unwrap_or(json!(false)),
        "domain": c["domain"],
    });
    if c["hostOnly"].as_bool() == Some(true) {
        o["domain"] = json!(c["domain"]
            .as_str()
            .unwrap_or("claude.ai")
            .trim_start_matches('.'));
    }
    if let Some(s) = same {
        o["sameSite"] = json!(s);
    }
    if c["session"].as_bool() != Some(true) {
        if let Some(e) = c["expirationDate"].as_f64() {
            o["expires"] = json!(e);
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_cookie_to_cdp() {
        let c = json!({"name": "sessionKey", "value": "v", "domain": ".claude.ai", "path": "/", "secure": true, "httpOnly": true,
            "sameSite": "lax", "hostOnly": false, "session": false, "expirationDate": 4102444800.0});
        let o = from_extension(&c);
        assert_eq!(o["sameSite"], "Lax");
        assert_eq!(o["domain"], ".claude.ai");
        assert_eq!(o["expires"], 4102444800.0);
        let h = from_extension(
            &json!({"name": "lastActiveOrg", "value": "o", "domain": "claude.ai", "hostOnly": true, "session": true}),
        );
        assert_eq!(h["domain"], "claude.ai");
        assert!(h.get("expires").is_none());
    }
}
