//! Shared observation helpers: the event log and account quota reads. The observing loop itself
//! is the daemon (`daemon.rs`); `bascule observe` runs it without acting.

use crate::config::AccountList;
use crate::paths::Paths;
use crate::usage::{self, Usage};
use crate::vault::{self, AccountState, Where};
use anyhow::Result;
use serde::Serialize;
use serde_json::{json, Value};
use std::fs::OpenOptions;
use std::io::Write;

const LOG_MAX_BYTES: u64 = 10 * 1024 * 1024;

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Append one event to `events.jsonl` (0600, rotated at 10 MB). Never holds a token.
pub fn log(paths: &Paths, kind: &str, data: Value) {
    let file = paths.log_file();
    if std::fs::metadata(&file)
        .map(|m| m.len() > LOG_MAX_BYTES)
        .unwrap_or(false)
    {
        let _ = std::fs::rename(&file, file.with_extension("jsonl.1"));
    }
    let line = json!({"at": chrono::Local::now().to_rfc3339(), "kind": kind, "data": data});
    let mut opts = OpenOptions::new();
    opts.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    if let Ok(mut f) = opts.open(&file) {
        let _ = writeln!(f, "{line}");
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct AccountView {
    #[serde(flatten)]
    pub state: AccountState,
    pub usage: Option<Usage>,
    pub usage_error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub at: i64,
    pub active: Option<String>,
    pub accounts: Vec<AccountView>,
    pub sessions: Vec<Value>,
    pub jaunt: String,
}

/// Read every account's quotas: the active one with the main token, resting ones with their
/// vault token (refreshed by Claude Code first when it expires within 10 minutes).
pub fn read_accounts(
    paths: &Paths,
    list: &AccountList,
    with_resets: bool,
) -> Result<Vec<AccountView>> {
    let mut out = vec![];
    for st in vault::states(paths, list)? {
        let (usage, usage_error) = match st.place {
            Where::SignedOut => (None, Some("signed out".to_string())),
            ref place => {
                let (slot, is_main) = if *place == Where::Active {
                    (paths.main.clone(), true)
                } else {
                    (paths.account_slot(&st.name), false)
                };
                match vault::fresh_access(&slot, is_main, 10 * 60 * 1000).and_then(|t| match t {
                    Some(t) => usage::fetch(&t, with_resets),
                    None => anyhow::bail!("no token"),
                }) {
                    Ok(u) => (Some(u), None),
                    Err(e) => (None, Some(format!("{e:#}"))),
                }
            }
        };
        // Token info may have changed with a refresh: read it again.
        let state = vault::states(paths, list)?
            .into_iter()
            .find(|s| s.name == st.name)
            .unwrap_or(st);
        out.push(AccountView {
            state,
            usage,
            usage_error,
        });
    }
    Ok(out)
}
