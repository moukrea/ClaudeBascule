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
    match releases() {
        Ok(list) => {
            if let Some(latest) = list.first() {
                st.available = newer(&latest.tag, current);
                st.latest = Some(latest.tag.trim_start_matches('v').to_string());
                st.url = latest.url.clone();
                st.notes = Some(notes_since(&list, current));
            } else {
                st.error = Some("no release published yet".into());
            }
        }
        Err(e) => st.error = Some(format!("{e:#}")),
    }
    st
}

#[derive(Debug, Clone)]
pub struct Release {
    pub tag: String,
    pub body: String,
    pub url: Option<String>,
}

/// Published releases (no drafts, no pre-releases), newest version first.
fn releases() -> Result<Vec<Release>> {
    let resp = crate::net::agent()
        .get(&format!(
            "https://api.github.com/repos/{REPO}/releases?per_page=30"
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
        Err(ureq::Error::Status(code, _)) => anyhow::bail!("GitHub answered HTTP {code}"),
        Err(e) => return Err(e).context("GitHub unreachable"),
    };
    let mut list: Vec<Release> = v
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| {
            !r.get("draft").and_then(Value::as_bool).unwrap_or(false)
                && !r
                    .get("prerelease")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
        })
        .filter_map(|r| {
            Some(Release {
                tag: r.get("tag_name")?.as_str()?.to_string(),
                body: r
                    .get("body")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                url: r
                    .get("html_url")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        })
        .filter(|r| parse_version(&r.tag).is_some())
        .collect();
    list.sort_by(|a, b| parse_version(&b.tag).cmp(&parse_version(&a.tag)));
    Ok(list)
}

/// What changed since `current`: each newer release's notes under `# <version>`, newest first.
pub fn notes_since(list: &[Release], current: &str) -> String {
    let mut out = String::new();
    for r in list.iter().filter(|r| newer(&r.tag, current)).take(15) {
        out.push_str(&format!("# {}\n", r.tag.trim_start_matches('v')));
        for line in r
            .body
            .lines()
            .filter(|l| !l.starts_with("Install or update"))
        {
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
    }
    out
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
    fn notes_of_every_missed_version() {
        let r = |t: &str, b: &str| Release {
            tag: t.into(),
            body: b.into(),
            url: None,
        };
        let list = vec![
            r(
                "v0.4.0",
                "## New\n- browser\n- the README screenshots\nInstall or update: see the README",
            ),
            r("v0.3.0", "## New\n- waits"),
            r("v0.2.2", "## Fixes\n- x"),
        ];
        let n = notes_since(&list, "0.2.2");
        assert!(n.starts_with("# 0.4.0\n## New\n- browser\n- the README screenshots\n"));
        assert!(n.contains("# 0.3.0\n## New\n- waits"));
        assert!(!n.contains("0.2.2") && !n.contains("Install or update"));
    }

    #[test]
    fn versions() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("0.4.0-rc.1"), Some((0, 4, 0)));
        assert!(newer("v0.2.0", "0.1.9"));
        assert!(!newer("v0.1.0", "0.1.0"));
        assert!(newer("1.0.0", "0.9.12"));
    }
}
