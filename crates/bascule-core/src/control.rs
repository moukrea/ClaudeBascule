//! The daemon's local socket for the mini-app and the CLI: `<state>/bascule.sock` (0600).
//! One JSON object per line: `{id, method, params}` → `{id, ok, result|error}`.
//! Methods: state, switch, reread, resume.add, resume.send, resume.dismiss, settings.get, settings.set,
//! login.start, login.code, login.cancel.

use crate::config::{AccountList, Settings};
use crate::daemon::Cmd;
use crate::login::{self, Login};
use crate::observe::log;
use crate::observe::now_ms;
use crate::paths::Paths;
use crate::platform::{self, Stream};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub fn socket_path(paths: &Paths) -> PathBuf {
    paths.state.join("bascule.sock")
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct LoginState {
    pub account: Option<String>,
    pub status: String,
    pub url: Option<String>,
    pub lines: Vec<String>,
    pub message: Option<String>,
}

struct Ctx {
    paths: Paths,
    shared: Arc<Mutex<Value>>,
    tx: Mutex<Sender<Cmd>>,
    login: Arc<Mutex<LoginState>>,
    child: Arc<Mutex<Option<Login>>>,
    /// Windows: the key every request must carry.
    key: Option<String>,
    updates: Arc<Mutex<crate::update::UpdateState>>,
    web: Arc<Mutex<crate::web::WebHub>>,
}

/// Windows (no Unix sockets): `{port, key, pid}` of a 127.0.0.1 listener; every line carries the key.
pub fn json_path(paths: &Paths) -> PathBuf {
    paths.state.join("control.json")
}

pub fn serve(
    paths: Paths,
    shared: Arc<Mutex<Value>>,
    tx: Sender<Cmd>,
    updates: Arc<Mutex<crate::update::UpdateState>>,
    web: Arc<Mutex<crate::web::WebHub>>,
) -> Result<()> {
    // Windows has no Unix sockets: a 127.0.0.1 port, and a key every request must carry.
    #[cfg(not(unix))]
    let (listener, key) = {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).context("bind 127.0.0.1")?;
        let key = platform::random_key();
        let port = listener.local_addr()?.port();
        crate::store::atomic_write(
            &json_path(&paths),
            &json!({"port": port, "key": key, "pid": std::process::id()}),
        )?;
        (listener, Some(key))
    };
    #[cfg(unix)]
    let key = None;
    let ctx = Arc::new(Ctx {
        paths: paths.clone(),
        shared,
        tx: Mutex::new(tx),
        login: Default::default(),
        child: Default::default(),
        key,
        updates,
        web,
    });
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = socket_path(&paths);
        let _ = std::fs::remove_file(&path);
        let listener = std::os::unix::net::UnixListener::bind(&path)
            .with_context(|| format!("bind {}", path.display()))?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let _ = client(&ctx, Stream::Unix(stream));
                });
            }
        });
    }
    #[cfg(not(unix))]
    {
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let _ = client(&ctx, Stream::Tcp(stream));
                });
            }
        });
    }
    Ok(())
}

fn client(ctx: &Arc<Ctx>, stream: Stream) -> Result<()> {
    let mut w = stream.try_clone()?;
    for line in BufReader::new(stream).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let parsed = serde_json::from_str::<Value>(&line);
        if let (Some(key), Ok(m)) = (&ctx.key, &parsed) {
            if m.get("key").and_then(Value::as_str) != Some(key.as_str()) {
                writeln!(
                    w,
                    "{}",
                    json!({"id": m.get("id"), "ok": false, "error": "bad key"})
                )?;
                continue;
            }
        }
        let (id, reply) = match parsed {
            Ok(m) => {
                let id = m.get("id").cloned().unwrap_or(Value::Null);
                let method = m
                    .get("method")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let params = m.get("params").cloned().unwrap_or(json!({}));
                (id, dispatch(ctx, &method, &params))
            }
            Err(e) => (Value::Null, Err(anyhow::anyhow!("invalid JSON: {e}"))),
        };
        let out = match reply {
            Ok(result) => json!({"id": id, "ok": true, "result": result}),
            Err(e) => json!({"id": id, "ok": false, "error": format!("{e:#}")}),
        };
        writeln!(w, "{out}")?;
    }
    Ok(())
}

fn ask(
    ctx: &Ctx,
    make: impl FnOnce(Sender<Result<Value, String>>) -> Cmd,
    timeout: Duration,
) -> Result<Value> {
    let (tx, rx) = channel();
    ctx.tx
        .lock()
        .unwrap()
        .send(make(tx))
        .map_err(|_| anyhow::anyhow!("daemon stopped"))?;
    match rx.recv_timeout(timeout) {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => bail!(e),
        Err(_) => bail!("the daemon did not answer in {}s", timeout.as_secs()),
    }
}

fn strings(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

fn dispatch(ctx: &Arc<Ctx>, method: &str, p: &Value) -> Result<Value> {
    let long = Duration::from_secs(120);
    match method {
        "state" => {
            let mut v = ctx.shared.lock().unwrap().clone();
            v["login"] = serde_json::to_value(&*ctx.login.lock().unwrap())?;
            Ok(v)
        }
        "switch" => {
            let to = p
                .get("to")
                .and_then(Value::as_str)
                .context("switch needs 'to'")?
                .to_string();
            ask(ctx, |reply| Cmd::Switch { to, reply }, long)
        }
        "reread" => ask(ctx, |reply| Cmd::Reread { reply }, long),
        "resume.send" => {
            let sessions = strings(p, "sessions");
            let texts: BTreeMap<String, String> = p
                .get("texts")
                .and_then(Value::as_object)
                .map(|m| {
                    m.iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                        .collect()
                })
                .unwrap_or_default();
            ask(
                ctx,
                |reply| Cmd::Send {
                    sessions,
                    texts,
                    reply,
                },
                long,
            )
        }
        "resume.add" => {
            let sessions = strings(p, "sessions");
            ask(ctx, |reply| Cmd::Add { sessions, reply }, long)
        }
        "resume.dismiss" => {
            let sessions = strings(p, "sessions");
            ask(ctx, |reply| Cmd::Dismiss { sessions, reply }, long)
        }
        "settings.get" => Ok(serde_json::to_value(Settings::load(
            &ctx.paths.settings_file(),
        )?)?),
        "settings.set" => {
            // Merge the given keys into the current settings, check them, write the TOML file.
            let mut cur = serde_json::to_value(Settings::load(&ctx.paths.settings_file())?)?;
            if let (Some(c), Some(n)) = (
                cur.as_object_mut(),
                p.get("settings").and_then(Value::as_object),
            ) {
                for (k, v) in n {
                    c.insert(k.clone(), v.clone());
                }
            }
            let s: Settings = serde_json::from_value(cur).context("invalid settings")?;
            if !(1.0..=100.0).contains(&s.threshold_percent) {
                bail!("threshold_percent must be between 1 and 100");
            }
            crate::store::private_dir(&ctx.paths.config_dir)?;
            let text = toml::to_string_pretty(&s)?;
            let tmp = ctx.paths.settings_file().with_extension("toml.tmp");
            std::fs::write(&tmp, text)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
            }
            std::fs::rename(&tmp, ctx.paths.settings_file())?;
            log(&ctx.paths, "settings.set", serde_json::to_value(&s)?);
            Ok(serde_json::to_value(s)?)
        }
        "login.start" => login_start(
            ctx,
            p.get("account")
                .and_then(Value::as_str)
                .context("login.start needs 'account'")?,
        ),
        "accounts.add" => {
            let email = p
                .get("email")
                .and_then(Value::as_str)
                .context("accounts.add needs 'email'")?;
            let mut list = AccountList::load(&ctx.paths.accounts_file())?;
            let name = list.add(email)?;
            list.save(&ctx.paths.accounts_file())?;
            log(&ctx.paths, "accounts.add", json!({"name": name}));
            Ok(json!({"name": name}))
        }
        "accounts.remove" => {
            let name = p
                .get("name")
                .and_then(Value::as_str)
                .context("accounts.remove needs 'name'")?;
            let mut list = AccountList::load(&ctx.paths.accounts_file())?;
            let account = list.by_name(name).cloned().context("unknown account")?;
            if ctx
                .paths
                .main
                .email()
                .map(|e| e.eq_ignore_ascii_case(&account.email))
                .unwrap_or(false)
            {
                bail!("this account is the active one: switch to another account first");
            }
            // Its tokens on this machine go with it (Claude Code itself is not touched).
            let slot = ctx.paths.account_slot(name);
            let _ = slot.set_oauth(None);
            if let Some((service, acc)) = &slot.keychain {
                crate::platform::keychain_delete(service, acc);
            }
            let _ = std::fs::remove_dir_all(&slot.dir);
            list.remove(name);
            list.save(&ctx.paths.accounts_file())?;
            log(&ctx.paths, "accounts.remove", json!({"name": name}));
            Ok(json!({"ok": true}))
        }
        "jaunt.enroll" => {
            // jaunt asks the user on a paired device (up to a few minutes).
            let manifest = ctx.paths.state.join("jaunt-manifest.json");
            crate::store::atomic_write(
                &manifest,
                &json!({"title": "Claude Bascule", "description": "Switches the Claude account before a usage limit and resumes the sessions a limit blocked.",
                "rights": ["observe", "answer"], "topics": ["sessions", "rich.state"]}),
            )?;
            let out = crate::platform::jaunt()
                .args(["agents", "enroll", "bascule", "--manifest"])
                .arg(&manifest)
                .arg("--out")
                .arg(&ctx.paths.jaunt_key)
                .output()
                .context("run jaunt")?;
            if !out.status.success() || !ctx.paths.jaunt_key.exists() {
                bail!(
                    "{}",
                    String::from_utf8_lossy(&out.stderr)
                        .trim()
                        .lines()
                        .last()
                        .unwrap_or("jaunt refused the enrollment")
                );
            }
            log(&ctx.paths, "jaunt.enrolled", json!({}));
            Ok(json!({"ok": true}))
        }
        "web.hello" => {
            let mut hub = ctx.web.lock().unwrap();
            hub.seen_ms = now_ms();
            hub.browser = p.get("browser").and_then(Value::as_str).map(str::to_string);
            hub.version = p.get("version").and_then(Value::as_str).map(str::to_string);
            Ok(json!({"ok": true}))
        }
        // Long poll from the browser: the next command, or null after `timeout_ms`.
        "web.next" => {
            let timeout = p
                .get("timeout_ms")
                .and_then(Value::as_u64)
                .unwrap_or(20_000)
                .min(60_000);
            let until = std::time::Instant::now() + Duration::from_millis(timeout);
            loop {
                {
                    let mut hub = ctx.web.lock().unwrap();
                    hub.seen_ms = now_ms();
                    if let Some(c) = hub.queue.pop_front() {
                        return Ok(c);
                    }
                }
                if std::time::Instant::now() >= until {
                    return Ok(Value::Null);
                }
                std::thread::sleep(Duration::from_millis(250));
            }
        }
        // The browser's claude.ai session (sent when it changes): saved for its account.
        "web.capture" => {
            let list = AccountList::load(&ctx.paths.accounts_file())?;
            let cookies = crate::web::session_cookies(
                p.get("cookies")
                    .and_then(Value::as_array)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
            );
            let email = p.get("email").and_then(Value::as_str);
            let signed_in = cookies.iter().any(|c| {
                matches!(
                    c.get("name").and_then(Value::as_str),
                    Some("sessionKey") | Some("sessionKeyV3")
                )
            });
            let mut hub = ctx.web.lock().unwrap();
            hub.seen_ms = now_ms();
            if !signed_in {
                hub.account = None;
                hub.unknown_email = None;
                return Ok(json!({"account": null}));
            }
            match crate::web::account_for(&ctx.paths, &list, email, &cookies) {
                Some(name) => {
                    let email = list
                        .by_name(&name)
                        .map(|a| a.email.clone())
                        .unwrap_or_default();
                    crate::web::save(
                        &ctx.paths,
                        &name,
                        &crate::web::WebSession {
                            email,
                            cookies,
                            saved_at: now_ms(),
                        },
                    )?;
                    if hub.account.as_deref() != Some(name.as_str()) {
                        hub.notes
                            .push_back(("web.account".into(), json!({"account": name})));
                    }
                    hub.account = Some(name.clone());
                    hub.unknown_email = None;
                    Ok(json!({"account": name}))
                }
                None => {
                    hub.account = None;
                    hub.unknown_email = email.map(str::to_string);
                    Ok(json!({"account": null}))
                }
            }
        }
        "web.result" => {
            let ok = p.get("ok").and_then(Value::as_bool).unwrap_or(false);
            let mut hub = ctx.web.lock().unwrap();
            hub.last_error = if ok {
                None
            } else {
                p.get("error").and_then(Value::as_str).map(str::to_string)
            };
            let note = json!({"account": p.get("to"), "error": hub.last_error});
            let kind = if ok {
                "web.switched"
            } else {
                "web.switch_failed"
            };
            hub.notes.push_back((kind.into(), note));
            Ok(json!({"ok": true}))
        }
        "web.switch" => {
            let to = p
                .get("to")
                .and_then(Value::as_str)
                .context("web.switch needs 'to'")?;
            let mut hub = ctx.web.lock().unwrap();
            if !hub.connected(now_ms()) {
                bail!("the browser extension is not connected");
            }
            hub.push_switch(&ctx.paths, to)?;
            Ok(json!({"ok": true}))
        }
        "web.login" => {
            let name = p
                .get("account")
                .and_then(Value::as_str)
                .context("web.login needs 'account'")?;
            let list = AccountList::load(&ctx.paths.accounts_file())?;
            let a = list.by_name(name).context("unknown account")?;
            let mut hub = ctx.web.lock().unwrap();
            if !hub.connected(now_ms()) {
                bail!("the browser extension is not connected");
            }
            hub.push_login(&a.email);
            Ok(json!({"ok": true}))
        }
        "web.forget" => {
            let name = p
                .get("account")
                .and_then(Value::as_str)
                .context("web.forget needs 'account'")?;
            crate::web::forget(&ctx.paths, name);
            Ok(json!({"ok": true}))
        }
        "update.check" => {
            let st = crate::update::check(env!("CARGO_PKG_VERSION"));
            *ctx.updates.lock().unwrap() = st.clone();
            Ok(serde_json::to_value(st)?)
        }
        "login.code" => {
            let code = p
                .get("code")
                .and_then(Value::as_str)
                .context("login.code needs 'code'")?;
            let mut g = ctx.child.lock().unwrap();
            g.as_mut()
                .context("no sign-in running")?
                .submit_code(code)?;
            Ok(json!({"ok": true}))
        }
        "login.cancel" => {
            if let Some(l) = ctx.child.lock().unwrap().as_mut() {
                let _ = l.child.kill();
            }
            let mut s = ctx.login.lock().unwrap();
            s.status = "cancelled".into();
            Ok(json!({"ok": true}))
        }
        _ => bail!("unknown method '{method}'"),
    }
}

fn login_start(ctx: &Arc<Ctx>, name: &str) -> Result<Value> {
    if ctx.child.lock().unwrap().is_some() {
        bail!("a sign-in is already running");
    }
    let list = AccountList::load(&ctx.paths.accounts_file())?;
    let account = list
        .by_name(name)
        .with_context(|| format!("unknown account '{name}'"))?
        .clone();
    let l = Login::start(&ctx.paths, &account)?;
    *ctx.login.lock().unwrap() = LoginState {
        account: Some(name.into()),
        status: "running".into(),
        ..Default::default()
    };
    *ctx.child.lock().unwrap() = Some(l);
    let ctx2 = ctx.clone();
    std::thread::spawn(move || {
        let finished = loop {
            std::thread::sleep(Duration::from_millis(200));
            let mut g = ctx2.child.lock().unwrap();
            let Some(l) = g.as_mut() else { break None };
            while let Ok(line) = l.lines.try_recv() {
                let mut s = ctx2.login.lock().unwrap();
                if s.url.is_none() {
                    s.url = Login::url_in(&line);
                }
                s.lines.push(line);
            }
            if let Ok(Some(status)) = l.child.try_wait() {
                break Some(status.success());
            }
        };
        *ctx2.child.lock().unwrap() = None;
        let mut result = match finished {
            Some(true) => login::verify(&ctx2.paths, &account).map(|_| "done".to_string()),
            _ => Err(anyhow::anyhow!("the sign-in did not finish")),
        };
        // Signing the active account in again: its new tokens go to the main slot.
        if result.is_ok()
            && ctx2
                .paths
                .main
                .email()
                .map(|e| e.eq_ignore_ascii_case(&account.email))
                .unwrap_or(false)
        {
            result = ask(
                &ctx2,
                |reply| Cmd::AdoptLogin {
                    account: account.name.clone(),
                    reply,
                },
                Duration::from_secs(60),
            )
            .map(|_| "done".to_string());
        }
        // Read the account at once: its card shows the new sign-in instead of waiting for the
        // next regular read of resting accounts.
        if result.is_ok() {
            let (tx, _rx) = channel();
            let _ = ctx2.tx.lock().unwrap().send(Cmd::Reread { reply: tx });
        }
        let mut s = ctx2.login.lock().unwrap();
        if s.status == "cancelled" {
            return;
        }
        match result {
            Ok(st) => {
                s.status = st;
                log(&ctx2.paths, "login", json!({"account": account.name}));
            }
            Err(e) => {
                s.status = "error".into();
                s.message = Some(format!("{e:#}"));
            }
        }
    });
    Ok(json!({"ok": true}))
}

/// A one-shot client, for the CLI and the mini-app.
pub fn call(paths: &Paths, method: &str, params: Value) -> Result<Value> {
    let ep = platform::connect(&socket_path(paths), &json_path(paths))
        .context("the Claude Bascule service is not running")?;
    let mut s = ep.stream;
    s.set_read_timeout(Some(Duration::from_secs(150)))?;
    writeln!(
        s,
        "{}",
        platform::keyed(
            json!({"id": 1, "method": method, "params": params}),
            &ep.key
        )
    )?;
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line)?;
    let v: Value = serde_json::from_str(&line)?;
    if v.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(v.get("result").cloned().unwrap_or(Value::Null))
    } else {
        bail!(
            "{}",
            v.get("error").and_then(Value::as_str).unwrap_or("refused")
        )
    }
}

/// Whether the service answers on its local endpoint.
pub fn is_running(paths: &Paths) -> bool {
    platform::connect(&socket_path(paths), &json_path(paths)).is_ok()
}
