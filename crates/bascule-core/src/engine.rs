//! Decisions, as pure functions (no I/O): which window is over its threshold, which account to
//! switch to, when an exhausted account comes back. See docs/internals.md, "Switching rules".

use crate::config::Settings;
use crate::usage::Usage;
use serde::Serialize;
use std::collections::BTreeMap;

/// Windows shared by every model. Model-scoped windows are named `model:<display name>`.
pub const GLOBAL_WINDOWS: [&str; 2] = ["five_hour", "seven_day"];

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Candidate {
    pub name: String,
    /// Has tokens in the vault (resting) and is in the rotation.
    pub available: bool,
    pub usage: Option<Usage>,
}

/// An account known to be at its wall on some windows, until a time (ms).
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Exhausted {
    pub windows: Vec<String>,
    pub until_ms: i64,
}

pub type ExhaustedMap = BTreeMap<String, Exhausted>;

fn is_global(w: &str) -> bool {
    GLOBAL_WINDOWS.contains(&w)
}

/// The model a model-scoped window concerns, lowercase: `model:Fable` → `fable`,
/// `seven_day_opus` → `opus`, `seven_day_sonnet` → `sonnet`; None for the 5 h and week windows.
pub fn window_model(w: &str) -> Option<String> {
    if let Some(m) = w.strip_prefix("model:") {
        return Some(m.to_lowercase());
    }
    match w {
        "seven_day_opus" => Some("opus".into()),
        "seven_day_sonnet" => Some("sonnet".into()),
        _ => None,
    }
}

/// Whether one of the models working sessions use (ids such as `claude-fable-5-2`) falls in this window.
pub fn window_in_use(w: &str, working_models: &[String]) -> bool {
    match window_model(w) {
        None => true,
        Some(m) => working_models
            .iter()
            .any(|id| id.to_lowercase().contains(&m)),
    }
}

/// Settings key of a window for per-window thresholds.
pub fn threshold_key(w: &str) -> &str {
    if window_model(w).is_some() {
        "model"
    } else {
        w
    }
}

/// Windows of the active account at or over their anticipation threshold. A model-scoped window
/// counts only with `anticipate_model_windows` and only while a working session uses that model:
/// switching away for a model nobody is using would spend another account
/// for nothing; an idle session on that model is covered by the switch on limit.
pub fn over_threshold(
    settings: &Settings,
    usage: &Usage,
    working_models: &[String],
) -> Vec<(String, f64)> {
    usage
        .windows
        .iter()
        .filter(|w| {
            is_global(&w.name)
                || (settings.anticipate_model_windows
                    && window_model(&w.name).is_some()
                    && window_in_use(&w.name, working_models))
        })
        .filter(|w| w.percent >= settings.threshold_for(threshold_key(&w.name)))
        .map(|w| (w.name.clone(), w.percent))
        .collect()
}

/// Windows at their wall (100 %), with their reset time in ms when known.
pub fn at_wall(usage: &Usage) -> Vec<(String, Option<i64>)> {
    usage
        .windows
        .iter()
        .filter(|w| w.percent >= 100.0)
        .map(|w| (w.name.clone(), w.resets_at.as_deref().and_then(parse_ms)))
        .collect()
}

pub fn parse_ms(iso: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(iso)
        .ok()
        .map(|t| t.timestamp_millis())
}

/// The windows a limit event concerns. `limitKind` from jaunt: `five_hour`, `weekly`, `model`, `credits`.
/// For `model`, the active account's model windows at their wall (or its fullest model window).
pub fn windows_of_limit(kind: &str, active: Option<&Usage>) -> Vec<String> {
    match kind {
        "five_hour" => vec!["five_hour".into()],
        "weekly" => vec!["seven_day".into()],
        "model" => {
            let models: Vec<_> = active
                .map(|u| {
                    u.windows
                        .iter()
                        .filter(|w| w.name.starts_with("model:"))
                        .collect()
                })
                .unwrap_or_default();
            let full: Vec<String> = models
                .iter()
                .filter(|w| w.percent >= 100.0)
                .map(|w| w.name.clone())
                .collect();
            if !full.is_empty() {
                full
            } else {
                models
                    .iter()
                    .max_by(|a, b| a.percent.total_cmp(&b.percent))
                    .map(|w| vec![w.name.clone()])
                    .unwrap_or_default()
            }
        }
        _ => vec![],
    }
}

fn percent(u: &Usage, w: &str) -> f64 {
    u.window(w).map(|x| x.percent).unwrap_or(0.0)
}

/// Pick the account to switch to.
/// `needed`: the windows that must have room; `ceiling(w)`: a candidate qualifies when its
/// percent on `w` is below it (the threshold when anticipating, 100 on a limit).
pub fn pick_next(
    settings: &Settings,
    candidates: &[Candidate],
    active: Option<&str>,
    exhausted: &ExhaustedMap,
    needed: &[String],
    ceiling: &dyn Fn(&str) -> f64,
    now_ms: i64,
) -> Option<String> {
    let ok = |c: &&Candidate| -> bool {
        if !c.available || Some(c.name.as_str()) == active {
            return false;
        }
        if let Some(e) = exhausted.get(&c.name) {
            if e.until_ms > now_ms {
                return false;
            }
        }
        let Some(u) = &c.usage else { return false };
        if GLOBAL_WINDOWS.iter().any(|w| percent(u, w) >= 100.0) {
            return false;
        }
        needed.iter().all(|w| percent(u, w) < ceiling(w))
    };
    let eligible: Vec<&Candidate> = candidates.iter().filter(ok).collect();
    if eligible.is_empty() {
        return None;
    }
    if settings.next_account_rule == "order" {
        let order: Vec<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
        let start = active
            .and_then(|a| order.iter().position(|n| *n == a))
            .map(|i| i + 1)
            .unwrap_or(0);
        return (0..order.len())
            .map(|k| order[(start + k) % order.len()])
            .find(|n| eligible.iter().any(|c| c.name == *n))
            .map(str::to_string);
    }
    // Headroom: the most room on the windows concerned, then the most room on the week.
    let room = |c: &Candidate| {
        let u = c.usage.as_ref().unwrap();
        let worst = needed
            .iter()
            .map(|w| 100.0 - percent(u, w))
            .fold(f64::INFINITY, f64::min);
        (
            if worst.is_finite() { worst } else { 100.0 },
            100.0 - percent(u, "seven_day"),
        )
    };
    eligible
        .into_iter()
        .max_by(|a, b| {
            let (ra, rb) = (room(a), room(b));
            ra.0.total_cmp(&rb.0).then(ra.1.total_cmp(&rb.1))
        })
        .map(|c| c.name.clone())
}

/// When nothing is available: the exhausted account that comes back first (with the margin).
pub fn next_return(
    settings: &Settings,
    exhausted: &ExhaustedMap,
    candidates: &[Candidate],
) -> Option<(String, i64)> {
    exhausted
        .iter()
        .filter(|(n, _)| candidates.iter().any(|c| &c.name == *n))
        .map(|(n, e)| {
            (
                n.clone(),
                e.until_ms + settings.reset_margin_seconds as i64 * 1000,
            )
        })
        .min_by_key(|(_, t)| *t)
}

/// The rotation: `settings.rotation` in its order, else every account in list order.
pub fn rotation(settings: &Settings, all: &[String]) -> Vec<String> {
    if settings.rotation.is_empty() {
        all.to_vec()
    } else {
        settings
            .rotation
            .iter()
            .filter(|n| all.contains(n))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::Window;

    fn usage(ws: &[(&str, f64)]) -> Usage {
        Usage {
            fetched_at: 0,
            windows: ws
                .iter()
                .map(|(n, p)| Window {
                    name: n.to_string(),
                    percent: *p,
                    resets_at: Some("2026-11-24T07:00:00+00:00".into()),
                    active: false,
                    severity: None,
                })
                .collect(),
            resets: None,
        }
    }

    fn cand(name: &str, ws: &[(&str, f64)]) -> Candidate {
        Candidate {
            name: name.into(),
            available: true,
            usage: Some(usage(ws)),
        }
    }

    #[test]
    fn model_windows_count_only_while_a_working_session_uses_the_model() {
        let s = Settings::default();
        let u = usage(&[
            ("seven_day", 81.0),
            ("model:Fable", 100.0),
            ("seven_day_opus", 97.0),
        ]);
        let fable = vec!["claude-fable-5-2".to_string()];
        let opus = vec!["claude-opus-5-5".to_string()];
        assert_eq!(
            over_threshold(&s, &u, &fable),
            vec![("model:Fable".to_string(), 100.0)]
        );
        assert_eq!(
            over_threshold(&s, &u, &opus),
            vec![("seven_day_opus".to_string(), 97.0)]
        );
        assert!(
            over_threshold(&s, &u, &[]).is_empty(),
            "nobody works on Fable or Opus"
        );
        let mut s2 = s.clone();
        s2.anticipate_model_windows = false;
        assert!(over_threshold(&s2, &u, &fable).is_empty());
        let u = usage(&[("five_hour", 96.0), ("seven_day", 50.0)]);
        assert_eq!(over_threshold(&s, &u, &[])[0].0, "five_hour");
        assert_eq!(window_model("model:Fable").as_deref(), Some("fable"));
        assert!(window_in_use("seven_day", &[]));
    }

    #[test]
    fn picks_most_headroom_on_the_model_window() {
        // Real numbers from 2026-11-18: "a" active, Fable exhausted on "a" and "b".
        let s = Settings::default();
        let cands = vec![
            cand("a", &[("seven_day", 81.0), ("model:Fable", 100.0)]),
            cand("b", &[("seven_day", 56.0), ("model:Fable", 100.0)]),
            cand("c", &[("seven_day", 14.0), ("model:Fable", 24.0)]),
        ];
        let needed = windows_of_limit("model", cands[0].usage.as_ref());
        assert_eq!(needed, vec!["model:Fable".to_string()]);
        let got = pick_next(
            &s,
            &cands,
            Some("a"),
            &ExhaustedMap::new(),
            &needed,
            &|_| 100.0,
            0,
        );
        assert_eq!(got.as_deref(), Some("c"));
        // A weekly limit: "b" and "c" both qualify, "c" has more room.
        let got = pick_next(
            &s,
            &cands,
            Some("a"),
            &ExhaustedMap::new(),
            &["seven_day".into()],
            &|_| 100.0,
            0,
        );
        assert_eq!(got.as_deref(), Some("c"));
    }

    #[test]
    fn respects_exhausted_marks_ceiling_and_order_rule() {
        let mut s = Settings::default();
        let cands = vec![
            cand("a", &[("seven_day", 10.0)]),
            cand("b", &[("seven_day", 96.0)]),
            cand("c", &[("seven_day", 50.0)]),
        ];
        let mut ex = ExhaustedMap::new();
        ex.insert(
            "c".into(),
            Exhausted {
                windows: vec!["five_hour".into()],
                until_ms: 1000,
            },
        );
        // c exhausted until 1000, b over the 95 % ceiling: nobody at t=500.
        assert_eq!(
            pick_next(
                &s,
                &cands,
                Some("a"),
                &ex,
                &["seven_day".into()],
                &|_| 95.0,
                500
            ),
            None
        );
        assert_eq!(
            pick_next(
                &s,
                &cands,
                Some("a"),
                &ex,
                &["seven_day".into()],
                &|_| 95.0,
                2000
            )
            .as_deref(),
            Some("c")
        );
        assert_eq!(
            next_return(&s, &ex, &cands),
            Some(("c".into(), 1000 + 120_000))
        );
        s.next_account_rule = "order".into();
        assert_eq!(
            pick_next(
                &s,
                &cands,
                Some("a"),
                &ExhaustedMap::new(),
                &["seven_day".into()],
                &|_| 100.0,
                0
            )
            .as_deref(),
            Some("b")
        );
    }

    #[test]
    fn never_picks_an_account_at_a_global_wall_or_unavailable() {
        let s = Settings::default();
        let mut cands = vec![
            cand("a", &[("five_hour", 100.0)]),
            cand("b", &[("seven_day", 20.0)]),
        ];
        cands[1].available = false;
        assert_eq!(
            pick_next(&s, &cands, None, &ExhaustedMap::new(), &[], &|_| 100.0, 0),
            None
        );
    }
}
