//! Settings (`~/.config/bascule/config.toml`) and the account list (`accounts.toml`).
//! Every value has a default; a missing file or key means the default.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Master switch for automatic switching (the "pause").
    pub auto_switch: bool,
    /// Switch before the wall.
    pub anticipate: bool,
    /// Anticipation threshold, percent of a window.
    pub threshold_percent: f64,
    /// Per-window thresholds overriding the global one: keys `five_hour`, `seven_day`, `model`.
    pub thresholds: std::collections::BTreeMap<String, f64>,
    /// Switch when a session hits a limit.
    pub switch_on_limit: bool,
    /// After a switch, how long to collect sessions failing on the old account.
    pub collect_seconds: u64,
    /// How long a blocked session may take to restart by itself before it is listed.
    pub spontaneous_restart_seconds: u64,
    /// After sending the resume message, how long to wait for the session to go busy.
    pub confirm_seconds: u64,
    /// Margin after a reset time before an exhausted account is used again.
    pub reset_margin_seconds: u64,
    /// Minimum time between two automatic switches (a real limit overrides it).
    pub min_switch_gap_seconds: u64,
    /// Re-read the quotas of resting accounts this often.
    pub reread_resting_minutes: u64,
    /// Refresh a resting login this many days before its refresh token expires.
    pub refresh_days_before: u64,
    /// `headroom` (most room on the window concerned) or `order` (the `rotation` order).
    pub next_account_rule: String,
    /// Accounts the automatic switch may use, in order. Empty = every account.
    pub rotation: Vec<String>,
    /// Send the resume message without asking.
    pub auto_send: bool,
    /// Custom resume message; empty = the default for the system language.
    pub resume_message: String,
    /// Observer: how often to poll jaunt when no event arrives.
    pub observe_poll_seconds: u64,
    /// Read the active account's quotas from the usage API this often. Between reads, anticipation
    /// uses the windows Claude Code's status line reports through jaunt (no extra request).
    pub active_poll_seconds: u64,
    /// After Anthropic refuses a quota read (HTTP 429), wait at least this long before reading
    /// that account again.
    pub rate_limit_backoff_minutes: u64,
    /// Let model-scoped windows (e.g. Fable weekly) trigger an anticipated switch (on by default).
    /// Off: a model limit switches only when a session actually hits it.
    pub anticipate_model_windows: bool,
    /// Desktop notifications (switches, sessions to resume).
    pub notify: bool,
    /// Read each account's offered resets (read only; the request presents itself as Claude Code).
    pub read_resets: bool,
    /// Display names by account id; a display name hides the account's email everywhere.
    pub display_names: std::collections::BTreeMap<String, String>,
    /// An icon (app/ui/avatars.js) in place of the initial, by account id.
    pub avatar_icons: std::collections::BTreeMap<String, String>,
    /// Demo mode: mask every email address (in the mini-app and in notifications).
    pub hide_emails: bool,
    /// Language of the mini-app, notifications and the default resume message: one of
    /// LANGUAGES, or empty for the system language.
    pub language: String,
    /// Look for a new release on GitHub every hour (and when the window opens).
    pub check_updates: bool,
    /// When Claude Code switches account, switch claude.ai in the browser too (needs the
    /// Claude Bascule Connector extension and a saved web session for the account).
    pub web_follow: bool,
}

/// The languages Bascule ships, the same as jaunt and daffin.
pub const LANGUAGES: [&str; 6] = ["en", "fr", "de", "es", "it", "pt"];

impl Default for Settings {
    fn default() -> Self {
        Settings {
            auto_switch: true,
            anticipate: true,
            threshold_percent: 95.0,
            thresholds: Default::default(),
            switch_on_limit: true,
            collect_seconds: 30,
            spontaneous_restart_seconds: 10,
            confirm_seconds: 20,
            reset_margin_seconds: 120,
            min_switch_gap_seconds: 300,
            reread_resting_minutes: 15,
            refresh_days_before: 3,
            next_account_rule: "headroom".into(),
            rotation: vec![],
            auto_send: false,
            resume_message: String::new(),
            observe_poll_seconds: 30,
            active_poll_seconds: 300,
            rate_limit_backoff_minutes: 10,
            anticipate_model_windows: true,
            notify: true,
            read_resets: true,
            display_names: Default::default(),
            avatar_icons: Default::default(),
            hide_emails: false,
            language: String::new(),
            check_updates: true,
            web_follow: true,
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).with_context(|| format!("parse {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
            Err(e) => Err(e).with_context(|| format!("read {}", path.display())),
        }
    }

    pub fn threshold_for(&self, window: &str) -> f64 {
        self.thresholds
            .get(window)
            .copied()
            .unwrap_or(self.threshold_percent)
    }

    /// How an account is shown: its display name, else its email (masked in demo mode).
    pub fn label_for(&self, name: &str, email: &str) -> String {
        match self
            .display_names
            .get(name)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            Some(alias) => alias.to_string(),
            None if self.hide_emails => mask_email(email),
            None => email.to_string(),
        }
    }

    /// The language in use: the chosen one, else the system's when Bascule ships it, else English.
    pub fn lang(&self) -> String {
        let pick = |l: &str| LANGUAGES.contains(&l).then(|| l.to_string());
        pick(self.language.trim())
            .or_else(|| pick(&system_language()))
            .unwrap_or_else(|| "en".into())
    }

    /// The resume message: the custom one, or the default in the language in use.
    pub fn resume_text(&self) -> String {
        if !self.resume_message.trim().is_empty() {
            return self.resume_message.trim().to_string();
        }
        default_resume_message(&self.lang())
    }
}

/// `someone@gmail.com` → `s•••@g•••.com`: enough to tell accounts apart in a demo, nothing more.
pub fn mask_email(email: &str) -> String {
    let (local, domain) = email.split_once('@').unwrap_or((email, ""));
    let first = |s: &str| s.chars().next().map(|c| c.to_string()).unwrap_or_default();
    if domain.is_empty() {
        return format!("{}•••", first(local));
    }
    let (host, tld) = domain.rsplit_once('.').unwrap_or((domain, ""));
    let tld = if tld.is_empty() {
        String::new()
    } else {
        format!(".{tld}")
    };
    format!("{}•••@{}•••{}", first(local), first(host), tld)
}

/// The system language from the usual locale variables (`fr_FR.UTF-8` → `fr`).
pub fn system_language() -> String {
    for key in ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
        if let Ok(v) = std::env::var(key) {
            let lang: String = v.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
            if !lang.is_empty() && lang != "C" && lang != "POSIX" {
                return lang.to_lowercase();
            }
        }
    }
    "en".into()
}

pub fn default_resume_message(lang: &str) -> String {
    match lang {
        "fr" => "Continue, limites réinitialisées.",
        "de" => "Mach weiter, die Limits wurden zurückgesetzt.",
        "es" => "Continúa, los límites se han restablecido.",
        "it" => "Continua, i limiti sono stati reimpostati.",
        "pt" => "Continua, os limites foram redefinidos.",
        _ => "Continue, the limits have been reset.",
    }
    .into()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Account {
    pub name: String,
    pub email: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AccountList {
    #[serde(default, rename = "account")]
    pub accounts: Vec<Account>,
}

impl AccountList {
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).with_context(|| format!("parse {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(AccountList::default()),
            Err(e) => Err(e).with_context(|| format!("read {}", path.display())),
        }
    }

    pub fn by_email(&self, email: &str) -> Option<&Account> {
        self.accounts
            .iter()
            .find(|a| a.email.eq_ignore_ascii_case(email))
    }

    pub fn by_name(&self, name: &str) -> Option<&Account> {
        self.accounts.iter().find(|a| a.name == name)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            crate::store::private_dir(dir)?;
        }
        let text = format!(
            "# Claude Bascule accounts on this machine (local only).\n\n{}",
            toml::to_string_pretty(self)?
        );
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Add an account by email; its id comes from the email (`some.one@example.com` → `some-one`, then
    /// `some-one-2`...). Returns the id; an email already listed returns the existing id.
    pub fn add(&mut self, email: &str) -> Result<String> {
        let email = email.trim();
        if !email.contains('@') || email.contains(char::is_whitespace) {
            anyhow::bail!("not an email address: {email}");
        }
        if let Some(a) = self.by_email(email) {
            return Ok(a.name.clone());
        }
        let base: String = email
            .split('@')
            .next()
            .unwrap_or("account")
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        let base = if base.trim_matches('-').is_empty() {
            "account".to_string()
        } else {
            base.trim_matches('-').to_string()
        };
        let mut name = base.clone();
        let mut i = 2;
        while self.by_name(&name).is_some() {
            name = format!("{base}-{i}");
            i += 1;
        }
        self.accounts.push(Account {
            name: name.clone(),
            email: email.to_string(),
        });
        Ok(name)
    }

    pub fn remove(&mut self, name: &str) -> bool {
        let n = self.accounts.len();
        self.accounts.retain(|a| a.name != name);
        n != self.accounts.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_file_keeps_defaults() {
        let s: Settings =
            toml::from_str("threshold_percent = 90\n[thresholds]\nseven_day = 97\n").unwrap();
        assert_eq!(s.threshold_percent, 90.0);
        assert_eq!(s.threshold_for("seven_day"), 97.0);
        assert_eq!(s.threshold_for("five_hour"), 90.0);
        assert_eq!(s.collect_seconds, 30);
        assert!(s.anticipate);
    }

    #[test]
    fn custom_message_overrides_language() {
        let s = Settings {
            resume_message: "Go on, quotas are back.".into(),
            ..Default::default()
        };
        assert_eq!(s.resume_text(), "Go on, quotas are back.");
        assert_eq!(
            default_resume_message("fr"),
            "Continue, limites réinitialisées."
        );
    }

    #[test]
    fn language_choice() {
        let s = Settings {
            language: "de".into(),
            ..Default::default()
        };
        assert_eq!(s.lang(), "de");
        assert_eq!(
            s.resume_text(),
            "Mach weiter, die Limits wurden zurückgesetzt."
        );
        let s = Settings {
            language: "xx".into(),
            ..Default::default()
        };
        assert!(LANGUAGES.contains(&s.lang().as_str()));
    }

    #[test]
    fn labels_and_masking() {
        let mut s = Settings::default();
        assert_eq!(s.label_for("a", "someone@gmail.com"), "someone@gmail.com");
        s.hide_emails = true;
        assert_eq!(s.label_for("a", "someone@gmail.com"), "s•••@g•••.com");
        assert_eq!(mask_email("admin@example.earth"), "a•••@e•••.earth");
        s.display_names.insert("a".into(), "Perso".into());
        assert_eq!(s.label_for("a", "someone@gmail.com"), "Perso");
    }

    #[test]
    fn add_and_remove_accounts() {
        let mut l = AccountList::default();
        assert_eq!(l.add("Some.One@example.com").unwrap(), "some-one");
        assert_eq!(l.add("some.one@other.org").unwrap(), "some-one-2");
        assert_eq!(l.add("Some.One@example.com").unwrap(), "some-one");
        assert!(l.add("not an email").is_err());
        assert!(l.remove("some-one"));
        assert_eq!(l.accounts.len(), 1);
    }

    #[test]
    fn accounts_file() {
        let l: AccountList =
            toml::from_str("[[account]]\nname = \"a\"\nemail = \"A@x.io\"\n").unwrap();
        assert_eq!(l.by_email("a@x.io").unwrap().name, "a");
    }
}
