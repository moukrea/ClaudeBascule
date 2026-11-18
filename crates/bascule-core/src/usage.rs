//! An account's quotas: `GET /api/oauth/usage`, the call behind Claude Code's `/usage`
//! (docs/internals.md). Needs that account's valid access token.
//!
//! Offered resets (docs/internals.md): with `cedar_ember=1` the same
//! call returns the account's reset grants, but only to a request that presents itself as
//! Claude Code (its User-Agent). Read only: Bascule never claims a reset.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const URL: &str = "https://api.anthropic.com/api/oauth/usage";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Window {
    /// The window's name: `five_hour`, `seven_day`, `seven_day_opus`, `seven_day_sonnet`,
    /// or `model:<display name>` for a model-scoped weekly limit.
    pub name: String,
    pub percent: f64,
    /// ISO 8601, when the window resets.
    pub resets_at: Option<String>,
    /// The server marks the limit that currently applies.
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub severity: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct Usage {
    pub fetched_at: i64,
    pub windows: Vec<Window>,
    /// Offered resets, when read (`read_resets`).
    #[serde(default)]
    pub resets: Option<Resets>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct Grant {
    pub label: String,
    pub resets_total: u64,
    pub resets_left: u64,
    /// Can be used right now.
    pub usable_now: bool,
    /// Only once a limit is reached.
    pub use_requires_limit: bool,
    /// Windows it refills (`seven_day`, `seven_day_overage_included` = the model-scoped week...).
    pub clears: Vec<String>,
    pub ends_at: Option<String>,
    pub kind: Option<String>,
    pub paused: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct Resets {
    pub eligible: bool,
    pub ineligible_reason: Option<String>,
    pub at_limit: bool,
    pub cooldown_until: Option<String>,
    pub grants: Vec<Grant>,
}

impl Resets {
    /// Grants with at least one reset left.
    pub fn available(&self) -> impl Iterator<Item = &Grant> {
        self.grants.iter().filter(|g| g.resets_left > 0)
    }
}

fn parse_resets(v: &Value) -> Option<Resets> {
    let v = v.as_object()?;
    let st = |o: &serde_json::Map<String, Value>, k: &str| {
        o.get(k).and_then(Value::as_str).map(str::to_string)
    };
    let grants = v
        .get("grants")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .map(|g| Grant {
            label: st(g, "label").unwrap_or_default(),
            resets_total: g.get("resets_total").and_then(Value::as_u64).unwrap_or(0),
            resets_left: g.get("resets_left").and_then(Value::as_u64).unwrap_or(0),
            usable_now: g
                .get("usable_now")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            use_requires_limit: g
                .get("use_requires_limit")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            clears: g
                .get("clears")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
            ends_at: st(g, "ends_at"),
            kind: st(g, "type"),
            paused: g.get("paused").and_then(Value::as_bool).unwrap_or(false),
        })
        .collect();
    Some(Resets {
        eligible: v.get("eligible").and_then(Value::as_bool).unwrap_or(false),
        ineligible_reason: st(v, "ineligible_reason"),
        at_limit: v.get("at_limit").and_then(Value::as_bool).unwrap_or(false),
        cooldown_until: st(v, "cooldown_until"),
        grants,
    })
}

/// The installed Claude Code version: `~/.local/bin/claude` points at `.../versions/<version>`,
/// else `claude --version`.
pub fn claude_version() -> Option<String> {
    static V: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    V.get_or_init(|| {
        let looks = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || c == '.');
        let home = crate::paths::home_dir();
        if let Ok(target) = std::fs::canonicalize(home.join(".local/bin/claude")) {
            if let Some(name) = target.file_name().and_then(|n| n.to_str()) {
                if looks(name) {
                    return Some(name.to_string());
                }
            }
        }
        let out = crate::platform::claude()
            .arg("--version")
            .env("DISABLE_AUTOUPDATER", "1")
            .output()
            .ok()?;
        let first = String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .next()?
            .to_string();
        looks(&first).then_some(first)
    })
    .clone()
}

impl Usage {
    /// The fullest window: what decides when this account hits the wall.
    pub fn binding(&self) -> Option<&Window> {
        self.windows
            .iter()
            .max_by(|a, b| a.percent.total_cmp(&b.percent))
    }

    pub fn window(&self, name: &str) -> Option<&Window> {
        self.windows.iter().find(|w| w.name == name)
    }

    /// Windows whose reset time has passed are empty again, whatever was read before: set them to 0.
    /// (Numbers only refresh with a real request; a reset happens without one.)
    pub fn expire_resets(&mut self, now_ms: i64) {
        for w in &mut self.windows {
            if w.resets_at
                .as_deref()
                .and_then(|r| chrono::DateTime::parse_from_rfc3339(r).ok())
                .map(|t| t.timestamp_millis() <= now_ms)
                .unwrap_or(false)
            {
                w.percent = 0.0;
                w.active = false;
            }
        }
    }

    /// Room left on a window, in percent (100 when the window is not running).
    pub fn headroom(&self, name: &str) -> f64 {
        self.window(name)
            .map(|w| (100.0 - w.percent).max(0.0))
            .unwrap_or(100.0)
    }
}

pub fn parse(body: &Value, fetched_at: i64) -> Usage {
    let mut windows = vec![];
    for key in [
        "five_hour",
        "seven_day",
        "seven_day_opus",
        "seven_day_sonnet",
    ] {
        if let Some(w) = body.get(key).filter(|w| w.is_object()) {
            if let Some(p) = w.get("utilization").and_then(Value::as_f64) {
                windows.push(Window {
                    name: key.into(),
                    percent: p,
                    resets_at: w
                        .get("resets_at")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    active: false,
                    severity: None,
                });
            }
        }
    }
    for l in body
        .get("limits")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let model = l
            .pointer("/scope/model/display_name")
            .and_then(Value::as_str);
        let kind = l.get("kind").and_then(Value::as_str).unwrap_or("");
        let percent = l.get("percent").and_then(Value::as_f64).unwrap_or(0.0);
        let active = l.get("is_active").and_then(Value::as_bool).unwrap_or(false);
        let severity = l
            .get("severity")
            .and_then(Value::as_str)
            .map(str::to_string);
        let resets_at = l
            .get("resets_at")
            .and_then(Value::as_str)
            .map(str::to_string);
        match (model, kind) {
            (Some(m), _) => windows.push(Window {
                name: format!("model:{m}"),
                percent,
                resets_at,
                active,
                severity,
            }),
            (None, "weekly_all") => {
                if let Some(w) = windows.iter_mut().find(|w| w.name == "seven_day") {
                    w.active |= active;
                    w.severity = severity;
                }
            }
            _ => {}
        }
    }
    Usage {
        fetched_at,
        windows,
        resets: body.get("cedar_ember").and_then(parse_resets),
    }
}

/// Read an account's windows; with `with_resets`, also its offered resets (the request then
/// presents itself as the installed Claude Code; skipped when its version is unknown).
pub fn fetch(access_token: &str, with_resets: bool) -> Result<Usage> {
    let cc = if with_resets { claude_version() } else { None };
    let (url, ua) = match &cc {
        Some(v) => (
            format!("{URL}?cedar_ember=1&skip_spend=1"),
            format!("claude-cli/{v} (external, cli)"),
        ),
        None => (URL.to_string(), "bascule".to_string()),
    };
    let resp = ureq::get(&url)
        .set("Authorization", &format!("Bearer {access_token}"))
        .set("anthropic-beta", "oauth-2025-04-20")
        .set("Accept", "application/json")
        .set("User-Agent", &ua)
        .timeout(std::time::Duration::from_secs(20))
        .call();
    let body: Value = match resp {
        Ok(r) => r.into_json().context("usage: bad JSON")?,
        Err(ureq::Error::Status(429, r)) => {
            // The usage API answers "Retry-After: 0" while still limiting: wait 10 minutes then.
            let wait = r
                .header("retry-after")
                .and_then(|v| v.trim().parse::<u64>().ok())
                .filter(|w| *w > 0)
                .unwrap_or(600);
            anyhow::bail!("usage: HTTP 429, retry after {wait}s")
        }
        Err(ureq::Error::Status(code, _)) => anyhow::bail!("usage: HTTP {code}"),
        Err(e) => return Err(e).context("usage: request failed"),
    };
    Ok(parse(&body, chrono::Utc::now().timestamp_millis()))
}

/// Seconds to wait after a 429 from the usage API, read from the error message.
pub fn retry_after(err: &str) -> Option<u64> {
    let i = err.find("HTTP 429")?;
    err[i..]
        .split("retry after ")
        .nth(1)
        .and_then(|r| r.trim_end_matches('s').trim().parse().ok())
        .or(Some(600))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_windows_and_model_limits() {
        // Shape observed on 2026-11-18 (docs/internals.md).
        let body = json!({
            "five_hour": null,
            "seven_day": {"utilization": 80.0, "resets_at": "2026-11-24T07:00:00+00:00"},
            "seven_day_opus": null,
            "limits": [
                {"kind": "weekly_all", "group": "weekly", "percent": 80, "severity": "warning", "resets_at": "2026-11-24T07:00:00+00:00", "scope": null, "is_active": false},
                {"kind": "weekly_scoped", "group": "weekly", "percent": 100, "severity": "critical", "resets_at": "2026-11-24T07:00:00+00:00",
                 "scope": {"model": {"id": null, "display_name": "Fable"}, "surface": null}, "is_active": true}
            ]
        });
        let u = parse(&body, 1);
        assert_eq!(u.windows.len(), 2);
        assert_eq!(
            u.window("seven_day").unwrap().severity.as_deref(),
            Some("warning")
        );
        let fable = u.window("model:Fable").unwrap();
        assert_eq!(fable.percent, 100.0);
        assert!(fable.active);
        assert_eq!(u.binding().unwrap().name, "model:Fable");
        assert_eq!(u.headroom("five_hour"), 100.0);
        assert_eq!(u.headroom("seven_day"), 20.0);
        assert!(u.resets.is_none());
        let mut u2 = u.clone();
        u2.expire_resets(
            chrono::DateTime::parse_from_rfc3339("2026-11-25T00:00:00+00:00")
                .unwrap()
                .timestamp_millis(),
        );
        assert_eq!(u2.window("model:Fable").unwrap().percent, 0.0);
        let mut u3 = u.clone();
        u3.expire_resets(0);
        assert_eq!(u3.window("model:Fable").unwrap().percent, 100.0);
    }

    #[test]
    fn parses_reset_grants() {
        // Shape observed on 2026-11-18 (docs/internals.md).
        let body = json!({"seven_day": {"utilization": 16.0, "resets_at": null}, "cedar_ember": {
            "eligible": true, "ineligible_reason": null, "at_limit": false, "exhausted": [], "next_grant_id": "g2", "cooldown_until": null,
            "grants": [
                {"id": "g1", "label": "One full limit reset (November 2026)", "resets_total": 1, "resets_left": 0, "usable_now": false,
                 "use_requires_limit": false, "clears": ["seven_day", "seven_day_overage_included"], "ends_at": "2026-12-10T22:00:00+00:00", "paused": false, "type": "weekly"},
                {"id": "g2", "label": "New model launch: one Fable reset", "resets_total": 1, "resets_left": 1, "usable_now": true,
                 "use_requires_limit": false, "clears": ["seven_day_overage_included"], "ends_at": "2027-03-09T07:59:00+00:00", "paused": false, "type": "full"}]}});
        let u = parse(&body, 1);
        let r = u.resets.unwrap();
        assert!(r.eligible);
        let avail: Vec<_> = r.available().collect();
        assert_eq!(avail.len(), 1);
        assert!(avail[0].usable_now && avail[0].label.contains("Fable"));
        assert_eq!(avail[0].kind.as_deref(), Some("full"));
    }
}
