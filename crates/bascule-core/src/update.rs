//! Self-update: the latest GitHub release, compared with this build's version.
//! Applying an update runs the platform's install one-liner (done by the mini-app).

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;

pub const REPO: &str = "moukrea/ClaudeBascule";

#[derive(Clone, Debug, Serialize, Default)]
pub struct UpdateState {
    pub current: String,
    pub latest: Option<String>,
    pub available: bool,
    pub notes: Option<String>,
    pub url: Option<String>,
    pub checked_at: Option<i64>,
    pub error: Option<String>,
}

/// `v1.2.3` / `1.2.3` → (1, 2, 3); pre-release suffixes are ignored.
pub fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let v = v.trim().trim_start_matches('v');
    let core = v.split(['-', '+']).next()?;
    let mut it = core.split('.').map(|x| x.parse::<u64>().ok());
    Some((
        it.next()??,
        it.next().flatten().unwrap_or(0),
        it.next().flatten().unwrap_or(0),
    ))
}

pub fn newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

pub fn check(current: &str) -> UpdateState {
    let mut st = UpdateState {
        current: current.to_string(),
        checked_at: Some(chrono::Utc::now().timestamp_millis()),
        ..Default::default()
    };
    match latest_release() {
        Ok((tag, notes, url)) => {
            st.available = newer(&tag, current);
            st.latest = Some(tag.trim_start_matches('v').to_string());
            st.notes = notes;
            st.url = url;
        }
        Err(e) => st.error = Some(format!("{e:#}")),
    }
    st
}

fn latest_release() -> Result<(String, Option<String>, Option<String>)> {
    let resp = ureq::get(&format!(
        "https://api.github.com/repos/{REPO}/releases/latest"
    ))
    .set("Accept", "application/vnd.github+json")
    .set(
        "User-Agent",
        &format!("claude-bascule/{}", env!("CARGO_PKG_VERSION")),
    )
    .timeout(std::time::Duration::from_secs(20))
    .call();
    let v: Value = match resp {
        Ok(r) => r.into_json()?,
        Err(ureq::Error::Status(404, _)) => anyhow::bail!("no release published yet"),
        Err(ureq::Error::Status(code, _)) => anyhow::bail!("GitHub answered HTTP {code}"),
        Err(e) => return Err(e).context("GitHub unreachable"),
    };
    let tag = v
        .get("tag_name")
        .and_then(Value::as_str)
        .context("no tag")?
        .to_string();
    let notes = v.get("body").and_then(Value::as_str).map(str::to_string);
    let url = v
        .get("html_url")
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok((tag, notes, url))
}

/// The command that installs the latest release over this one (and restarts everything).
pub fn install_command() -> (String, Vec<String>) {
    if cfg!(windows) {
        (
            "powershell".into(),
            vec![
                "-NoProfile".into(),
                "-ExecutionPolicy".into(),
                "Bypass".into(),
                "-Command".into(),
                format!("irm https://github.com/{REPO}/releases/latest/download/install.ps1 | iex"),
            ],
        )
    } else {
        ("sh".into(), vec!["-c".into(), format!("curl -fsSL https://github.com/{REPO}/releases/latest/download/install.sh | CLAUDE_BASCULE_RELAUNCH=1 sh")])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("0.4.0-rc.1"), Some((0, 4, 0)));
        assert!(newer("v0.2.0", "0.1.9"));
        assert!(!newer("v0.1.0", "0.1.0"));
        assert!(newer("1.0.0", "0.9.12"));
    }
}
