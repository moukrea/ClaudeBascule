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

    /// Ask the browser to sign out of claude.ai (keeping the current session) and open the
    /// sign-in page, so the user signs `email` in.
    pub fn push_login(&mut self, email: &str) {
        self.queue
            .push_back(json!({"type": "login", "email": email}));
    }
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
