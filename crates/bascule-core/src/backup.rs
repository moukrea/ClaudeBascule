//! Account backups: one JSON file with each account's identity, its preferences in Claude Bascule
//! (display name, icon, rotation) and its claude.ai session (the browser cookies Claude Bascule
//! keeps for it). Never Claude Code's tokens: a refresh token must have one live copy, and a copy
//! is invalidated by the first refresh, so each computer keeps its own Claude Code sign-in.

use crate::config::{AccountList, Settings};
use crate::paths::Paths;
use crate::web::{self, WebSession};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const FORMAT: &str = "claude-bascule-accounts";
pub const VERSION: u32 = 1;
/// A backup is a few kilobytes; anything far bigger is not one.
const MAX_BYTES: usize = 4 << 20;

#[derive(Debug, Serialize, Deserialize)]
pub struct Backup {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub exported_at: String,
    #[serde(default)]
    pub app_version: String,
    pub accounts: Vec<Entry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Entry {
    pub email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default = "yes")]
    pub in_rotation: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_ai: Option<WebPart>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WebPart {
    /// Cookies as the browser extension's `chrome.cookies` API gives them.
    pub cookies: Vec<Value>,
    #[serde(default)]
    pub saved_at: i64,
}

/// What an import did, by account id.
#[derive(Debug, Default, Serialize, PartialEq)]
pub struct Report {
    /// New accounts (they still need a Claude Code sign-in on this computer).
    pub added: Vec<String>,
    /// Accounts already here that received what they lacked (claude.ai session, name, icon).
    pub completed: Vec<String>,
    /// Accounts already here with nothing to add.
    pub unchanged: Vec<String>,
}

/// The accounts named (every account when `names` is empty), in the order Claude Bascule shows them.
pub fn export(
    paths: &Paths,
    names: &[String],
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Backup> {
    let list = AccountList::load(&paths.accounts_file())?;
    let settings = Settings::load(&paths.settings_file())?;
    for n in names {
        if list.by_name(n).is_none() {
            bail!("unknown account: {n}");
        }
    }
    let rotation: Vec<String> = if settings.rotation.is_empty() {
        list.accounts.iter().map(|a| a.name.clone()).collect()
    } else {
        settings.rotation.clone()
    };
    let mut ordered: Vec<_> = list
        .accounts
        .iter()
        .filter(|a| names.is_empty() || names.contains(&a.name))
        .collect();
    ordered.sort_by_key(|a| {
        rotation
            .iter()
            .position(|n| n == &a.name)
            .unwrap_or(rotation.len())
    });
    let accounts = ordered
        .into_iter()
        .map(|a| Entry {
            email: a.email.clone(),
            display_name: settings
                .display_names
                .get(&a.name)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            icon: settings.avatar_icons.get(&a.name).cloned(),
            in_rotation: rotation.contains(&a.name),
            claude_ai: web::load(paths, &a.name)
                .map(|s| WebPart {
                    cookies: web::session_cookies(&s.cookies),
                    saved_at: s.saved_at,
                })
                .filter(|w| !w.cookies.is_empty()),
        })
        .collect();
    Ok(Backup {
        format: FORMAT.into(),
        version: VERSION,
        exported_at: now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        app_version: env!("CARGO_PKG_VERSION").into(),
        accounts,
    })
}

/// `claude-bascule-<account id, or "accounts">-<date>.json`, not yet used in `dir`.
pub fn free_path(dir: &Path, names: &[String], now: chrono::DateTime<chrono::Local>) -> PathBuf {
    let what = match names {
        [one] => one.clone(),
        _ => "accounts".into(),
    };
    let base = format!("claude-bascule-{what}-{}", now.format("%Y-%m-%d"));
    let mut path = dir.join(format!("{base}.json"));
    let mut i = 2;
    while path.symlink_metadata().is_ok() {
        path = dir.join(format!("{base}-{i}.json"));
        i += 1;
    }
    path
}

/// Write the backup privately (0600 on Unix), atomically, never through a symlink.
pub fn write(path: &Path, backup: &Backup) -> Result<()> {
    // Field order as declared (format first), for a file people may open.
    let mut text = serde_json::to_string_pretty(backup)?;
    text.push('\n');
    crate::store::atomic_write_text(path, &text, 0o600)
}

/// Read a backup file's text and check it is one this version understands.
pub fn parse(text: &str) -> Result<Backup> {
    if text.len() > MAX_BYTES {
        bail!("this file is not a Claude Bascule backup (too big)");
    }
    let b: Backup = serde_json::from_str(text)
        .ok()
        .filter(|b: &Backup| b.format == FORMAT)
        .context("this file is not a Claude Bascule backup")?;
    if b.version > VERSION {
        bail!("this backup comes from a newer Claude Bascule: update this one first");
    }
    Ok(b)
}

fn icon_ok(icon: &str) -> bool {
    !icon.is_empty()
        && icon.len() <= 40
        && icon
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Add the backup's accounts that are not here, and give the ones already here what they lack.
/// What this computer already has (a claude.ai session, a name, an icon) is never overwritten.
pub fn import(paths: &Paths, backup: &Backup) -> Result<Report> {
    let mut list = AccountList::load(&paths.accounts_file())?;
    let mut settings = Settings::load(&paths.settings_file())?;
    let before: Vec<String> = list.accounts.iter().map(|a| a.name.clone()).collect();
    let mut rotation = if settings.rotation.is_empty() {
        before.clone()
    } else {
        settings.rotation.clone()
    };
    let mut report = Report::default();
    let mut settings_changed = false;
    for e in &backup.accounts {
        let existing = list.by_email(&e.email).map(|a| a.name.clone());
        let name = match &existing {
            Some(n) => n.clone(),
            None => list.add(&e.email)?,
        };
        let mut changed = false;
        if let Some(w) = &e.claude_ai {
            let cookies = web::session_cookies(&w.cookies);
            if !cookies.is_empty() && !web::has(paths, &name) {
                web::save(
                    paths,
                    &name,
                    &WebSession {
                        email: list
                            .by_name(&name)
                            .map(|a| a.email.clone())
                            .unwrap_or_default(),
                        cookies,
                        saved_at: w.saved_at,
                    },
                )?;
                changed = true;
            }
        }
        if let Some(d) = e.display_name.as_deref().map(str::trim) {
            if !d.is_empty() && !settings.display_names.contains_key(&name) {
                settings.display_names.insert(name.clone(), d.to_string());
                changed = true;
                settings_changed = true;
            }
        }
        if let Some(i) = e.icon.as_deref().filter(|i| icon_ok(i)) {
            if !settings.avatar_icons.contains_key(&name) {
                settings.avatar_icons.insert(name.clone(), i.to_string());
                changed = true;
                settings_changed = true;
            }
        }
        if existing.is_none() {
            if e.in_rotation {
                rotation.push(name.clone());
            }
            report.added.push(name);
        } else if changed {
            report.completed.push(name);
        } else {
            report.unchanged.push(name);
        }
    }
    if !report.added.is_empty() {
        list.save(&paths.accounts_file())?;
        // Every account in the list's order is written as "every account" (an empty rotation).
        let all: Vec<String> = list.accounts.iter().map(|a| a.name.clone()).collect();
        let next = if rotation == all { vec![] } else { rotation };
        if next != settings.rotation {
            settings.rotation = next;
            settings_changed = true;
        }
    }
    if settings_changed {
        settings.save(&paths.settings_file())?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A scratch folder removed at the end of the test.
    struct Tmp(PathBuf);
    impl Tmp {
        fn new(tag: &str) -> Tmp {
            let root =
                std::env::temp_dir().join(format!("bascule-backup-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            Tmp(root)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn paths(dir: &Path) -> Paths {
        let mut p = Paths::from_env();
        p.state = dir.join("state");
        p.vault = dir.join("state/accounts");
        p.config_dir = dir.join("config");
        p.files_only = true;
        p.main = crate::store::Slot::config_dir(dir.join("main")).files_only();
        p
    }

    fn cookie(name: &str, value: &str) -> Value {
        json!({"name": name, "value": value, "domain": ".claude.ai", "path": "/", "secure": true, "httpOnly": true})
    }

    fn setup(p: &Paths) {
        let mut l = AccountList::default();
        l.add("pro@example.com").unwrap();
        l.add("perso@example.com").unwrap();
        l.add("spare@example.com").unwrap();
        l.save(&p.accounts_file()).unwrap();
        let mut s = Settings {
            rotation: vec!["perso".into(), "pro".into()],
            ..Default::default()
        };
        s.display_names.insert("pro".into(), "Work".into());
        s.avatar_icons.insert("pro".into(), "briefcase".into());
        s.save(&p.settings_file()).unwrap();
        web::save(
            p,
            "pro",
            &WebSession {
                email: "pro@example.com".into(),
                cookies: vec![cookie("sessionKey", "s1"), cookie("_ga", "tracking")],
                saved_at: 7,
            },
        )
        .unwrap();
    }

    #[test]
    fn export_carries_preferences_and_session_only() {
        let t = Tmp::new("export");
        let p = paths(t.path());
        setup(&p);
        let b = export(&p, &[], chrono::Utc::now()).unwrap();
        let emails: Vec<_> = b.accounts.iter().map(|e| e.email.as_str()).collect();
        assert_eq!(
            emails,
            ["perso@example.com", "pro@example.com", "spare@example.com"]
        );
        let pro = &b.accounts[1];
        assert_eq!(pro.display_name.as_deref(), Some("Work"));
        assert_eq!(pro.icon.as_deref(), Some("briefcase"));
        let w = pro.claude_ai.as_ref().unwrap();
        assert_eq!(w.cookies.len(), 1, "only session cookies");
        assert!(!b.accounts[2].in_rotation);
        assert!(b.accounts[0].claude_ai.is_none());
        let text = serde_json::to_string(&b).unwrap();
        assert!(!text.contains("claudeAiOauth") && !text.contains("refresh"));
        assert!(export(&p, &["nobody".into()], chrono::Utc::now()).is_err());
        let one = export(&p, &["pro".into()], chrono::Utc::now()).unwrap();
        assert_eq!(one.accounts.len(), 1);
    }

    #[test]
    fn import_adds_and_completes_without_overwriting() {
        let src = Tmp::new("src");
        let sp = paths(src.path());
        setup(&sp);
        let text = serde_json::to_string(&export(&sp, &[], chrono::Utc::now()).unwrap()).unwrap();

        let dst = Tmp::new("dst");
        let dp = paths(dst.path());
        let mut l = AccountList::default();
        l.add("pro@example.com").unwrap();
        l.save(&dp.accounts_file()).unwrap();
        let mut s = Settings::default();
        s.display_names.insert("pro".into(), "Mine".into());
        s.save(&dp.settings_file()).unwrap();

        let r = import(&dp, &parse(&text).unwrap()).unwrap();
        assert_eq!(r.added, ["perso", "spare"]);
        assert_eq!(r.completed, ["pro"]);
        let s = Settings::load(&dp.settings_file()).unwrap();
        assert_eq!(s.display_names["pro"], "Mine", "local name kept");
        assert_eq!(s.avatar_icons["pro"], "briefcase");
        // pro and perso were in rotation, spare was not.
        assert_eq!(s.rotation, ["pro", "perso"]);
        assert_eq!(web::load(&dp, "pro").unwrap().cookies.len(), 1);

        // A second import adds nothing.
        let r = import(&dp, &parse(&text).unwrap()).unwrap();
        assert!(r.added.is_empty() && r.completed.is_empty());
        assert_eq!(r.unchanged.len(), 3);
    }

    #[test]
    fn parse_refuses_other_files() {
        assert!(parse("{}").is_err());
        assert!(parse("not json").is_err());
        assert!(
            parse(r#"{"format":"claude-bascule-accounts","version":99,"accounts":[]}"#).is_err()
        );
        assert!(parse(r#"{"format":"claude-bascule-accounts","version":1,"accounts":[]}"#).is_ok());
    }

    #[test]
    fn file_names() {
        let t = Tmp::new("names");
        let now = chrono::Local::now();
        let p = free_path(t.path(), &["pro".into()], now);
        assert!(p
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("claude-bascule-pro-"));
        std::fs::write(&p, "x").unwrap();
        let q = free_path(t.path(), &["pro".into()], now);
        assert!(q.to_string_lossy().ends_with("-2.json"));
        let all = free_path(t.path(), &[], now);
        assert!(all.to_string_lossy().contains("claude-bascule-accounts-"));
    }
}
