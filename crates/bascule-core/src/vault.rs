//! The vault: one isolated `CLAUDE_CONFIG_DIR` per account, and the switch.
//!
//! One live copy of each refresh token (docs/internals.md): the active account's tokens live in the
//! main slot (`~/.claude`), where Claude Code refreshes them; its vault entry holds no token
//! ("checked out"). A switch checks the outgoing tokens in, then writes the incoming ones.

use crate::config::{Account, AccountList};
use crate::paths::Paths;
use crate::store::{fp, private_dir, Slot, TokenInfo};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::Value;
use std::process::Stdio;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Where {
    /// Its tokens are in the main slot: Claude Code uses it now.
    Active,
    /// Its tokens are in its vault entry.
    Resting,
    /// No tokens anywhere: sign in.
    SignedOut,
}

#[derive(Clone, Debug, Serialize)]
pub struct AccountState {
    pub name: String,
    pub email: String,
    pub place: Where,
    pub token: Option<TokenInfo>,
}

/// The account whose email is in the main slot's `oauthAccount`.
pub fn active_account<'a>(paths: &Paths, list: &'a AccountList) -> Option<&'a Account> {
    paths.main.email().and_then(|e| list.by_email(&e))
}

pub fn states(paths: &Paths, list: &AccountList) -> Result<Vec<AccountState>> {
    let active = active_account(paths, list).map(|a| a.name.clone());
    let main_oauth = paths.main.oauth()?;
    let mut out = vec![];
    for a in &list.accounts {
        let (place, token) = if Some(&a.name) == active.as_ref() && main_oauth.is_some() {
            (Where::Active, main_oauth.as_ref().map(TokenInfo::of))
        } else {
            match paths.account_slot(&a.name).oauth()? {
                Some(o) => (Where::Resting, Some(TokenInfo::of(&o))),
                None => (Where::SignedOut, None),
            }
        };
        out.push(AccountState {
            name: a.name.clone(),
            email: a.email.clone(),
            place,
            token,
        });
    }
    Ok(out)
}

/// Remove a vault copy of the live tokens (left by an interrupted switch): one live copy.
pub fn heal(paths: &Paths, list: &AccountList) -> Result<Vec<String>> {
    let mut fixed = vec![];
    let Some(main) = paths.main.oauth()? else {
        return Ok(fixed);
    };
    let live = TokenInfo::of(&main).refresh;
    for a in &list.accounts {
        let slot = paths.account_slot(&a.name);
        if let Some(o) = slot.oauth()? {
            if TokenInfo::of(&o).refresh == live {
                slot.set_oauth(None)?;
                fixed.push(a.name.clone());
            }
        }
    }
    Ok(fixed)
}

#[derive(Clone, Debug, Serialize)]
pub struct SwitchReport {
    pub from: Option<String>,
    pub to: String,
    pub checked_in: Option<String>,
    pub now_live: String,
    pub account_written: bool,
}

/// Switch the main slot to `to`. Order: check the outgoing tokens in, write the incoming ones
/// (credentials, then `oauthAccount`), empty the incoming vault entry, verify.
pub fn switch(paths: &Paths, list: &AccountList, to: &str) -> Result<SwitchReport> {
    let target = list
        .by_name(to)
        .with_context(|| format!("unknown account '{to}' (accounts.toml)"))?;
    let from = active_account(paths, list).map(|a| a.name.clone());
    let outgoing = paths.main.oauth()?;
    if from.is_none() && outgoing.is_some() {
        bail!(
            "the account signed in to Claude Code ({}) is not in accounts.toml: add it before switching, or its tokens would be lost",
            paths.main.email().unwrap_or_else(|| "unknown".into())
        );
    }
    if from.as_deref() == Some(to) {
        bail!("{to} is already the active account");
    }
    let to_slot = paths.account_slot(to);
    let incoming = to_slot.oauth()?.with_context(|| {
        format!("{to} has no tokens in the vault: sign in first (bascule login {to})")
    })?;
    let incoming_account = to_slot.account()?;
    if let Some(email) = incoming_account
        .as_ref()
        .and_then(|a| a.get("emailAddress"))
        .and_then(Value::as_str)
    {
        if !email.eq_ignore_ascii_case(&target.email) {
            bail!(
                "the vault entry '{to}' is signed in as {email}, not {}",
                target.email
            );
        }
    }

    let mut checked_in = None;
    if let (Some(name), Some(out)) = (&from, &outgoing) {
        let slot = paths.account_slot(name);
        private_dir(&slot.dir)?;
        slot.set_oauth(Some(out))?;
        if let Some(acct) = paths.main.account()? {
            slot.set_account(Some(&acct))?;
        }
        checked_in = Some(TokenInfo::of(out).refresh);
    }
    paths.main.set_oauth(Some(&incoming))?;
    let account_written = match &incoming_account {
        Some(a) => {
            paths.main.set_account(Some(a))?;
            true
        }
        None => false,
    };
    to_slot.set_oauth(None)?;

    let live = paths
        .main
        .oauth()?
        .context("main slot empty after the switch")?;
    let want = TokenInfo::of(&incoming).refresh;
    if TokenInfo::of(&live).refresh != want {
        bail!("verification failed: the main slot holds another token than the one written (Claude Code wrote meanwhile?)");
    }
    Ok(SwitchReport {
        from,
        to: to.to_string(),
        checked_in,
        now_live: want,
        account_written,
    })
}

/// After a switch, sessions rewrite `.claude.json` often: put `oauthAccount` back if it was reverted.
pub fn recheck_account(paths: &Paths, list: &AccountList, to: &str) -> Result<bool> {
    let target = list.by_name(to).context("unknown account")?;
    if paths
        .main
        .email()
        .map(|e| e.eq_ignore_ascii_case(&target.email))
        .unwrap_or(false)
    {
        return Ok(false);
    }
    // The vault entry keeps the account object (only its tokens are checked out).
    let acct = paths
        .account_slot(to)
        .account()?
        .context("no oauthAccount kept for this account")?;
    paths.main.set_account(Some(&acct))?;
    Ok(true)
}

/// `claude auth status` in the slot's directory: Claude Code refreshes an expired token itself
/// (docs/internals.md). For the main slot no `CLAUDE_CONFIG_DIR` is set unless it is a test dir.
pub fn refresh(slot: &Slot, is_main: bool) -> Result<()> {
    let mut cmd = crate::platform::claude();
    cmd.args(["auth", "status"])
        .env("DISABLE_AUTOUPDATER", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if !is_main || std::env::var_os("BASCULE_CLAUDE_DIR").is_some() {
        cmd.env("CLAUDE_CONFIG_DIR", &slot.dir);
    } else {
        cmd.env_remove("CLAUDE_CONFIG_DIR");
    }
    let mut child = cmd.spawn().context("run claude auth status")?;
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if child.try_wait()?.is_some() {
            return Ok(());
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            bail!("claude auth status did not finish in 60 s");
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// The slot's access token, refreshed first when it expires within `margin_ms`.
pub fn fresh_access(slot: &Slot, is_main: bool, margin_ms: i64) -> Result<Option<String>> {
    let Some(o) = slot.oauth()? else {
        return Ok(None);
    };
    let exp = o.get("expiresAt").and_then(Value::as_i64).unwrap_or(0);
    let o = if exp - chrono::Utc::now().timestamp_millis() < margin_ms {
        refresh(slot, is_main)?;
        slot.oauth()?.context("tokens vanished during refresh")?
    } else {
        o
    };
    Ok(crate::store::access_token(&o).map(str::to_string))
}

/// Debug helper: a fingerprint line for a slot.
pub fn describe(slot: &Slot) -> String {
    match slot.oauth().ok().flatten() {
        Some(o) => {
            let t = TokenInfo::of(&o);
            format!("access {} refresh {}", t.access, t.refresh)
        }
        None => format!("no tokens ({})", fp("")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::atomic_write;
    use serde_json::json;
    use std::path::PathBuf;

    fn setup(tag: &str) -> (Paths, AccountList, PathBuf) {
        let root = std::env::temp_dir().join(format!("bascule-vault-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut p = Paths::from_env();
        p.state = root.join("state");
        p.vault = root.join("state/accounts");
        p.files_only = true;
        p.main = Slot::config_dir(root.join("main")).files_only();
        let list: AccountList = toml::from_str(
            "[[account]]\nname=\"a\"\nemail=\"a@x.io\"\n[[account]]\nname=\"b\"\nemail=\"b@x.io\"\n",
        )
        .unwrap();
        atomic_write(&p.main.creds, &json!({"mcpOAuth": {"keep": true}, "claudeAiOauth": {"accessToken": "A1", "refreshToken": "AR"}})).unwrap();
        atomic_write(
            &p.main.conf,
            &json!({"oauthAccount": {"emailAddress": "a@x.io"}, "other": 1}),
        )
        .unwrap();
        let b = p.account_slot("b");
        atomic_write(
            &b.creds,
            &json!({"claudeAiOauth": {"accessToken": "B1", "refreshToken": "BR"}}),
        )
        .unwrap();
        atomic_write(
            &b.conf,
            &json!({"oauthAccount": {"emailAddress": "b@x.io"}}),
        )
        .unwrap();
        (p, list, root)
    }

    #[test]
    fn switch_moves_tokens_and_keeps_other_keys() {
        let (p, list, root) = setup("switch");
        let r = switch(&p, &list, "b").unwrap();
        assert_eq!(r.from.as_deref(), Some("a"));
        let main = p.main.oauth().unwrap().unwrap();
        assert_eq!(main["accessToken"], "B1");
        assert_eq!(p.main.email().as_deref(), Some("b@x.io"));
        let creds: Value = serde_json::from_slice(&std::fs::read(&p.main.creds).unwrap()).unwrap();
        assert_eq!(creds["mcpOAuth"]["keep"], true);
        let conf: Value = serde_json::from_slice(&std::fs::read(&p.main.conf).unwrap()).unwrap();
        assert_eq!(conf["other"], 1);
        assert!(
            p.account_slot("b").oauth().unwrap().is_none(),
            "incoming entry is checked out"
        );
        assert_eq!(
            p.account_slot("a").oauth().unwrap().unwrap()["refreshToken"],
            "AR"
        );
        assert_eq!(
            p.account_slot("b").email().as_deref(),
            Some("b@x.io"),
            "account object kept"
        );
        let s = states(&p, &list).unwrap();
        assert_eq!(s[0].place, Where::Resting);
        assert_eq!(s[1].place, Where::Active);
        // and back
        switch(&p, &list, "a").unwrap();
        assert_eq!(p.main.oauth().unwrap().unwrap()["accessToken"], "A1");
        assert_eq!(
            p.account_slot("b").oauth().unwrap().unwrap()["accessToken"],
            "B1"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refuses_unknown_active_account_and_self_switch() {
        let (p, list, root) = setup("refuse");
        assert!(switch(&p, &list, "a").is_err());
        atomic_write(
            &p.main.conf,
            &json!({"oauthAccount": {"emailAddress": "z@x.io"}}),
        )
        .unwrap();
        let e = switch(&p, &list, "b").unwrap_err().to_string();
        assert!(e.contains("not in accounts.toml"), "{e}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn heal_removes_duplicate_copy() {
        let (p, list, root) = setup("heal");
        let a = p.account_slot("a");
        a.set_oauth(Some(&json!({"accessToken": "A1", "refreshToken": "AR"})))
            .unwrap();
        assert_eq!(heal(&p, &list).unwrap(), vec!["a".to_string()]);
        assert!(a.oauth().unwrap().is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recheck_restores_reverted_account() {
        let (p, list, root) = setup("recheck");
        switch(&p, &list, "b").unwrap();
        p.main
            .set_account(Some(&json!({"emailAddress": "a@x.io"})))
            .unwrap();
        assert!(recheck_account(&p, &list, "b").unwrap());
        assert_eq!(p.main.email().as_deref(), Some("b@x.io"));
        assert!(!recheck_account(&p, &list, "b").unwrap());
        std::fs::remove_dir_all(root).unwrap();
    }
}
