//! `bascule`: command line for the vault, the switch, quotas and the observer.

use anyhow::{bail, Context, Result};
use bascule_core::config::{AccountList, Settings};
use bascule_core::control;
mod native_host;
use bascule_core::jaunt::Jaunt;
use bascule_core::login::{self, Login};
use bascule_core::observe;
use bascule_core::paths::Paths;
use bascule_core::usage::Usage;
use bascule_core::vault::{self, Where};
use clap::{Parser, Subcommand};
use serde_json::json;
use std::io::{BufRead, Write};
use std::time::Duration;

#[derive(Parser)]
#[command(
    name = "bascule",
    version,
    about = "Switch Claude Code between your accounts before a usage limit"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Accounts, where their tokens are, and (with --quotas) their windows.
    Status {
        #[arg(long)]
        quotas: bool,
        #[arg(long)]
        json: bool,
    },
    /// Switch Claude Code to an account (all running sessions follow at their next request).
    Switch {
        name: String,
        #[arg(long)]
        yes: bool,
    },
    /// Sign an account in (or again), in its own vault directory.
    Login { name: String },
    /// Have Claude Code refresh a resting account's token now.
    Refresh { name: String },
    /// Claude sessions seen through jaunt.
    Sessions,
    /// Watch sessions and quotas and record events without acting. --once prints one snapshot.
    Observe {
        #[arg(long)]
        once: bool,
    },
    /// The service: observe, switch, resume, and serve the mini-app's socket.
    Daemon {
        /// Record only: never switch, never send.
        #[arg(long)]
        observe_only: bool,
    },
    /// The running daemon's state (JSON).
    State,
    /// Keep the connection to Chrome's DevTools open (started by the service when needed).
    #[command(hide = true)]
    CdpKeeper,
    /// Each account's usage windows as the service last read them: percentages and reset times
    /// only (no name, address or token), without any network request. --json for programs.
    Usage {
        #[arg(long)]
        json: bool,
    },
    /// Sessions waiting to be resumed; --send sends them the message (all, or those named).
    Resume {
        #[arg(long)]
        send: bool,
        #[arg(long)]
        add: bool,
        #[arg(long)]
        text: Option<String>,
        sessions: Vec<String>,
    },
    /// Save accounts to a JSON backup: identity, display name, icon, rotation, claude.ai session
    /// and Claude Code sign-in. Every account when none is named; the file goes to the downloads
    /// folder unless --out is given.
    Export {
        names: Vec<String>,
        #[arg(short, long)]
        out: Option<std::path::PathBuf>,
    },
    /// Add the accounts of a backup made by `bascule export`, and give the accounts already
    /// here what they lack, Claude Code sign-in included (nothing here is overwritten).
    Import { file: std::path::PathBuf },
    /// Effective settings.
    Config,
    /// Remove a vault copy of the live tokens left by an interrupted switch.
    Heal,
}

fn main() {
    // Started by a browser as its native messaging host: `bascule chrome-extension://<id>/ ...`.
    if std::env::args()
        .nth(1)
        .is_some_and(|a| a.starts_with("chrome-extension://"))
    {
        native_host::run();
        return;
    }
    if let Err(e) = run() {
        eprintln!("bascule: {e:#}");
        std::process::exit(1);
    }
}

fn load() -> Result<(Paths, AccountList, Settings)> {
    let paths = Paths::from_env();
    let list = AccountList::load(&paths.accounts_file())?;
    if list.accounts.is_empty() {
        bail!(
            "no accounts: list them in {}",
            paths.accounts_file().display()
        );
    }
    let settings = Settings::load(&paths.settings_file())?;
    Ok((paths, list, settings))
}

fn when(ms: Option<i64>) -> String {
    match ms.and_then(chrono::DateTime::from_timestamp_millis) {
        Some(t) => t
            .with_timezone(&chrono::Local)
            .format("%d/%m %H:%M")
            .to_string(),
        None => "-".into(),
    }
}

fn windows(u: &Usage) -> String {
    let mut parts: Vec<String> = u
        .windows
        .iter()
        .map(|w| {
            let reset = w
                .resets_at
                .as_deref()
                .and_then(|r| chrono::DateTime::parse_from_rfc3339(r).ok())
                .map(|t| {
                    t.with_timezone(&chrono::Local)
                        .format(" → %a %H:%M")
                        .to_string()
                })
                .unwrap_or_default();
            format!(
                "{} {:.0}%{}",
                w.name
                    .replace("seven_day", "week")
                    .replace("five_hour", "5h"),
                w.percent,
                reset
            )
        })
        .collect();
    if parts.is_empty() {
        parts.push("no window running".into());
    }
    if let Some(r) = &u.resets {
        let offered: Vec<String> = r
            .available()
            .map(|g| {
                format!(
                    "{}× {}{}",
                    g.resets_left,
                    g.label,
                    if g.usable_now {
                        " (now)"
                    } else if g.use_requires_limit {
                        " (at a limit)"
                    } else {
                        ""
                    }
                )
            })
            .collect();
        parts.push(if offered.is_empty() {
            "no reset left".into()
        } else {
            format!("resets: {}", offered.join("; "))
        });
    }
    parts.join(" · ")
}

fn run_daemon(
    paths: Paths,
    list: AccountList,
    settings: Settings,
    observe_only: bool,
) -> Result<()> {
    bascule_core::daemon::run_service(paths, list, settings, observe_only)
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Status { quotas, json } => {
            let (paths, list, settings) = load()?;
            if quotas {
                let views = observe::read_accounts(&paths, &list, settings.read_resets)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&views)?);
                    return Ok(());
                }
                for v in views {
                    let tag = match v.state.place {
                        Where::Active => "ACTIVE ",
                        Where::Resting => "resting",
                        Where::SignedOut => "signed out",
                    };
                    let detail = match (&v.usage, &v.usage_error) {
                        (Some(u), _) => windows(u),
                        (None, Some(e)) => e.clone(),
                        _ => String::new(),
                    };
                    println!(
                        "{:<8} {:<10} {:<24} {}",
                        v.state.name, tag, v.state.email, detail
                    );
                }
            } else {
                let states = vault::states(&paths, &list)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&states)?);
                    return Ok(());
                }
                for s in states {
                    let t = s.token.as_ref();
                    println!(
                        "{:<8} {:<10} {:<24} token until {}  refresh until {}",
                        s.name,
                        format!("{:?}", s.place).to_lowercase(),
                        s.email,
                        when(t.and_then(|t| t.expires_at)),
                        when(t.and_then(|t| t.refresh_expires_at))
                    );
                }
            }
        }
        Cmd::Export { names, out } => {
            let paths = Paths::from_env();
            let backup = bascule_core::backup::export(&paths, &names, chrono::Utc::now())?;
            if backup.accounts.is_empty() {
                bail!("no accounts to export");
            }
            let path = match out {
                Some(o) => std::path::absolute(o)?,
                None => bascule_core::backup::free_path(
                    &bascule_core::platform::downloads_dir(&paths.home),
                    &names,
                    chrono::Local::now(),
                ),
            };
            bascule_core::backup::write(&path, &backup)?;
            let with_web = backup
                .accounts
                .iter()
                .filter(|e| e.claude_ai.is_some())
                .count();
            observe::log(
                &paths,
                "accounts.export",
                json!({"accounts": backup.accounts.len(), "with_web": with_web}),
            );
            println!(
                "{} account(s) saved to {} ({} with a claude.ai session).\nThe file holds these accounts' sign-ins: keep it like a password.",
                backup.accounts.len(),
                path.display(),
                with_web
            );
        }
        Cmd::Import { file } => {
            let paths = Paths::from_env();
            let text = std::fs::read_to_string(&file)
                .with_context(|| format!("read {}", file.display()))?;
            let report =
                bascule_core::backup::import(&paths, &bascule_core::backup::parse(&text)?)?;
            observe::log(&paths, "accounts.import", serde_json::to_value(&report)?);
            for i in &report.accounts {
                let cli = match i.cli {
                    "restored" => "Claude Code restored",
                    "here" => "Claude Code already signed in",
                    "restored_elsewhere" => "Claude Code restored from another computer (if it stays signed in there, the first to renew keeps it)",
                    "expired" => "Claude Code: expired in the backup, sign in again",
                    _ => "Claude Code: not in the backup",
                };
                let web = match i.web {
                    "restored" => "claude.ai restored",
                    "here" => "claude.ai already saved",
                    _ => "no claude.ai session",
                };
                println!("{:<10} {:<12} {cli}; {web}", i.status, i.name);
            }
        }
        Cmd::Switch { name, yes } => {
            let (paths, list, _) = load()?;
            let from = vault::active_account(&paths, &list)
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "?".into());
            if !yes {
                eprint!("Switch Claude Code from {from} to {name}? Every running session follows at its next request. [y/N] ");
                std::io::stderr().flush()?;
                let mut answer = String::new();
                std::io::stdin().lock().read_line(&mut answer)?;
                if !matches!(answer.trim(), "y" | "Y" | "o" | "O" | "yes" | "oui") {
                    bail!("cancelled");
                }
            }
            // Through the daemon when it runs (it then knows, and resumes blocked sessions).
            if control::is_running(&paths) {
                control::call(&paths, "switch", json!({"to": name}))?;
                println!("Switched {from} → {name}.");
                return Ok(());
            }
            let report = vault::switch(&paths, &list, &name)?;
            observe::log(&paths, "switch", json!({"manual": true, "report": report}));
            std::thread::sleep(Duration::from_secs(3));
            if vault::recheck_account(&paths, &list, &name)? {
                observe::log(&paths, "switch.account_rewritten", json!({"to": name}));
            }
            println!(
                "Switched {} → {}.",
                report.from.as_deref().unwrap_or("?"),
                report.to
            );
        }
        Cmd::Login { name } => {
            let (paths, list, _) = load()?;
            let account = list
                .by_name(&name)
                .with_context(|| format!("unknown account '{name}'"))?
                .clone();
            let mut l = Login::start(&paths, &account)?;
            eprintln!("Signing in {} ({}). Claude Code opens your browser; if the page shows a code, paste it here.", account.name, account.email);
            let (tx, rx) = std::sync::mpsc::channel::<String>();
            std::thread::spawn(move || {
                for line in std::io::stdin().lock().lines().map_while(Result::ok) {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            });
            loop {
                while let Ok(line) = l.lines.try_recv() {
                    eprintln!("  {line}");
                }
                if let Ok(code) = rx.try_recv() {
                    if !code.trim().is_empty() {
                        l.submit_code(&code)?;
                    }
                }
                if let Some(status) = l.child.try_wait()? {
                    while let Ok(line) = l.lines.try_recv() {
                        eprintln!("  {line}");
                    }
                    if !status.success() {
                        bail!("claude auth login failed ({status})");
                    }
                    break;
                }
                std::thread::sleep(Duration::from_millis(150));
            }
            login::verify(&paths, &account)?;
            observe::log(&paths, "login", json!({"name": account.name}));
            println!("{} signed in.", account.name);
        }
        Cmd::Refresh { name } => {
            let (paths, list, _) = load()?;
            let is_main = vault::active_account(&paths, &list)
                .map(|a| a.name == name)
                .unwrap_or(false);
            let slot = if is_main {
                paths.main.clone()
            } else {
                paths.account_slot(&name)
            };
            let before = vault::describe(&slot);
            vault::refresh(&slot, is_main)?;
            println!("{name}: {before} → {}", vault::describe(&slot));
        }
        Cmd::Sessions => {
            let paths = Paths::from_env();
            let mut j = Jaunt::connect(&paths)?;
            for s in j.summary(None)?.into_iter().filter(|s| s.is_claude()) {
                let lt = &s.last_turn;
                println!(
                    "{:<18} {:<8} {:<34} last turn: {}{}{}",
                    s.session,
                    s.status.as_deref().unwrap_or("-"),
                    s.title.chars().take(34).collect::<String>(),
                    lt.reason.as_deref().unwrap_or("-"),
                    lt.limit_kind
                        .as_deref()
                        .map(|k| format!(" ({k})"))
                        .unwrap_or_default(),
                    s.screen_title
                        .as_deref()
                        .map(|t| format!("  [dialog: {t}]"))
                        .unwrap_or_default()
                );
            }
        }
        Cmd::Observe { once } => {
            let (paths, list, settings) = load()?;
            if once {
                let views = observe::read_accounts(&paths, &list, settings.read_resets)?;
                let mut j = Jaunt::connect(&paths)?;
                let sessions: Vec<_> = j.summary(None)?.into_iter().filter(|s| s.is_claude()).map(|s| json!({"session": s.session, "status": s.status, "lastTurn": s.last_turn, "limits": s.limits})).collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &json!({"accounts": views, "sessions": sessions})
                    )?
                );
            } else {
                run_daemon(paths, list, settings, true)?;
            }
        }
        Cmd::Daemon { observe_only } => {
            // Starts with no account too: the first run imports the signed-in one, the app adds more.
            let paths = Paths::from_env();
            let list = AccountList::load(&paths.accounts_file())?;
            let settings = Settings::load(&paths.settings_file())?;
            run_daemon(paths, list, settings, observe_only)?;
        }
        Cmd::Resume {
            send,
            add,
            text,
            sessions,
        } => {
            let paths = Paths::from_env();
            if add {
                println!(
                    "{}",
                    control::call(&paths, "resume.add", json!({"sessions": sessions}))?
                );
                return Ok(());
            }
            let st = control::call(&paths, "state", json!({}))?;
            let entries = st["resume"].as_array().cloned().unwrap_or_default();
            if send {
                let ids: Vec<String> = if sessions.is_empty() {
                    entries
                        .iter()
                        .filter(|e| {
                            e["state"] == "ready"
                                || e["state"] == "needs_you"
                                || e["state"] == "failed"
                        })
                        .filter_map(|e| e["session"].as_str().map(str::to_string))
                        .collect()
                } else {
                    sessions
                };
                let texts: serde_json::Map<String, serde_json::Value> = match &text {
                    Some(t) => ids.iter().map(|i| (i.clone(), json!(t))).collect(),
                    None => Default::default(),
                };
                control::call(
                    &paths,
                    "resume.send",
                    json!({"sessions": ids, "texts": texts}),
                )?;
                println!("sent to {} session(s)", ids.len());
            } else {
                for e in entries {
                    println!(
                        "{:<18} {:<10} {}  {}",
                        e["session"].as_str().unwrap_or(""),
                        e["state"].as_str().unwrap_or(""),
                        e["title"].as_str().unwrap_or(""),
                        e["note"].as_str().unwrap_or("")
                    );
                }
                println!("message: {}", st["default_message"].as_str().unwrap_or(""));
            }
        }
        Cmd::Usage { json } => {
            let paths = Paths::from_env();
            let state = control::call(&paths, "state", json!({}))?;
            let out = usage_view(&state, chrono::Utc::now().timestamp_millis());
            if json {
                println!("{}", serde_json::to_string(&out)?);
            } else {
                for (i, a) in out["accounts"].as_array().into_iter().flatten().enumerate() {
                    let wins: Vec<String> = a["windows"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|w| {
                            format!(
                                "{} {:.0}%",
                                w["name"].as_str().unwrap_or("?"),
                                w["percent"].as_f64().unwrap_or(0.0)
                            )
                        })
                        .collect();
                    println!(
                        "{} {:<6} {}",
                        i + 1,
                        a["place"].as_str().unwrap_or(""),
                        wins.join(" · ")
                    );
                }
            }
        }
        Cmd::CdpKeeper => {
            bascule_core::cdp::run_keeper(&Paths::from_env())?;
        }
        Cmd::State => {
            let paths = Paths::from_env();
            println!(
                "{}",
                serde_json::to_string_pretty(&control::call(&paths, "state", json!({}))?)?
            );
        }
        Cmd::Config => {
            let (paths, _, settings) = load()?;
            println!(
                "# {}\n{}",
                paths.settings_file().display(),
                toml::to_string_pretty(&settings)?
            );
            println!("# resume message: {}", settings.resume_text());
        }
        Cmd::Heal => {
            let (paths, list, _) = load()?;
            let fixed = vault::heal(&paths, &list)?;
            println!(
                "{}",
                if fixed.is_empty() {
                    "nothing to heal".to_string()
                } else {
                    format!("removed duplicate copies: {}", fixed.join(", "))
                }
            );
        }
    }
    Ok(())
}

/// `bascule usage`: numbers only. A window whose reset time has passed counts as 0 % (the service
/// may not have read the account again since).
fn usage_view(state: &serde_json::Value, now_ms: i64) -> serde_json::Value {
    use serde_json::Value;
    let iso = |ms: i64| {
        chrono::DateTime::from_timestamp_millis(ms)
            .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
    };
    let mut latest: Option<i64> = None;
    let accounts: Vec<Value> = state["accounts"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|a| {
            let place = if a["place"].as_str() == Some("active") { "active" } else { "rest" };
            let fetched = a["usage"]["fetched_at"].as_i64();
            if let Some(f) = fetched {
                latest = Some(latest.map_or(f, |l| l.max(f)));
            }
            let windows: Vec<Value> = a["usage"]["windows"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|w| {
                    let resets = w["resets_at"].as_str().map(str::to_string);
                    let passed = resets
                        .as_deref()
                        .and_then(|r| chrono::DateTime::parse_from_rfc3339(r).ok())
                        .is_some_and(|t| t.timestamp_millis() <= now_ms);
                    json!({
                        "name": w["name"],
                        "percent": if passed { 0.0 } else { w["percent"].as_f64().unwrap_or(0.0) },
                        "resets_at": resets
                            .as_deref()
                            .and_then(|r| chrono::DateTime::parse_from_rfc3339(r).ok())
                            .map(|t| t.with_timezone(&chrono::Utc).to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
                    })
                })
                .collect();
            json!({"place": place, "fetched_at": fetched.and_then(iso), "windows": windows})
        })
        .collect();
    json!({"fetched_at": latest.and_then(iso), "accounts": accounts})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_is_numbers_only() {
        let state = json!({"accounts": [
            {"name": "perso", "email": "someone@example.com", "label": "Perso", "place": "active",
             "token": {"expires_at": 1}, "usage": {"fetched_at": 1795000000000i64, "windows": [
                {"name": "seven_day", "percent": 71.0, "resets_at": "2026-11-24T07:00:00+00:00", "severity": "warning", "active": true},
                {"name": "model:Fable", "percent": 100.0, "resets_at": "2026-11-01T00:00:00+00:00"}]}},
            {"name": "work", "email": "w@example.com", "place": "resting", "usage": null}]});
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-20T00:00:00+00:00")
            .unwrap()
            .timestamp_millis();
        let v = usage_view(&state, now);
        let text = v.to_string();
        for secret in [
            "perso",
            "someone",
            "Perso",
            "example.com",
            "work",
            "token",
            "severity",
        ] {
            assert!(!text.contains(secret), "{secret} leaked: {text}");
        }
        assert_eq!(v["accounts"][0]["place"], "active");
        assert_eq!(v["accounts"][0]["windows"][0]["percent"], 71.0);
        assert_eq!(
            v["accounts"][0]["windows"][0]["resets_at"],
            "2026-11-24T07:00:00Z"
        );
        assert_eq!(v["accounts"][0]["windows"][1]["percent"], 0.0); // reset already passed
        assert_eq!(v["accounts"][1]["place"], "rest");
        assert_eq!(v["accounts"][1]["windows"].as_array().unwrap().len(), 0);
        assert!(v["fetched_at"].as_str().unwrap().starts_with("2026-11-18T"));
    }
}
