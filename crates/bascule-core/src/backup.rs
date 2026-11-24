//! Account backups: one JSON file with each account's identity, its preferences in Claude Bascule
//! (display name, icon, rotation), its claude.ai session (the browser cookies Claude Bascule keeps
//! for it) and its Claude Code sign-in.
//!
//! An import puts a backup's Claude Code sign-in back for an account that has none here (deleted
//! since, signed out, or new on this computer); a sign-in already here is never replaced. Claude
//! Code renews a sign-in every few hours and each renewal retires the previous refresh token
//! (observed: the refresh token changes at every refresh). So a backup's sign-in works as long as
//! nothing renewed it since the export, and when two computers hold the same one, the first to
//! renew keeps it and the other must sign in again. The report says when it came from another
//! computer.

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
    /// Which computer and user made it (`platform::machine_key`): Claude Code sign-ins are only
    /// restored there.
    #[serde(default)]
    pub machine: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_code: Option<CliPart>,
}

/// Claude Code's sign-in: its `claudeAiOauth` and `oauthAccount`, as Claude Code writes them.
#[derive(Debug, Serialize, Deserialize)]
pub struct CliPart {
    pub oauth: Value,
    pub account: Value,
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

/// What an import did to one account.
#[derive(Debug, Serialize, PartialEq)]
pub struct Item {
    pub name: String,
    /// `added`, `completed` (it received something it lacked) or `unchanged`.
    pub status: &'static str,
    /// Claude Code's sign-in: `restored`, `restored_elsewhere` (restored from another computer's
    /// backup), `here` (this computer already has one), `expired`, or `none` (not in the backup).
    pub cli: &'static str,
    /// The claude.ai session: `restored`, `here` (one is already saved here), `other_machine`
    /// (made in another computer's browser: claude.ai refuses a session copied to another browser,
    /// so it is not restored) or `none`.
    pub web: &'static str,
}

#[derive(Debug, Default, Serialize, PartialEq)]
pub struct Report {
    pub accounts: Vec<Item>,
}

/// The accounts named (every account when `names` is empty), in the order Claude Bascule shows them.
pub fn export(
    paths: &Paths,
    names: &[String],
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Backup> {
    let list = AccountList::load(&paths.accounts_file())?;
    let active = crate::vault::active_account(paths, &list).map(|a| a.name.clone());
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
            claude_code: {
                let slot = if active.as_deref() == Some(a.name.as_str()) {
                    paths.main.clone()
                } else {
                    paths.account_slot(&a.name)
                };
                match (slot.oauth().ok().flatten(), slot.account().ok().flatten()) {
                    (Some(oauth), Some(account)) => Some(CliPart { oauth, account }),
                    _ => None,
                }
            },
        })
        .collect();
    Ok(Backup {
        format: FORMAT.into(),
        version: VERSION,
        exported_at: now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        app_version: env!("CARGO_PKG_VERSION").into(),
        machine: crate::platform::machine_key(&paths.home),
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

/// Put a backup's Claude Code sign-in back into the account's vault entry, when that is safe.
fn restore_cli(
    paths: &Paths,
    backup: &Backup,
    e: &Entry,
    name: &str,
    active: bool,
    now_ms: i64,
) -> Result<&'static str> {
    let Some(c) = &e.claude_code else {
        return Ok("none");
    };
    let slot = paths.account_slot(name);
    if active || slot.oauth()?.is_some() {
        return Ok("here");
    }
    let same_email = c
        .account
        .get("emailAddress")
        .and_then(Value::as_str)
        .is_some_and(|m| m.eq_ignore_ascii_case(&e.email));
    if !same_email || !c.oauth.get("refreshToken").is_some_and(Value::is_string) {
        return Ok("none");
    }
    if c.oauth
        .get("refreshTokenExpiresAt")
        .and_then(Value::as_i64)
        .is_some_and(|t| t <= now_ms)
    {
        return Ok("expired");
    }
    crate::store::private_dir(&slot.dir)?;
    slot.set_oauth(Some(&c.oauth))?;
    slot.set_account(Some(&c.account))?;
    if backup.machine.is_empty() || backup.machine != crate::platform::machine_key(&paths.home) {
        Ok("restored_elsewhere")
    } else {
        Ok("restored")
    }
}

/// Add the backup's accounts that are not here, and give the ones already here what they lack.
/// What this computer already has (a sign-in, a claude.ai session, a name, an icon) is never
/// overwritten.
pub fn import(paths: &Paths, backup: &Backup) -> Result<Report> {
    let mut list = AccountList::load(&paths.accounts_file())?;
    let active = crate::vault::active_account(paths, &list).map(|a| a.name.clone());
    let now_ms = chrono::Utc::now().timestamp_millis();
    let mut settings = Settings::load(&paths.settings_file())?;
    let before: Vec<String> = list.accounts.iter().map(|a| a.name.clone()).collect();
    let mut rotation = if settings.rotation.is_empty() {
        before.clone()
    } else {
        settings.rotation.clone()
    };
    let mut report = Report::default();
    let mut settings_changed = false;
    let same_machine =
        !backup.machine.is_empty() && backup.machine == crate::platform::machine_key(&paths.home);
    for e in &backup.accounts {
        let existing = list.by_email(&e.email).map(|a| a.name.clone());
        let name = match &existing {
            Some(n) => n.clone(),
            None => list.add(&e.email)?,
        };
        let mut changed = false;
        let cli = restore_cli(
            paths,
            backup,
            e,
            &name,
            active.as_deref() == Some(name.as_str()),
            now_ms,
        )?;
        changed |= cli.starts_with("restored");
        let mut web_done = "none";
        if let Some(w) = &e.claude_ai {
            let cookies = web::session_cookies(&w.cookies);
            if !cookies.is_empty() && web::has(paths, &name) {
                web_done = "here";
            } else if !cookies.is_empty() && !same_machine {
                // claude.ai ties a session to the browser that signed in: put in another browser,
                // it is refused, and that browser ends up signed out.
                web_done = "other_machine";
            } else if !cookies.is_empty() {
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
                web_done = "restored";
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
        let status = if existing.is_none() {
            if e.in_rotation {
                rotation.push(name.clone());
            }
            "added"
        } else if changed {
            "completed"
        } else {
            "unchanged"
        };
        report.accounts.push(Item {
            name,
            status,
            cli,
            web: web_done,
        });
    }
    if report.accounts.iter().any(|i| i.status == "added") {
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

    fn oauth(rt: &str) -> Value {
        json!({"accessToken": format!("A-{rt}"), "refreshToken": rt, "refreshTokenExpiresAt": 4102444800000_i64})
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
        // perso is the active account, pro rests in the vault, spare is signed out.
        p.main.set_oauth(Some(&oauth("RT-perso"))).unwrap();
        p.main
            .set_account(Some(&json!({"emailAddress": "perso@example.com"})))
            .unwrap();
        let pro = p.account_slot("pro");
        pro.set_oauth(Some(&oauth("RT-pro"))).unwrap();
        pro.set_account(Some(&json!({"emailAddress": "pro@example.com"})))
            .unwrap();
    }

    fn item<'a>(r: &'a Report, name: &str) -> &'a Item {
        r.accounts.iter().find(|i| i.name == name).unwrap()
    }

    #[test]
    fn export_carries_preferences_session_and_sign_in() {
        let t = Tmp::new("export");
        let p = paths(t.path());
        setup(&p);
        let b = export(&p, &[], chrono::Utc::now()).unwrap();
        let emails: Vec<_> = b.accounts.iter().map(|e| e.email.as_str()).collect();
        assert_eq!(
            emails,
            ["perso@example.com", "pro@example.com", "spare@example.com"]
        );
        assert_eq!(b.machine, crate::platform::machine_key(&p.home));
        let pro = &b.accounts[1];
        assert_eq!(pro.display_name.as_deref(), Some("Work"));
        assert_eq!(pro.icon.as_deref(), Some("briefcase"));
        let w = pro.claude_ai.as_ref().unwrap();
        assert_eq!(w.cookies.len(), 1, "only session cookies");
        assert_eq!(
            pro.claude_code.as_ref().unwrap().oauth["refreshToken"],
            "RT-pro"
        );
        // The active account's sign-in comes from Claude Code's own place.
        assert_eq!(
            b.accounts[0].claude_code.as_ref().unwrap().oauth["refreshToken"],
            "RT-perso"
        );
        assert!(b.accounts[2].claude_code.is_none());
        assert!(!b.accounts[2].in_rotation);
        assert!(b.accounts[0].claude_ai.is_none());
        assert!(export(&p, &["nobody".into()], chrono::Utc::now()).is_err());
        let one = export(&p, &["pro".into()], chrono::Utc::now()).unwrap();
        assert_eq!(one.accounts.len(), 1);
    }

    #[test]
    fn delete_then_import_restores_everything_here() {
        let t = Tmp::new("restore");
        let p = paths(t.path());
        setup(&p);
        let text = serde_json::to_string(&export(&p, &[], chrono::Utc::now()).unwrap()).unwrap();
        // pro is deleted (its vault entry goes with it), then the backup is imported.
        std::fs::remove_dir_all(&p.account_slot("pro").dir).unwrap();
        let mut l = AccountList::load(&p.accounts_file()).unwrap();
        l.remove("pro");
        l.save(&p.accounts_file()).unwrap();

        let r = import(&p, &parse(&text).unwrap()).unwrap();
        let pro = item(&r, "pro");
        assert_eq!(
            (pro.status, pro.cli, pro.web),
            ("added", "restored", "restored")
        );
        let slot = p.account_slot("pro");
        assert_eq!(slot.oauth().unwrap().unwrap()["refreshToken"], "RT-pro");
        assert_eq!(slot.email().as_deref(), Some("pro@example.com"));
        // The active account is never touched; nothing is overwritten.
        assert_eq!(item(&r, "perso").cli, "here");
        assert_eq!(p.main.oauth().unwrap().unwrap()["refreshToken"], "RT-perso");
        let spare = item(&r, "spare");
        assert_eq!(
            (spare.status, spare.cli, spare.web),
            ("unchanged", "none", "none")
        );
        let s = Settings::load(&p.settings_file()).unwrap();
        assert_eq!(s.avatar_icons["pro"], "briefcase");
        assert_eq!(s.display_names["pro"], "Work");
        // A second import changes nothing.
        let r = import(&p, &parse(&text).unwrap()).unwrap();
        assert!(r.accounts.iter().all(|i| i.status == "unchanged"));
        assert_eq!(item(&r, "pro").cli, "here");
    }

    #[test]
    fn another_computer_takes_the_sign_in_when_it_has_none() {
        let src = Tmp::new("src");
        let sp = paths(src.path());
        setup(&sp);
        let mut b = export(&sp, &[], chrono::Utc::now()).unwrap();
        b.machine = "another-computer".into();
        let text = serde_json::to_string(&b).unwrap();

        let dst = Tmp::new("dst");
        let dp = paths(dst.path());
        let mut l = AccountList::default();
        l.add("pro@example.com").unwrap();
        l.save(&dp.accounts_file()).unwrap();
        let mut s = Settings::default();
        s.display_names.insert("pro".into(), "Mine".into());
        s.save(&dp.settings_file()).unwrap();

        let r = import(&dp, &parse(&text).unwrap()).unwrap();
        let added: Vec<_> = r
            .accounts
            .iter()
            .filter(|i| i.status == "added")
            .map(|i| i.name.as_str())
            .collect();
        assert_eq!(added, ["perso", "spare"]);
        let pro = item(&r, "pro");
        assert_eq!(
            (pro.status, pro.cli, pro.web),
            ("completed", "restored_elsewhere", "other_machine")
        );
        assert_eq!(item(&r, "perso").cli, "restored_elsewhere");
        assert_eq!(
            dp.account_slot("pro").oauth().unwrap().unwrap()["refreshToken"],
            "RT-pro"
        );
        assert_eq!(
            dp.account_slot("perso").oauth().unwrap().unwrap()["refreshToken"],
            "RT-perso"
        );
        // A sign-in already on this computer is never replaced.
        dp.account_slot("pro")
            .set_oauth(Some(&oauth("RT-local")))
            .unwrap();
        let r = import(&dp, &parse(&text).unwrap()).unwrap();
        assert_eq!(item(&r, "pro").cli, "here");
        assert_eq!(
            dp.account_slot("pro").oauth().unwrap().unwrap()["refreshToken"],
            "RT-local"
        );
        let s = Settings::load(&dp.settings_file()).unwrap();
        assert_eq!(s.display_names["pro"], "Mine", "local name kept");
        assert_eq!(s.avatar_icons["pro"], "briefcase");
        // pro and perso were in rotation, spare was not.
        assert_eq!(s.rotation, ["pro", "perso"]);
        // claude.ai refuses a session copied into another browser: it stays behind.
        assert!(web::load(&dp, "pro").is_none());
    }

    #[test]
    fn expired_sign_in_is_not_restored() {
        let t = Tmp::new("expired");
        let p = paths(t.path());
        setup(&p);
        let mut b = export(&p, &["pro".into()], chrono::Utc::now()).unwrap();
        b.accounts[0].claude_code.as_mut().unwrap().oauth["refreshTokenExpiresAt"] = json!(1000);
        std::fs::remove_dir_all(&p.account_slot("pro").dir).unwrap();
        let r = import(&p, &b).unwrap();
        assert_eq!(item(&r, "pro").cli, "expired");
        assert!(p.account_slot("pro").oauth().unwrap().is_none());
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
