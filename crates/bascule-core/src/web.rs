//! claude.ai in the browser: the "Claude Bascule Connector" extension reports the claude.ai session
//! of the browser (its cookies) and applies the one the service asks for, through the native
//! messaging host (`bascule` started by the browser). Each account's web session lives next to
//! its tokens in the vault (`web-session.json`, private), and is never logged or printed.

use crate::config::AccountList;
use crate::paths::Paths;
use crate::store::atomic_write;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// The cookies that make a claude.ai session (both `claude.ai` and `.claude.ai` hosts).
pub const SESSION_COOKIES: &[&str] = &[
    "sessionKey",
    "sessionKeyV3",
    "sessionKeyLC",
    "sessionKeyV3LC",
    "routingHint",
    "lastActiveOrg",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WebSession {
    pub email: String,
    /// Cookies as the extension's `chrome.cookies` API gives them.
    pub cookies: Vec<Value>,
    pub saved_at: i64,
}

fn file(paths: &Paths, name: &str) -> PathBuf {
    paths.vault.join(name).join("web-session.json")
}

pub fn load(paths: &Paths, name: &str) -> Option<WebSession> {
    let text = std::fs::read_to_string(file(paths, name)).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn has(paths: &Paths, name: &str) -> bool {
    file(paths, name).exists()
}

pub fn save(paths: &Paths, name: &str, s: &WebSession) -> Result<()> {
    crate::store::private_dir(&paths.vault.join(name))?;
    atomic_write(&file(paths, name), &serde_json::to_value(s)?)
}

pub fn forget(paths: &Paths, name: &str) {
    let _ = std::fs::remove_file(file(paths, name));
}

/// Keep only session cookies, whatever the extension sent.
pub fn session_cookies(cookies: &[Value]) -> Vec<Value> {
    cookies
        .iter()
        .filter(|c| {
            c.get("name")
                .and_then(Value::as_str)
                .is_some_and(|n| SESSION_COOKIES.contains(&n))
                && c.get("domain")
                    .and_then(Value::as_str)
                    .is_some_and(|d| d.trim_start_matches('.') == "claude.ai")
        })
        .cloned()
        .collect()
}

/// Which listed account a browser session belongs to: by the email the extension read, else by
/// the `lastActiveOrg` cookie against each account's organisation (from its `.claude.json`).
pub fn account_for(
    paths: &Paths,
    list: &AccountList,
    email: Option<&str>,
    cookies: &[Value],
) -> Option<String> {
    if let Some(e) = email.filter(|e| !e.is_empty()) {
        return list.by_email(e).map(|a| a.name.clone());
    }
    let org = cookies.iter().find_map(|c| {
        (c.get("name").and_then(Value::as_str) == Some("lastActiveOrg"))
            .then(|| c.get("value").and_then(Value::as_str))
            .flatten()
    })?;
    for a in &list.accounts {
        let slot = if paths
            .main
            .email()
            .is_some_and(|e| e.eq_ignore_ascii_case(&a.email))
        {
            paths.main.clone()
        } else {
            paths.account_slot(&a.name)
        };
        let acct_org = slot.account().ok().flatten().and_then(|o| {
            o.get("organizationUuid")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
        if acct_org.as_deref() == Some(org) {
            return Some(a.name.clone());
        }
    }
    None
}

/// What the browser side knows and is asked, shared by the daemon and the control endpoint.
#[derive(Default)]
pub struct WebHub {
    /// Commands for the extension, taken by `web.next`.
    pub queue: VecDeque<Value>,
    /// Last time the extension asked for commands (it does so constantly while connected).
    pub seen_ms: i64,
    pub browser: Option<String>,
    pub version: Option<String>,
    /// The account claude.ai is signed in with in that browser, None when signed out or unknown.
    pub account: Option<String>,
    /// The email of a session that is not one of the listed accounts.
    pub unknown_email: Option<String>,
    pub last_error: Option<String>,
    /// Events for the log, taken by the daemon.
    pub notes: VecDeque<(String, Value)>,
    /// The account the Claude in Chrome extension is signed in with, as last moved by Claude
    /// Bascule (it cannot be read from the extension), and a move in progress.
    pub cic_account: Option<String>,
    pub cic_flow: Option<CicFlow>,
    /// The browser driven through its DevTools protocol (one connection kept open), and its state:
    /// "off" (remote debugging not allowed), "available", "connecting", "connected", or an error.
    pub cdp: Arc<Mutex<Option<crate::cdp::Cdp>>>,
    pub cdp_state: String,
    /// Chrome's accessibility for unattended moves (see a11y): "unsupported", "off", "restart", "ready".
    pub a11y_state: String,
}

/// Moving Claude in Chrome to `to`: claude.ai carries `to`'s session while the user signs the
/// extension out and in again; then claude.ai goes back to `restore` (unless it should stay).
#[derive(Clone, Debug, Serialize)]
pub struct CicFlow {
    pub id: i64,
    pub to: String,
    pub restore: Option<String>,
    pub started_ms: i64,
    /// "waiting" (the user signs out and in), "working" (Claude Bascule does it), "done", "failed".
    pub stage: String,
    pub error: Option<String>,
    /// "auto" (Chrome driven by Claude Bascule) or "guided".
    pub mode: String,
}

fn cic_file(paths: &Paths) -> PathBuf {
    paths.state.join("claude-in-chrome.json")
}

pub fn load_cic(paths: &Paths) -> Option<String> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(cic_file(paths)).ok()?).ok()?;
    v.get("account").and_then(Value::as_str).map(str::to_string)
}

pub fn save_cic(paths: &Paths, account: &str) {
    let _ = atomic_write(&cic_file(paths), &json!({"account": account}));
}

impl WebHub {
    pub fn connected(&self, now_ms: i64) -> bool {
        now_ms - self.seen_ms < 45_000
    }

    /// Ask the browser to use `name`'s session (it saves its current one first).
    pub fn push_switch(&mut self, paths: &Paths, name: &str) -> Result<()> {
        let s = load(paths, name).with_context(|| {
            format!("no claude.ai session saved for {name}: sign it in in the browser once")
        })?;
        self.queue
            .retain(|c| c.get("type").and_then(Value::as_str) != Some("switch"));
        self.queue.push_back(
            json!({"type": "switch", "to": name, "email": s.email, "cookies": s.cookies}),
        );
        Ok(())
    }

    /// Start moving Claude in Chrome to `name`: claude.ai gets `name`'s session, the extension's
    /// options open, and the Connector watches for its new sign-in.
    pub fn push_cic(
        &mut self,
        paths: &Paths,
        name: &str,
        keep_web: bool,
        now_ms: i64,
    ) -> Result<i64> {
        let s = load(paths, name).with_context(|| {
            format!("no claude.ai session saved for {name}: sign it in in the browser once")
        })?;
        let restore = if keep_web {
            None
        } else {
            self.account.clone().filter(|a| a != name)
        };
        let id = now_ms;
        self.cic_flow = Some(CicFlow {
            id,
            to: name.to_string(),
            restore,
            started_ms: now_ms,
            stage: "waiting".into(),
            error: None,
            mode: "guided".into(),
        });
        self.queue
            .retain(|c| c.get("type").and_then(Value::as_str) != Some("switch"));
        self.queue.push_back(
            json!({"type": "cic_begin", "id": id, "to": name, "email": s.email, "cookies": s.cookies}),
        );
        Ok(id)
    }

    /// The Claude in Chrome move ended (signed in again, cancelled or failed): put claude.ai back.
    pub fn end_cic(&mut self, paths: &Paths, ok: bool, error: Option<String>) {
        let Some(mut flow) = self.cic_flow.take() else {
            return;
        };
        if ok {
            self.cic_account = Some(flow.to.clone());
            save_cic(paths, &flow.to);
        }
        if let Some(back) = flow.restore.clone() {
            let _ = self.push_switch(paths, &back);
        }
        // The pages opened for the move go away.
        self.queue.push_back(json!({"type": "cic_cleanup"}));
        flow.stage = if ok { "done" } else { "failed" }.into();
        flow.error = error;
        self.notes.push_back((
            if ok { "cic.switched" } else { "cic.failed" }.into(),
            json!({"account": flow.to, "error": flow.error}),
        ));
        // Kept a moment so the window can show the outcome.
        self.cic_flow = Some(flow);
    }

    /// Ask the browser to sign out of claude.ai (keeping the current session) and open the
    /// sign-in page, so the user signs `email` in.
    pub fn push_login(&mut self, email: &str) {
        self.queue
            .push_back(json!({"type": "login", "email": email}));
    }
}

/// Connect to the browser's DevTools (Chrome asks the user to allow it), keeping the connection.
pub fn cdp_connect(web: &Arc<Mutex<WebHub>>) -> Result<()> {
    let handle = web.lock().unwrap().cdp.clone();
    let mut cdp = handle.lock().unwrap();
    if cdp.as_mut().is_some_and(|c| c.alive()) {
        web.lock().unwrap().cdp_state = "connected".into();
        return Ok(());
    }
    *cdp = None;
    web.lock().unwrap().cdp_state = "connecting".into();
    match crate::cdp::Cdp::connect() {
        Ok(c) => {
            *cdp = Some(c);
            web.lock().unwrap().cdp_state = "connected".into();
            Ok(())
        }
        Err(e) => {
            web.lock().unwrap().cdp_state = if crate::cdp::endpoint().is_some() {
                format!("{e:#}")
            } else {
                "off".into()
            };
            Err(e)
        }
    }
}

/// Save the browser's current claude.ai session for its account, read through Chrome's DevTools
/// (the same as a capture by the extension). Returns the account, when it is one of the list.
pub fn cdp_capture(
    cdp: &mut crate::cdp::Cdp,
    paths: &Paths,
    now_ms: i64,
) -> Result<Option<String>> {
    let cookies: Vec<Value> = cdp
        .session_cookies()?
        .iter()
        .map(crate::cdp::to_extension)
        .collect();
    if !cookies.iter().any(|c| {
        matches!(
            c["name"].as_str(),
            Some("sessionKey") | Some("sessionKeyV3")
        )
    }) {
        return Ok(None);
    }
    let list = crate::config::AccountList::load(&paths.accounts_file())?;
    let Some(name) = account_for(paths, &list, None, &cookies) else {
        return Ok(None);
    };
    let email = list
        .by_name(&name)
        .map(|a| a.email.clone())
        .unwrap_or_default();
    save(
        paths,
        &name,
        &WebSession {
            email,
            cookies,
            saved_at: now_ms,
        },
    )?;
    Ok(Some(name))
}

/// claude.ai to `name`'s saved session through Chrome's DevTools (when the extension is absent).
pub fn cdp_switch(web: &Arc<Mutex<WebHub>>, paths: &Paths, name: &str) -> Result<()> {
    let session = load(paths, name).with_context(|| {
        format!("no claude.ai session saved for {name}: sign it in in the browser once")
    })?;
    let (web, paths, name) = (web.clone(), paths.clone(), name.to_string());
    std::thread::spawn(move || {
        let r = (|| -> Result<()> {
            cdp_connect(&web)?;
            let handle = web.lock().unwrap().cdp.clone();
            let mut guard = handle.lock().unwrap();
            let cdp = guard.as_mut().context("not connected to Chrome")?;
            let now = crate::observe::now_ms();
            let _ = cdp_capture(cdp, &paths, now);
            let target: Vec<Value> = session
                .cookies
                .iter()
                .map(crate::cdp::from_extension)
                .collect();
            cdp.set_session(&target)?;
            cdp.reload_claude_tabs()?;
            Ok(())
        })();
        let mut hub = web.lock().unwrap();
        match r {
            Ok(()) => {
                hub.account = Some(name.clone());
                hub.last_error = None;
                hub.notes
                    .push_back(("web.switched".into(), json!({"account": name})));
            }
            Err(e) => {
                let error = format!("{e:#}");
                hub.last_error = Some(error.clone());
                hub.notes.push_back((
                    "web.switch_failed".into(),
                    json!({"account": name, "error": error}),
                ));
            }
        }
    });
    Ok(())
}

/// Sign an account in to claude.ai in the browser through Chrome's DevTools: the current session
/// is kept, claude.ai signs out and opens its sign-in page; the new session is saved once the user
/// has signed in (within 10 minutes).
pub fn cdp_login(web: &Arc<Mutex<WebHub>>, paths: &Paths) {
    let (web, paths) = (web.clone(), paths.clone());
    std::thread::spawn(move || {
        let r = (|| -> Result<Option<String>> {
            cdp_connect(&web)?;
            let handle = web.lock().unwrap().cdp.clone();
            let mut guard = handle.lock().unwrap();
            let cdp = guard.as_mut().context("not connected to Chrome")?;
            let _ = cdp_capture(cdp, &paths, crate::observe::now_ms());
            cdp.set_session(&[])?;
            cdp.open_login()?;
            let end = std::time::Instant::now() + std::time::Duration::from_secs(600);
            while std::time::Instant::now() < end {
                std::thread::sleep(std::time::Duration::from_secs(2));
                let signed = cdp.session_cookies()?.iter().any(|c| {
                    matches!(
                        c["name"].as_str(),
                        Some("sessionKey") | Some("sessionKeyV3")
                    )
                });
                if signed {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    return cdp_capture(cdp, &paths, crate::observe::now_ms());
                }
            }
            Ok(None)
        })();
        let mut hub = web.lock().unwrap();
        if let Ok(Some(name)) = &r {
            hub.account = Some(name.clone());
            hub.notes
                .push_back(("web.account".into(), json!({"account": name})));
        }
    });
}

/// Move Claude in Chrome to `name`: by driving Chrome when its remote debugging is allowed (no
/// action from the user), else guided (the user signs it out and in). claude.ai goes back to its
/// account afterwards unless `keep_web`.
pub fn start_cic(
    web: &Arc<Mutex<WebHub>>,
    paths: &Paths,
    name: &str,
    keep_web: bool,
    now_ms: i64,
) -> Result<String> {
    // Without a connection Chrome already allowed, accessibility moves it unattended (Linux),
    // else the user is guided. Opening a new DevTools connection would make Chrome ask.
    let cdp_ready = {
        let handle = web.lock().unwrap().cdp.clone();
        let mut g = handle.lock().unwrap();
        if g.as_mut().is_some_and(|c| c.alive()) {
            true
        } else if let Some(c) = crate::cdp::Cdp::existing(paths) {
            *g = Some(c);
            true
        } else {
            false
        }
    };
    if !cdp_ready {
        let (extension, a11y) = {
            let hub = web.lock().unwrap();
            (hub.connected(now_ms), hub.a11y_state == "ready")
        };
        web.lock()
            .unwrap()
            .push_cic(paths, name, keep_web, now_ms)?;
        // Chrome does not let an extension open another extension's page: the service opens
        // Claude in Chrome's options itself, once claude.ai carries the account.
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(1500));
            let _ = crate::platform::open_in_browser(
                "chrome",
                &format!("chrome-extension://{}/options.html", crate::cdp::CIC_ID),
            );
        });
        if !(extension && a11y) {
            return Ok("guided".into());
        }
        let email = load(paths, name).map(|s| s.email).unwrap_or_default();
        let id = {
            let mut hub = web.lock().unwrap();
            let f = hub.cic_flow.as_mut().context("no move")?;
            f.mode = "auto".into();
            f.stage = "working".into();
            f.id
        };
        let (web, paths) = (web.clone(), paths.clone());
        std::thread::spawn(move || {
            // The extension puts claude.ai on the account and opens Claude in Chrome's options.
            std::thread::sleep(std::time::Duration::from_secs(3));
            let r = crate::a11y::resign_cic(&email);
            let mut hub = web.lock().unwrap();
            if hub
                .cic_flow
                .as_ref()
                .is_some_and(|f| f.id == id && f.stage == "working")
            {
                hub.queue.push_back(json!({"type": "cic_stop"}));
                hub.end_cic(&paths, r.is_ok(), r.err().map(|e| format!("{e:#}")));
            }
        });
        return Ok("auto".into());
    }
    {
        let mut hub = web.lock().unwrap();
        if hub
            .cic_flow
            .as_ref()
            .is_some_and(|f| f.stage == "waiting" || f.stage == "working")
        {
            anyhow::bail!("Claude in Chrome is already being moved");
        }
        hub.cic_flow = Some(CicFlow {
            id: now_ms,
            to: name.to_string(),
            restore: None,
            started_ms: now_ms,
            stage: "working".into(),
            error: None,
            mode: "auto".into(),
        });
    }
    let (web, paths, name) = (web.clone(), paths.clone(), name.to_string());
    std::thread::spawn(move || {
        let r = (|| -> Result<()> {
            cdp_connect(&web)?;
            let handle = web.lock().unwrap().cdp.clone();
            let mut guard = handle.lock().unwrap();
            let cdp = guard.as_mut().context("not connected to Chrome")?;
            // The browser's current session is kept for its account first.
            if let Ok(Some(current)) = cdp_capture(cdp, &paths, now_ms) {
                web.lock().unwrap().account = Some(current);
            }
            let session = load(&paths, &name).with_context(|| {
                format!("no claude.ai session saved for {name}: sign it in in the browser once")
            })?;
            let before = cdp.session_cookies()?;
            let target: Vec<Value> = session
                .cookies
                .iter()
                .map(crate::cdp::from_extension)
                .collect();
            cdp.set_session(&target)?;
            let moved = cdp.resign_claude_in_chrome(&session.email);
            if !keep_web || moved.is_err() {
                let _ = cdp.set_session(&before);
            }
            moved
        })();
        let mut hub = web.lock().unwrap();
        if r.is_err() {
            // A connection that failed midway is dropped; the next move connects again.
            if let Ok(mut c) = hub.cdp.clone().try_lock() {
                if c.as_mut().is_some_and(|c| !c.alive()) {
                    *c = None;
                    hub.cdp_state = "available".into();
                }
            }
        }
        hub.end_cic(&paths, r.is_ok(), r.err().map(|e| format!("{e:#}")));
    });
    Ok("auto".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_session_cookies_only() {
        let c = vec![
            json!({"name": "sessionKey", "domain": ".claude.ai", "value": "x"}),
            json!({"name": "cf_clearance", "domain": ".claude.ai", "value": "y"}),
            json!({"name": "sessionKey", "domain": "evil.example", "value": "z"}),
            json!({"name": "lastActiveOrg", "domain": "claude.ai", "value": "o"}),
        ];
        let kept = session_cookies(&c);
        assert_eq!(kept.len(), 2);
        assert!(kept
            .iter()
            .all(|k| k["domain"].as_str().unwrap().ends_with("claude.ai")));
    }
}
