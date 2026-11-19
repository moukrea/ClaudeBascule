//! Phase 4, the daemon: observes (phase 1), decides (engine) and acts — anticipated switch,
//! switch on limit with a collection window, scheduled return, resume of blocked sessions.
//! It serves its state and takes commands through `control` (the mini-app's socket).

use crate::config::{AccountList, Settings};
use crate::engine::{self, Candidate, Exhausted, ExhaustedMap};
use crate::jaunt::{Jaunt, SessionSummary};
use crate::observe::{log, now_ms, AccountView};
use crate::paths::Paths;
use crate::usage::{self, Usage};
use crate::vault::{self, Where};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

pub type Reply = Sender<Result<Value, String>>;

pub enum Cmd {
    Switch {
        to: String,
        reply: Reply,
    },
    Reread {
        reply: Reply,
    },
    Send {
        sessions: Vec<String>,
        texts: BTreeMap<String, String>,
        reply: Reply,
    },
    Dismiss {
        sessions: Vec<String>,
        reply: Reply,
    },
    /// The user adds sessions to the resume list by hand (ready at once).
    Add {
        sessions: Vec<String>,
        reply: Reply,
    },
    /// A sign-in finished for the active account: move its new tokens into the main slot.
    AdoptLogin {
        account: String,
        reply: Reply,
    },
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EntryState {
    /// Every account is at its wall: waits for a return.
    Waiting,
    /// Collecting: other sessions may still fail on the old account, or it may restart alone.
    Pending,
    /// Ready to be sent the resume message (the user confirms, unless auto-send).
    Ready,
    Sending,
    Sent,
    Resumed,
    /// A dialog Bascule must not answer covers the prompt.
    NeedsYou,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
pub struct Entry {
    pub session: String,
    pub title: String,
    pub cwd: Option<String>,
    pub blocked_at_ms: i64,
    pub limit_kind: Option<String>,
    pub state: EntryState,
    pub text: Option<String>,
    pub ready_at_ms: i64,
    pub updated_ms: i64,
    pub note: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SwitchInfo {
    pub at_ms: i64,
    pub from: Option<String>,
    pub to: String,
    pub reason: String,
    pub manual: bool,
}

pub struct Daemon {
    paths: Paths,
    list: AccountList,
    settings: Settings,
    settings_mtime: Option<SystemTime>,
    observe_only: bool,
    accounts: BTreeMap<String, AccountView>,
    read_at: HashMap<String, Instant>,
    /// Usage API answered 429: no read before this instant.
    backoff: HashMap<String, Instant>,
    /// Network failures in a row per account, and when to try again.
    failures: HashMap<String, u32>,
    retry_at: HashMap<String, Instant>,
    /// Status-line windows already applied (`fetchedAt` of the last one).
    live_applied: i64,
    exhausted: ExhaustedMap,
    last_switch: Option<SwitchInfo>,
    rechecks: Vec<(Instant, String)>,
    sessions: BTreeMap<String, SessionSummary>,
    seen: HashMap<String, String>,
    handled_limits: HashMap<String, i64>,
    /// When each session was last seen busy (making requests).
    last_busy: HashMap<String, i64>,
    /// Last known plan tier per account.
    tiers: HashMap<String, String>,
    /// Model each conversation last answered with (from its transcript): (file mtime, model id).
    models: HashMap<String, (std::time::SystemTime, String)>,
    anticipation_idle_logged: Option<Instant>,
    resume: BTreeMap<String, Entry>,
    waiting_return: Option<(String, i64)>,
    events: VecDeque<Value>,
    jaunt: Option<Jaunt>,
    jaunt_state: String,
    jaunt_retry: Instant,
    jaunt_backoff: Duration,
    next_summary: Instant,
    dirty: bool,
    last_summary: Instant,
    anticipation_miss_logged: Option<Instant>,
    notified_ready: usize,
    shared: Arc<Mutex<Value>>,
    rx: Receiver<Cmd>,
    /// Latest release check, shared with the control endpoint (`update.check`).
    pub updates: Arc<Mutex<crate::update::UpdateState>>,
    /// claude.ai in the browser (the Connector extension), shared with the control endpoint.
    pub web: Arc<Mutex<crate::web::WebHub>>,
    update_busy: Arc<std::sync::atomic::AtomicBool>,
    update_tried_ms: i64,
}

/// Notification strings in the six languages; {n}, {a}, {t} are filled by the caller.
fn tr(lang: &str, key: &str) -> String {
    let row: [&str; 6] = match key {
        "switched" => [
            "Claude account switched",
            "Compte Claude changé",
            "Claude-Konto gewechselt",
            "Cuenta de Claude cambiada",
            "Account Claude cambiato",
            "Conta Claude trocada",
        ],
        "open" => ["Open", "Ouvrir", "Öffnen", "Abrir", "Apri", "Abrir"],
        "cic_title" => [
            "Claude in Chrome: sign in again",
            "Claude in Chrome : reconnexion",
            "Claude in Chrome: neu anmelden",
            "Claude in Chrome: vuelve a iniciar sesión",
            "Claude in Chrome: accedi di nuovo",
            "Claude in Chrome: inicie sessão de novo",
        ],
        "cic_body" => [
            "To follow {a}: Log out, then Sign in in the extension (no code needed).",
            "Pour passer sur {a} : Log out puis Sign in dans l'extension (sans code).",
            "Für {a}: in der Erweiterung Log out, dann Sign in (ohne Code).",
            "Para pasar a {a}: Log out y luego Sign in en la extensión (sin código).",
            "Per passare a {a}: Log out e poi Sign in nell'estensione (senza codice).",
            "Para passar para {a}: Log out e depois Sign in na extensão (sem código).",
        ],
        "resume_one" => [
            "1 session to resume",
            "1 session à relancer",
            "1 Sitzung fortzusetzen",
            "1 sesión por reanudar",
            "1 sessione da riprendere",
            "1 sessão por retomar",
        ],
        "resume_other" => [
            "{n} sessions to resume",
            "{n} sessions à relancer",
            "{n} Sitzungen fortzusetzen",
            "{n} sesiones por reanudar",
            "{n} sessioni da riprendere",
            "{n} sessões por retomar",
        ],
        "all_out" => [
            "No account left.",
            "Plus aucun compte disponible.",
            "Kein Konto mehr verfügbar.",
            "No queda ninguna cuenta.",
            "Nessun account disponibile.",
            "Nenhuma conta disponível.",
        ],
        "back_at" => [
            "Next back: {a} at {t}.",
            "Retour prévu : {a} à {t}.",
            "Zurück: {a} um {t}.",
            "Vuelta prevista: {a} a las {t}.",
            "Ritorno previsto: {a} alle {t}.",
            "Regresso previsto: {a} às {t}.",
        ],
        "offered" => [
            "Offered reset available: {a}.",
            "Reset offert disponible : {a}.",
            "Angebotener Reset verfügbar: {a}.",
            "Reinicio ofrecido disponible: {a}.",
            "Reset offerto disponibile: {a}.",
            "Reposição oferecida disponível: {a}.",
        ],
        "not_restarted" => [
            "the session did not restart",
            "la session n'est pas repartie",
            "die Sitzung lief nicht wieder an",
            "la sesión no se reanudó",
            "la sessione non è ripartita",
            "a sessão não retomou",
        ],
        _ => [key; 6],
    };
    let i = crate::config::LANGUAGES
        .iter()
        .position(|l| *l == lang)
        .unwrap_or(0);
    row[i].to_string()
}

fn hhmm(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|d| d.with_timezone(&chrono::Local).format("%H:%M").to_string())
        .unwrap_or_default()
}

impl Daemon {
    pub fn new(
        paths: Paths,
        list: AccountList,
        settings: Settings,
        observe_only: bool,
        shared: Arc<Mutex<Value>>,
        rx: Receiver<Cmd>,
    ) -> Self {
        Daemon {
            settings_mtime: std::fs::metadata(paths.settings_file())
                .and_then(|m| m.modified())
                .ok(),
            paths,
            list,
            settings,
            observe_only,
            accounts: BTreeMap::new(),
            read_at: HashMap::new(),
            backoff: HashMap::new(),
            failures: HashMap::new(),
            retry_at: HashMap::new(),
            live_applied: 0,
            exhausted: ExhaustedMap::new(),
            last_switch: None,
            rechecks: vec![],
            sessions: BTreeMap::new(),
            seen: HashMap::new(),
            handled_limits: HashMap::new(),
            last_busy: HashMap::new(),
            tiers: HashMap::new(),
            models: HashMap::new(),
            anticipation_idle_logged: None,
            resume: BTreeMap::new(),
            waiting_return: None,
            events: VecDeque::new(),
            jaunt: None,
            jaunt_state: "not connected".into(),
            jaunt_retry: Instant::now(),
            jaunt_backoff: Duration::from_secs(2),
            next_summary: Instant::now(),
            dirty: true,
            last_summary: Instant::now() - Duration::from_secs(60),
            anticipation_miss_logged: None,
            notified_ready: 0,
            shared,
            rx,
            update_busy: Default::default(),
            web: Default::default(),
            update_tried_ms: 0,
            updates: Arc::new(Mutex::new(crate::update::UpdateState {
                current: env!("CARGO_PKG_VERSION").into(),
                ..Default::default()
            })),
        }
    }

    fn lang(&self) -> String {
        self.settings.lang()
    }

    fn event(&mut self, kind: &str, data: Value) {
        log(&self.paths, kind, data.clone());
        self.events
            .push_back(json!({"at": now_ms(), "kind": kind, "data": data}));
        while self.events.len() > 60 {
            self.events.pop_front();
        }
    }

    fn notify(&self, title: &str, body: &str, open_app: bool) {
        if !self.settings.notify {
            return;
        }
        let (title, body) = (title.to_string(), body.to_string());
        let open_label = tr(&self.lang(), "open");
        let paths = self.paths.clone();
        std::thread::spawn(move || {
            if crate::platform::notify(&title, &body, open_app.then_some(open_label.as_str())) {
                launch_app(&paths);
            }
        });
    }

    /// An account as the user wants it shown (display name, email, or masked email).
    fn label(&self, name: &str) -> String {
        let email = self
            .list
            .by_name(name)
            .map(|a| a.email.clone())
            .unwrap_or_default();
        self.settings.label_for(name, &email)
    }

    fn active(&self) -> Option<String> {
        vault::active_account(&self.paths, &self.list).map(|a| a.name.clone())
    }

    // ---- accounts -------------------------------------------------------------------------

    fn read_account(&mut self, name: &str) {
        let states = match vault::states(&self.paths, &self.list) {
            Ok(s) => s,
            Err(e) => {
                self.event(
                    "error",
                    json!({"where": "states", "error": format!("{e:#}")}),
                );
                return;
            }
        };
        let Some(st) = states.into_iter().find(|s| s.name == name) else {
            return;
        };
        let (usage, usage_error) = match st.place {
            Where::SignedOut => (None, Some("signed out".to_string())),
            ref place => {
                let is_main = *place == Where::Active;
                let slot = if is_main {
                    self.paths.main.clone()
                } else {
                    self.paths.account_slot(name)
                };
                match vault::fresh_access(&slot, is_main, 10 * 60 * 1000).and_then(
                    |tok| match tok {
                        Some(tok) => usage::fetch(&tok, self.settings.read_resets),
                        None => anyhow::bail!("no token"),
                    },
                ) {
                    Ok(u) => (Some(u), None),
                    Err(e) => (None, Some(format!("{e:#}"))),
                }
            }
        };
        // A network failure (no connection yet, a proxy, a certificate) is retried soon, with a
        // growing delay, rather than at the next regular read.
        match usage_error.as_deref() {
            Some(e) if usage::retry_after(e).is_none() && e != "signed out" => {
                let n = self.failures.entry(name.to_string()).or_insert(0);
                *n += 1;
                let delay = (30u64 << (*n - 1).min(4)).min(300);
                self.retry_at.insert(
                    name.to_string(),
                    Instant::now() + Duration::from_secs(delay),
                );
            }
            _ => {
                self.failures.remove(name);
                self.retry_at.remove(name);
            }
        }
        if let Some(wait) = usage_error.as_deref().and_then(usage::retry_after) {
            let wait = wait.max(self.settings.rate_limit_backoff_minutes.max(1) * 60);
            self.backoff
                .insert(name.to_string(), Instant::now() + Duration::from_secs(wait));
            self.event(
                "usage.rate_limited",
                json!({"account": name, "retry_after_s": wait}),
            );
        }
        let state = vault::states(&self.paths, &self.list)
            .ok()
            .and_then(|v| v.into_iter().find(|s| s.name == name))
            .unwrap_or(st);
        if let Some(t) = state.token.as_ref().and_then(|t| t.tier.clone()) {
            self.tiers.insert(name.to_string(), t);
        }
        // Keep the last good numbers when a read fails.
        let usage = usage.or_else(|| self.accounts.get(name).and_then(|v| v.usage.clone()));
        self.accounts.insert(
            name.to_string(),
            AccountView {
                state,
                usage,
                usage_error,
            },
        );
        self.read_at.insert(name.to_string(), Instant::now());
    }

    fn read_due(&mut self, force_all: bool) {
        let active = self.active();
        let names: Vec<String> = self.list.accounts.iter().map(|a| a.name.clone()).collect();
        for name in names {
            let period = if Some(&name) == active.as_ref() {
                Duration::from_secs(self.settings.active_poll_seconds.max(15))
            } else {
                Duration::from_secs(self.settings.reread_resting_minutes.max(1) * 60)
            };
            let held = self
                .backoff
                .get(&name)
                .map(|t| Instant::now() < *t)
                .unwrap_or(false);
            let retry = self
                .retry_at
                .get(&name)
                .map(|t| Instant::now() >= *t)
                .unwrap_or(false);
            let due = !held
                && (force_all
                    || retry
                    || self
                        .read_at
                        .get(&name)
                        .map(|t| t.elapsed() >= period)
                        .unwrap_or(true));
            if due {
                self.read_account(&name);
                if Some(&name) == active.as_ref() {
                    self.after_active_read(&name);
                }
            }
        }
    }

    fn candidates(&mut self, fresh_within: Duration) -> Vec<Candidate> {
        let all: Vec<String> = self.list.accounts.iter().map(|a| a.name.clone()).collect();
        let rot = engine::rotation(&self.settings, &all);
        for n in &rot {
            let held = self
                .backoff
                .get(n)
                .map(|t| Instant::now() < *t)
                .unwrap_or(false);
            if !held
                && self
                    .read_at
                    .get(n)
                    .map(|t| t.elapsed() > fresh_within)
                    .unwrap_or(true)
            {
                self.read_account(n);
            }
        }
        rot.iter()
            .map(|n| {
                let v = self.accounts.get(n);
                Candidate {
                    name: n.clone(),
                    available: v.map(|v| v.state.place == Where::Resting).unwrap_or(false),
                    usage: v.and_then(|v| v.usage.clone()).map(|mut u| {
                        u.expire_resets(now_ms());
                        u
                    }),
                }
            })
            .collect()
    }

    fn mark_exhausted(&mut self, name: &str, windows: Vec<String>, until_ms: i64) {
        let e = self.exhausted.entry(name.to_string()).or_insert(Exhausted {
            windows: vec![],
            until_ms,
        });
        for w in windows {
            if !e.windows.contains(&w) {
                e.windows.push(w);
            }
        }
        e.until_ms = e.until_ms.max(until_ms);
    }

    fn usage_of(&self, name: &str) -> Option<&Usage> {
        self.accounts.get(name).and_then(|v| v.usage.as_ref())
    }

    /// The model a conversation last answered with, read from the end of its transcript
    /// (`<claude dir>/projects/*/<conversation>.jsonl`, assistant entries carry `message.model`).
    fn session_model(&mut self, conversation: &str) -> Option<String> {
        if conversation.is_empty()
            || !conversation
                .chars()
                .all(|c| c.is_ascii_hexdigit() || c == '-')
        {
            return None;
        }
        let projects = self.paths.main.dir.join("projects");
        let file = std::fs::read_dir(&projects)
            .ok()?
            .flatten()
            .map(|d| d.path().join(format!("{conversation}.jsonl")))
            .find(|p| p.is_file())?;
        let mtime = std::fs::metadata(&file).and_then(|m| m.modified()).ok()?;
        if let Some((t, m)) = self.models.get(conversation) {
            if *t == mtime {
                return Some(m.clone());
            }
        }
        use std::io::{Read, Seek, SeekFrom};
        let mut f = std::fs::File::open(&file).ok()?;
        let len = f.metadata().ok()?.len();
        let from = len.saturating_sub(512 * 1024);
        f.seek(SeekFrom::Start(from)).ok()?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).ok()?;
        let text = String::from_utf8_lossy(&buf);
        let model = text
            .lines()
            .rev()
            .filter(|l| l.contains("\"assistant\""))
            .find_map(|l| {
                let v: Value = serde_json::from_str(l).ok()?;
                let m = v.pointer("/message/model")?.as_str()?;
                (v.get("type").and_then(Value::as_str) == Some("assistant")
                    && m.starts_with("claude"))
                .then(|| m.to_string())
            })?;
        self.models
            .insert(conversation.to_string(), (mtime, model.clone()));
        Some(model)
    }

    /// Models of the sessions working now (busy, or waiting in the middle of a turn).
    fn working_models(&mut self) -> Vec<String> {
        let convs: Vec<String> = self
            .sessions
            .values()
            .filter(|s| matches!(s.status.as_deref(), Some("busy") | Some("waiting")))
            .filter_map(|s| s.conversation.clone())
            .collect();
        let mut out: Vec<String> = convs.iter().filter_map(|c| self.session_model(c)).collect();
        out.sort();
        out.dedup();
        out
    }

    /// After each read of the active account: global walls, anticipation.
    fn after_active_read(&mut self, active: &str) {
        let Some(mut u) = self.usage_of(active).cloned() else {
            return;
        };
        u.expire_resets(now_ms());
        // A global window at its wall (5 h, week): the account is exhausted, whatever the sessions say.
        let walls: Vec<(String, Option<i64>)> = engine::at_wall(&u)
            .into_iter()
            .filter(|(w, _)| engine::GLOBAL_WINDOWS.contains(&w.as_str()))
            .collect();
        if !walls.is_empty() {
            let until = walls
                .iter()
                .filter_map(|(_, r)| *r)
                .max()
                .unwrap_or(now_ms() + 5 * 3600 * 1000);
            let names: Vec<String> = walls.iter().map(|(w, _)| w.clone()).collect();
            self.mark_exhausted(active, names.clone(), until);
            if self.can_auto() && self.settings.switch_on_limit {
                self.switch_away(
                    active,
                    &names,
                    &|_| 100.0,
                    &format!("wall: {}", names.join(", ")),
                );
            }
            return;
        }
        if !(self.can_auto() && self.settings.anticipate) {
            return;
        }
        if let Some(ls) = &self.last_switch {
            if now_ms() - ls.at_ms < self.settings.min_switch_gap_seconds as i64 * 1000 {
                return;
            }
        }
        let working = self.working_models();
        let over = engine::over_threshold(&self.settings, &u, &working);
        if self.settings.anticipate_model_windows {
            let idle: Vec<String> = u
                .windows
                .iter()
                .filter(|w| {
                    engine::window_model(&w.name).is_some()
                        && !engine::window_in_use(&w.name, &working)
                })
                .filter(|w| {
                    w.percent >= self.settings.threshold_for(engine::threshold_key(&w.name))
                })
                .map(|w| format!("{} {:.0}%", w.name, w.percent))
                .collect();
            if !idle.is_empty()
                && self
                    .anticipation_idle_logged
                    .map(|t| t.elapsed() > Duration::from_secs(1800))
                    .unwrap_or(true)
            {
                self.anticipation_idle_logged = Some(Instant::now());
                self.event(
                    "anticipate.model_idle",
                    json!({"active": active, "over": idle.join(", "), "working_models": working}),
                );
            }
        }
        if over.is_empty() {
            return;
        }
        let names: Vec<String> = over.iter().map(|(w, _)| w.clone()).collect();
        let settings = self.settings.clone();
        let reason = over
            .iter()
            .map(|(w, p)| format!("{w} {p:.0}%"))
            .collect::<Vec<_>>()
            .join(", ");
        let switched = self.switch_away(
            active,
            &names,
            &move |w| settings.threshold_for(engine::threshold_key(w)),
            &format!("anticipate: {reason}"),
        );
        if !switched
            && self
                .anticipation_miss_logged
                .map(|t| t.elapsed() > Duration::from_secs(1800))
                .unwrap_or(true)
        {
            self.anticipation_miss_logged = Some(Instant::now());
            self.event(
                "anticipate.no_room",
                json!({"active": active, "over": reason}),
            );
        }
    }

    fn can_auto(&self) -> bool {
        !self.observe_only && self.settings.auto_switch
    }

    /// Pick an account with room on `needed` and switch to it. Returns whether it switched.
    fn switch_away(
        &mut self,
        active: &str,
        needed: &[String],
        ceiling: &dyn Fn(&str) -> f64,
        reason: &str,
    ) -> bool {
        let cands = self.candidates(Duration::from_secs(120));
        let pick = engine::pick_next(
            &self.settings,
            &cands,
            Some(active),
            &self.exhausted,
            needed,
            ceiling,
            now_ms(),
        );
        match pick {
            Some(to) => self.do_switch(&to, reason, false).is_ok(),
            None => {
                if reason.starts_with("anticipate") {
                    return false;
                }
                let ret = engine::next_return(&self.settings, &self.exhausted, &cands);
                if self.waiting_return != ret {
                    self.waiting_return = ret.clone();
                    let lang = self.lang();
                    let body = match &ret {
                        Some((n, at)) => format!(
                            "{} {}",
                            tr(&lang, "all_out"),
                            tr(&lang, "back_at")
                                .replace("{a}", &self.label(n))
                                .replace("{t}", &hhmm(*at))
                        ),
                        None => tr(&lang, "all_out"),
                    };
                    let offered: Vec<String> = self
                        .accounts
                        .values()
                        .filter_map(|v| {
                            let r = v.usage.as_ref()?.resets.as_ref()?;
                            let n = r
                                .available()
                                .filter(|g| g.usable_now || g.use_requires_limit)
                                .count();
                            (n > 0).then(|| self.label(&v.state.name))
                        })
                        .collect();
                    let body = if offered.is_empty() {
                        body
                    } else {
                        format!(
                            "{body} {}",
                            tr(&lang, "offered").replace("{a}", &offered.join(", "))
                        )
                    };
                    self.event(
                        "all_exhausted",
                        json!({"return": ret, "offered_resets": offered}),
                    );
                    self.notify("Claude Bascule", &body, false);
                }
                false
            }
        }
    }

    fn do_switch(&mut self, to: &str, reason: &str, manual: bool) -> anyhow::Result<Value> {
        let from = self.active();
        match vault::switch(&self.paths, &self.list, to) {
            Ok(report) => {
                let now = now_ms();
                if manual {
                    self.exhausted.remove(to);
                }
                self.last_switch = Some(SwitchInfo {
                    at_ms: now,
                    from: from.clone(),
                    to: to.to_string(),
                    reason: reason.to_string(),
                    manual,
                });
                self.waiting_return = None;
                self.rechecks
                    .push((Instant::now() + Duration::from_secs(3), to.to_string()));
                self.rechecks
                    .push((Instant::now() + Duration::from_secs(10), to.to_string()));
                self.event(
                    "switch",
                    json!({"from": from, "to": to, "reason": reason, "manual": manual}),
                );
                self.web_follow(to);
                let lang = self.lang();
                let body = format!(
                    "{} → {} ({reason})",
                    from.as_deref().map(|f| self.label(f)).unwrap_or_default(),
                    self.label(to)
                );
                self.notify(&tr(&lang, "switched"), &body, false);
                // Re-read both sides soon; collect the sessions this account change concerns.
                self.read_at.remove(to);
                if let Some(f) = &from {
                    self.read_at.remove(f);
                }
                let ready_at = now + self.settings.collect_seconds as i64 * 1000;
                let blocked: Vec<SessionSummary> = self
                    .sessions
                    .values()
                    .filter(|s| s.hit_limit())
                    .cloned()
                    .collect();
                for s in blocked {
                    self.add_entry(&s, ready_at);
                }
                for e in self.resume.values_mut() {
                    if e.state == EntryState::Waiting {
                        e.state = EntryState::Pending;
                        e.ready_at_ms = ready_at;
                    }
                }
                Ok(serde_json::to_value(report)?)
            }
            Err(e) => {
                self.event(
                    "switch.failed",
                    json!({"to": to, "reason": reason, "error": format!("{e:#}")}),
                );
                Err(e)
            }
        }
    }

    // ---- sessions ---------------------------------------------------------------------------

    fn add_entry(&mut self, s: &SessionSummary, ready_at: i64) {
        let now = now_ms();
        let blocked_at = s.last_turn.ended_at.map(|x| x as i64).unwrap_or(now);
        let waiting = self
            .active()
            .map(|a| {
                self.exhausted
                    .get(&a)
                    .map(|e| e.until_ms > now)
                    .unwrap_or(false)
            })
            .unwrap_or(false);
        let entry = self.resume.entry(s.session.clone()).or_insert(Entry {
            session: s.session.clone(),
            title: s.title.clone(),
            cwd: s.cwd.clone(),
            blocked_at_ms: blocked_at,
            limit_kind: s.last_turn.limit_kind.clone(),
            state: EntryState::Pending,
            text: None,
            ready_at_ms: ready_at,
            updated_ms: now,
            note: None,
        });
        if matches!(entry.state, EntryState::Resumed | EntryState::Failed) {
            entry.state = EntryState::Pending;
            entry.note = None;
        }
        entry.blocked_at_ms = blocked_at;
        entry.ready_at_ms = entry.ready_at_ms.max(ready_at);
        if waiting {
            entry.state = EntryState::Waiting;
        }
        entry.updated_ms = now;
    }

    fn on_limit(&mut self, s: &SessionSummary) {
        let now = now_ms();
        let kind = s
            .last_turn
            .limit_kind
            .clone()
            .unwrap_or_else(|| "unknown".into());
        self.event("session.limit", json!({"session": s.session, "title": s.title, "lastTurn": s.last_turn, "screen": s.screen_template, "raw": s.raw}));
        if kind == "credits" {
            return;
        }
        let ended = s.last_turn.ended_at.map(|x| x as i64).unwrap_or(now);
        let collect_ms = self.settings.collect_seconds as i64 * 1000;
        let spontaneous = now + self.settings.spontaneous_restart_seconds as i64 * 1000;
        let straggler = self
            .last_switch
            .as_ref()
            .map(|ls| ended <= ls.at_ms + collect_ms)
            .unwrap_or(false);
        let ready = self
            .last_switch
            .as_ref()
            .map(|ls| ls.at_ms + collect_ms)
            .unwrap_or(0)
            .max(spontaneous);
        self.add_entry(s, ready);
        if straggler {
            return;
        }
        let Some(active) = self.active() else { return };
        self.read_account(&active);
        let u = self.usage_of(&active).cloned();
        let windows = engine::windows_of_limit(&kind, u.as_ref());
        let until = s
            .last_turn
            .resets_at
            .map(|x| (x * 1000.0) as i64)
            .or_else(|| {
                u.as_ref().and_then(|u| {
                    windows
                        .iter()
                        .filter_map(|w| u.window(w))
                        .filter_map(|w| w.resets_at.as_deref())
                        .filter_map(engine::parse_ms)
                        .max()
                })
            })
            .unwrap_or(now + 5 * 3600 * 1000);
        self.mark_exhausted(&active, windows.clone(), until);
        if self.can_auto() && self.settings.switch_on_limit {
            let needed = if windows.is_empty() {
                vec!["seven_day".to_string()]
            } else {
                windows
            };
            self.switch_away(&active, &needed, &|_| 100.0, &format!("limit: {kind}"));
        }
        if self.waiting_return.is_some() {
            if let Some(e) = self.resume.get_mut(&s.session) {
                e.state = EntryState::Waiting;
            }
        }
    }

    fn record_sessions(&mut self, all: Vec<SessionSummary>) {
        let mut alive = std::collections::HashSet::new();
        for s in all.into_iter().filter(SessionSummary::is_claude) {
            alive.insert(s.session.clone());
            let d = s.digest();
            if self.seen.get(&s.session) != Some(&d) {
                log(
                    &self.paths,
                    "session",
                    json!({"session": s.session, "title": s.title, "status": s.status, "lastTurn": s.last_turn, "screen": s.screen_template}),
                );
                self.seen.insert(s.session.clone(), d);
            }
            if s.status.as_deref() == Some("busy") {
                self.last_busy.insert(s.session.clone(), now_ms());
            }
            let ended = s.last_turn.ended_at.map(|x| x as i64).unwrap_or(0);
            let new_limit = s.hit_limit() && self.handled_limits.get(&s.session) != Some(&ended);
            self.sessions.insert(s.session.clone(), s.clone());
            if new_limit {
                self.handled_limits.insert(s.session.clone(), ended);
                self.on_limit(&s);
            }
        }
        self.sessions.retain(|k, _| alive.contains(k));
        self.seen.retain(|k, _| alive.contains(k));
        self.last_busy.retain(|k, _| alive.contains(k));
        self.resume
            .retain(|k, e| alive.contains(k) || matches!(e.state, EntryState::Sending));
        let convs: Vec<String> = self
            .sessions
            .values()
            .filter_map(|s| s.conversation.clone())
            .collect();
        for c in &convs {
            self.session_model(c);
        }
        self.models.retain(|k, _| convs.contains(k));
        self.apply_live_windows();
    }

    /// Every session runs on the active account: the status-line windows a session reports
    /// (through jaunt) update the active account's 5 h and week between usage reads.
    /// The status line only changes with a real request, and jaunt's `fetchedAt` is when the
    /// line was drawn (every second), not when the numbers came: so only sessions that really
    /// worked since the last switch count (a turn ended after it, or busy 15 s after it), and
    /// the freshest of those is taken.
    fn apply_live_windows(&mut self) {
        let since = self.last_switch.as_ref().map(|l| l.at_ms).unwrap_or(0);
        let freshest = self
            .sessions
            .values()
            .filter(|s| {
                let ended = s.last_turn.ended_at.map(|x| x as i64).unwrap_or(0);
                let busy = self.last_busy.get(&s.session).copied().unwrap_or(0);
                ended > since || busy > since + 15_000
            })
            .filter_map(|s| {
                let at = s.limits.get("fetchedAt").and_then(Value::as_f64)? as i64;
                let worked = s
                    .last_turn
                    .ended_at
                    .map(|x| x as i64)
                    .unwrap_or(0)
                    .max(self.last_busy.get(&s.session).copied().unwrap_or(0));
                (at > self.live_applied).then_some((worked, at, s.limits.clone()))
            })
            .max_by_key(|(worked, _, _)| *worked)
            .map(|(_, at, l)| (at, l));
        let Some((at, limits)) = freshest else { return };
        let Some(active) = self.active() else { return };
        let Some(view) = self.accounts.get_mut(&active) else {
            return;
        };
        // No usage read yet (or it failed): start from the status line alone.
        let u = view.usage.get_or_insert_with(Usage::default);
        if at <= u.fetched_at {
            return;
        }
        let mut changed = false;
        for w in limits
            .get("windows")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let name = match w.get("windowMinutes").and_then(Value::as_u64) {
                Some(300) => "five_hour",
                Some(10080) => "seven_day",
                _ => continue,
            };
            let Some(p) = w.get("usedPercent").and_then(Value::as_f64) else {
                continue;
            };
            let resets = w
                .get("resetsAt")
                .and_then(Value::as_f64)
                .and_then(|x| chrono::DateTime::from_timestamp(x as i64, 0))
                .map(|d| d.to_rfc3339());
            match u.windows.iter_mut().find(|x| x.name == name) {
                Some(x) => {
                    changed |= (x.percent - p).abs() >= 0.5;
                    x.percent = p;
                    if resets.is_some() {
                        x.resets_at = resets;
                    }
                }
                None => {
                    u.windows.push(usage::Window {
                        name: name.into(),
                        percent: p,
                        resets_at: resets,
                        active: false,
                        severity: None,
                    });
                    changed = true;
                }
            }
        }
        u.fetched_at = at;
        self.live_applied = at;
        if changed {
            self.after_active_read(&active);
        }
    }

    // ---- resume -----------------------------------------------------------------------------

    fn process_resume(&mut self) {
        let now = now_ms();
        let active_exhausted = self
            .active()
            .and_then(|a| self.exhausted.get(&a).map(|e| e.until_ms > now))
            .unwrap_or(false);
        let keys: Vec<String> = self.resume.keys().cloned().collect();
        for k in keys {
            let s = self.sessions.get(&k).cloned();
            let Some(mut e) = self.resume.get(&k).cloned() else {
                continue;
            };
            if let Some(s) = &s {
                let restarted = s.status.as_deref() == Some("busy")
                    || (!s.hit_limit()
                        && s.last_turn.ended_at.map(|x| x as i64).unwrap_or(0) > e.blocked_at_ms);
                if restarted && e.state != EntryState::Resumed {
                    e.state = EntryState::Resumed;
                    e.updated_ms = now;
                    self.event("resume.resumed", json!({"session": k}));
                }
            }
            match e.state {
                EntryState::Resumed if now - e.updated_ms > 60_000 => {
                    self.resume.remove(&k);
                    continue;
                }
                EntryState::Waiting if !active_exhausted && self.waiting_return.is_none() => {
                    e.state = EntryState::Pending;
                    e.ready_at_ms = now + self.settings.spontaneous_restart_seconds as i64 * 1000;
                }
                EntryState::Pending if now >= e.ready_at_ms && !active_exhausted => {
                    e.state = if self.settings.auto_send && !self.observe_only {
                        EntryState::Sending
                    } else {
                        EntryState::Ready
                    };
                    e.updated_ms = now;
                }
                EntryState::Sending => {
                    self.resume.insert(k.clone(), e.clone());
                    e = self.send_one(&k, e);
                }
                EntryState::Sent
                    if now - e.updated_ms > self.settings.confirm_seconds as i64 * 1000 =>
                {
                    e.state = EntryState::Failed;
                    e.note = Some(tr(&self.lang(), "not_restarted"));
                    self.event("resume.failed", json!({"session": k, "why": "no restart"}));
                }
                _ => {}
            }
            self.resume.insert(k, e);
        }
        let ready = self
            .resume
            .values()
            .filter(|e| e.state == EntryState::Ready)
            .count();
        if ready > self.notified_ready {
            let lang = self.lang();
            let body = if ready == 1 {
                tr(&lang, "resume_one")
            } else {
                tr(&lang, "resume_other").replace("{n}", &ready.to_string())
            };
            self.notify("Claude Bascule", &body, true);
            if !app_running(&self.paths) {
                launch_app(&self.paths);
            }
        }
        self.notified_ready = ready;
    }

    fn send_one(&mut self, k: &str, mut e: Entry) -> Entry {
        let now = now_ms();
        e.updated_ms = now;
        let text = e
            .text
            .clone()
            .unwrap_or_else(|| self.settings.resume_text());
        let Some(j) = self.jaunt.as_mut() else {
            e.state = EntryState::Failed;
            e.note = Some("jaunt not connected".into());
            return e;
        };
        let result = (|| -> anyhow::Result<String> {
            let mut s = j
                .summary(Some(&[k.to_string()]))?
                .into_iter()
                .next()
                .ok_or_else(|| anyhow::anyhow!("session gone"))?;
            if s.status.as_deref() == Some("busy") {
                return Ok("busy".into());
            }
            if s.screen_kind.as_deref() == Some("dialog") {
                let title = s.screen_title.clone().unwrap_or_default();
                let limit_dialog = s.screen_template.as_deref() == Some("rate_limit_options")
                    || title.to_lowercase().contains("limit");
                if !(limit_dialog && s.dialog_with_esc()) {
                    return Ok(format!("dialog:{title}"));
                }
                j.keys(&s, json!(["esc"]), &format!("bascule-esc-{now}"))?;
                std::thread::sleep(Duration::from_millis(1500));
                s = j
                    .summary(Some(&[k.to_string()]))?
                    .into_iter()
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("session gone"))?;
                if s.screen_kind.as_deref() == Some("dialog") {
                    return Ok(format!("dialog:{}", s.screen_title.unwrap_or_default()));
                }
            }
            let r = j.send(&s, &text, &format!("bascule-resume-{now}"))?;
            Ok(r.get("status")
                .and_then(Value::as_str)
                .unwrap_or("sent")
                .to_string())
        })();
        match result {
            Ok(st) if st == "busy" => e.state = EntryState::Resumed,
            Ok(st) if st.starts_with("dialog:") => {
                e.state = EntryState::NeedsYou;
                e.note = Some(st.trim_start_matches("dialog:").to_string());
                self.event("resume.needs_you", json!({"session": k, "dialog": e.note}));
            }
            Ok(st) => {
                e.state = EntryState::Sent;
                e.note = Some(st.clone());
                self.event(
                    "resume.sent",
                    json!({"session": k, "status": st, "text": text}),
                );
            }
            Err(err) => {
                e.state = EntryState::Failed;
                e.note = Some(format!("{err:#}"));
                self.event(
                    "resume.failed",
                    json!({"session": k, "error": format!("{err:#}")}),
                );
            }
        }
        e
    }

    // ---- commands, state ------------------------------------------------------------------

    fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Switch { to, reply } => {
                let r = self
                    .do_switch(&to, "manual", true)
                    .map_err(|e| format!("{e:#}"));
                let _ = reply.send(r);
            }
            Cmd::Reread { reply } => {
                self.read_due(true);
                let _ = reply.send(Ok(json!({"ok": true})));
            }
            Cmd::Send {
                sessions,
                texts,
                reply,
            } => {
                for s in &sessions {
                    if let Some(e) = self.resume.get_mut(s) {
                        if let Some(t) = texts.get(s) {
                            e.text = Some(t.clone());
                        }
                        e.state = EntryState::Sending;
                        e.note = None;
                    }
                }
                self.process_resume();
                let _ = reply.send(Ok(json!({"ok": true})));
            }
            Cmd::Add { sessions, reply } => {
                let now = now_ms();
                let mut added = vec![];
                for id in &sessions {
                    if let Some(s) = self.sessions.get(id).cloned() {
                        self.add_entry(&s, now);
                        if let Some(e) = self.resume.get_mut(id) {
                            e.state = EntryState::Ready;
                            e.blocked_at_ms = now;
                        }
                        added.push(id.clone());
                    }
                }
                let _ = reply.send(Ok(json!({"added": added})));
            }
            Cmd::Dismiss { sessions, reply } => {
                for s in &sessions {
                    self.resume.remove(s);
                }
                let _ = reply.send(Ok(json!({"ok": true})));
            }
            Cmd::AdoptLogin { account, reply } => {
                let r = (|| -> anyhow::Result<Value> {
                    let slot = self.paths.account_slot(&account);
                    let o = slot.oauth()?.ok_or_else(|| anyhow::anyhow!("no tokens"))?;
                    self.paths.main.set_oauth(Some(&o))?;
                    if let Some(a) = slot.account()? {
                        self.paths.main.set_account(Some(&a))?;
                    }
                    slot.set_oauth(None)?;
                    Ok(json!({"adopted": account}))
                })();
                self.event(
                    "login.adopted",
                    json!({"account": account, "ok": r.is_ok()}),
                );
                self.read_at.remove(&account);
                let _ = reply.send(r.map_err(|e| format!("{e:#}")));
            }
        }
    }

    /// After Claude Code switched: claude.ai in the browser follows, when the Connector is there.
    fn web_follow(&mut self, to: &str) {
        let (web, cic) = (self.settings.web_follow, self.settings.cic_follow);
        if !web && !cic {
            return;
        }
        let mut hub = self.web.lock().unwrap();
        if !hub.connected(now_ms()) {
            return;
        }
        let mut missing = false;
        if web && hub.account.as_deref() != Some(to) {
            missing |= hub.push_switch(&self.paths, to).is_err();
        }
        if cic
            && hub.cic_account.as_deref() != Some(to)
            && hub.cic_flow.as_ref().is_none_or(|f| f.stage != "waiting")
        {
            missing |= hub.push_cic(&self.paths, to, web, now_ms()).is_err();
            if !missing {
                drop(hub);
                let lang = self.lang();
                self.notify(
                    &tr(&lang, "cic_title"),
                    &tr(&lang, "cic_body").replace("{a}", &self.label(to)),
                    true,
                );
                launch_app(&self.paths);
                return;
            }
        }
        drop(hub);
        if missing {
            self.event("web.no_session", json!({"account": to}));
        }
    }

    /// A Claude in Chrome move waits for the user at most 10 minutes; its outcome shows 30 s.
    fn cic_tick(&mut self) {
        let mut hub = self.web.lock().unwrap();
        let now = now_ms();
        match hub
            .cic_flow
            .as_ref()
            .map(|f| (f.stage.clone(), f.started_ms))
        {
            Some((stage, started)) if stage == "waiting" && now - started > 600_000 => {
                hub.queue.push_back(json!({"type": "cic_stop"}));
                hub.end_cic(&self.paths, false, Some("timeout".into()));
            }
            Some((stage, started)) if stage != "waiting" && now - started > 630_000 => {
                hub.cic_flow = None;
            }
            _ => {}
        }
    }

    /// Releases: checked at start, then every hour of wall-clock time (a laptop that slept all
    /// night checks as soon as it wakes), in the background.
    fn update_tick(&mut self) {
        if !self.settings.check_updates || self.update_busy.load(Ordering::SeqCst) {
            return;
        }
        let now = now_ms();
        let last = self.updates.lock().unwrap().checked_at.unwrap_or(0);
        if now - last.max(self.update_tried_ms) < 3_600_000 {
            return;
        }
        self.update_tried_ms = now;
        self.update_busy.store(true, Ordering::SeqCst);
        let (updates, busy) = (self.updates.clone(), self.update_busy.clone());
        std::thread::spawn(move || {
            let st = crate::update::check(env!("CARGO_PKG_VERSION"));
            *updates.lock().unwrap() = st;
            busy.store(false, Ordering::SeqCst);
        });
    }

    fn web_view(&self) -> Value {
        let hub = self.web.lock().unwrap();
        let sessions: Vec<&str> = self
            .list
            .accounts
            .iter()
            .filter(|a| crate::web::has(&self.paths, &a.name))
            .map(|a| a.name.as_str())
            .collect();
        json!({"connected": hub.connected(now_ms()), "browser": hub.browser, "version": hub.version,
            "cic_account": hub.cic_account, "cic_flow": hub.cic_flow,
            "account": hub.account, "unknown_email": hub.unknown_email, "last_error": hub.last_error,
            "pending": !hub.queue.is_empty(), "sessions": sessions,
            "extension_dir": crate::platform::extension_dir().map(|p| p.display().to_string())})
    }

    fn publish(&self) {
        let active = self.active();
        let accounts: Vec<Value> = self
            .list
            .accounts
            .iter()
            .map(|a| {
                let v = self.accounts.get(&a.name);
                // In demo mode the email never leaves the daemon unmasked.
                let shown_email = if self.settings.hide_emails { crate::config::mask_email(&a.email) } else { a.email.clone() };
                let has_alias = self.settings.display_names.get(&a.name).map(|s| !s.trim().is_empty()).unwrap_or(false);
                json!({
                    "name": a.name, "email": shown_email, "label": self.settings.label_for(&a.name, &a.email), "has_alias": has_alias,
                    "place": v.map(|v| json!(v.state.place)).unwrap_or(json!("unknown")),
                    "token": v.and_then(|v| v.state.token.as_ref()).map(|t| json!({"expires_at": t.expires_at, "refresh_expires_at": t.refresh_expires_at})),
                    // Plan tier (e.g. default_claude_max_20x): the mini-app greys the 5 h row on Max 20x.
                    "tier": v.and_then(|v| v.state.token.as_ref()).and_then(|t| t.tier.clone()).or_else(|| self.tiers.get(&a.name).cloned()),
                    "subscription": v.and_then(|v| v.state.token.as_ref()).and_then(|t| t.subscription.clone()),
                    "usage": v.and_then(|v| v.usage.clone()),
                    "usage_error": v.and_then(|v| v.usage_error.clone()),
                    "exhausted": self.exhausted.get(&a.name),
                    "in_rotation": engine::rotation(&self.settings, &self.list.accounts.iter().map(|x| x.name.clone()).collect::<Vec<_>>()).contains(&a.name),
                })
            })
            .collect();
        let models: HashMap<String, String> = self
            .models
            .iter()
            .map(|(k, (_, m))| (k.clone(), m.clone()))
            .collect();
        let sessions: Vec<Value> = self
            .sessions
            .values()
            .map(|s| {
                let model = s.conversation.as_ref().and_then(|c| models.get(c)).cloned();
                json!({"session": s.session, "title": s.title, "cwd": s.cwd, "status": s.status, "lastTurn": s.last_turn, "dialog": s.screen_title, "model": model})
            })
            .collect();
        let v = json!({
            "at": now_ms(), "version": env!("CARGO_PKG_VERSION"), "observe_only": self.observe_only,
            "language": self.lang(), "system_language": crate::config::system_language(), "default_message": self.settings.resume_text(),
            "settings": self.settings, "active": active, "last_switch": self.last_switch,
            "waiting_return": self.waiting_return, "accounts": accounts, "sessions": sessions,
            "resume": self.resume.values().collect::<Vec<_>>(), "events": self.events, "jaunt": self.jaunt_state,
            "update": *self.updates.lock().unwrap(), "platform": std::env::consts::OS, "web": self.web_view(),
        });
        if let Ok(mut g) = self.shared.lock() {
            *g = v.clone();
        }
        let _ = crate::store::atomic_write(&self.paths.snapshot_file(), &v);
    }

    fn reload_settings(&mut self) {
        let m = std::fs::metadata(self.paths.settings_file())
            .and_then(|m| m.modified())
            .ok();
        if m != self.settings_mtime {
            self.settings_mtime = m;
            match Settings::load(&self.paths.settings_file()) {
                Ok(s) => {
                    self.settings = s;
                    self.event("settings.reloaded", json!(self.settings));
                }
                Err(e) => self.event(
                    "error",
                    json!({"where": "settings", "error": format!("{e:#}")}),
                ),
            }
        }
        if let Ok(l) = AccountList::load(&self.paths.accounts_file()) {
            self.list = l;
        }
    }

    fn jaunt_tick(&mut self) {
        if self.jaunt.is_none() {
            if Instant::now() < self.jaunt_retry {
                return;
            }
            // Without jaunt, Claude Bascule still switches; it only cannot follow and resume sessions.
            if !crate::platform::jaunt_installed(&self.paths.home) {
                self.jaunt_state = "absent".into();
                self.jaunt_retry = Instant::now() + Duration::from_secs(30);
                return;
            }
            if !self.paths.jaunt_key.exists() {
                self.jaunt_state = "not_enrolled".into();
                self.jaunt_retry = Instant::now() + Duration::from_secs(5);
                return;
            }
            match Jaunt::connect(&self.paths).and_then(|mut j| {
                j.subscribe(&["sessions", "rich.state"])?;
                Ok(j)
            }) {
                Ok(j) => {
                    self.jaunt = Some(j);
                    self.jaunt_state = "connected".into();
                    self.jaunt_backoff = Duration::from_secs(2);
                    self.dirty = true;
                }
                Err(e) => {
                    let msg = format!("{e:#}");
                    if self.jaunt_state != msg {
                        self.event("jaunt.down", json!({"error": msg}));
                    }
                    self.jaunt_state = msg;
                    self.jaunt_retry = Instant::now() + self.jaunt_backoff;
                    self.jaunt_backoff = (self.jaunt_backoff * 2).min(Duration::from_secs(60));
                    return;
                }
            }
        }
        let due = (self.dirty && self.last_summary.elapsed() >= Duration::from_secs(2))
            || Instant::now() >= self.next_summary;
        if due {
            let r = self.jaunt.as_mut().unwrap().summary(None);
            match r {
                Ok(all) => {
                    self.record_sessions(all);
                    self.last_summary = Instant::now();
                    self.next_summary = self.last_summary
                        + Duration::from_secs(self.settings.observe_poll_seconds.max(5));
                    self.dirty = false;
                }
                Err(e) => self.drop_jaunt(e),
            }
        }
    }

    fn drop_jaunt(&mut self, e: anyhow::Error) {
        self.jaunt = None;
        self.jaunt_state = format!("{e:#}");
        self.event("jaunt.lost", json!({"error": self.jaunt_state}));
        self.jaunt_retry = Instant::now() + Duration::from_secs(2);
    }

    fn wait(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(1);
        if let Some(j) = self.jaunt.as_mut() {
            match j.next_event(Duration::from_millis(500)) {
                Ok(Some(ev)) => {
                    if ev.get("type").and_then(Value::as_str) == Some("service.closed") {
                        self.drop_jaunt(anyhow::anyhow!(
                            "jaunt closed the service: {}",
                            ev.get("reason").unwrap_or(&Value::Null)
                        ));
                    } else {
                        self.dirty = true;
                    }
                }
                Ok(None) => {}
                Err(e) => self.drop_jaunt(e),
            }
        }
        if let Ok(cmd) = self
            .rx
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        {
            self.handle(cmd);
        }
    }

    pub fn run(&mut self, stop: &dyn Fn() -> bool) -> anyhow::Result<()> {
        crate::store::private_dir(&self.paths.state)?;
        if let Ok(fixed) = vault::heal(&self.paths, &self.list) {
            if !fixed.is_empty() {
                self.event("heal", json!({"removed_copies": fixed}));
            }
        }
        self.event("daemon.start", json!({"version": env!("CARGO_PKG_VERSION"), "observe_only": self.observe_only, "settings": self.settings}));
        // First run: the account Claude Code is signed in with becomes the first account.
        if self.list.accounts.is_empty() {
            if let Some(email) = self.paths.main.email() {
                if self.list.add(&email).is_ok()
                    && self.list.save(&self.paths.accounts_file()).is_ok()
                {
                    self.event("accounts.imported", json!({"email": email}));
                }
            }
        }
        self.web.lock().unwrap().cic_account = crate::web::load_cic(&self.paths);
        // The app can show the accounts at once, before the first (possibly slow) quota reads.
        self.publish();
        while !stop() {
            self.reload_settings();
            self.update_tick();
            self.cic_tick();
            let notes: Vec<(String, Value)> = self.web.lock().unwrap().notes.drain(..).collect();
            for (kind, data) in notes {
                self.event(&kind, data);
            }
            while let Ok(cmd) = self.rx.try_recv() {
                self.handle(cmd);
            }
            self.read_due(false);
            self.jaunt_tick();
            let now = now_ms();
            self.exhausted.retain(|_, e| e.until_ms + 120_000 > now);
            if let Some((name, at)) = self.waiting_return.clone() {
                if now >= at {
                    self.waiting_return = None;
                    self.read_account(&name);
                    if Some(&name) != self.active().as_ref() && self.can_auto() {
                        let _ = self.do_switch(&name, "return after reset", false);
                    }
                }
            }
            self.process_resume();
            let due: Vec<String> = self
                .rechecks
                .iter()
                .filter(|(t, _)| Instant::now() >= *t)
                .map(|(_, n)| n.clone())
                .collect();
            self.rechecks.retain(|(t, _)| Instant::now() < *t);
            for n in due {
                if let Ok(true) = vault::recheck_account(&self.paths, &self.list, &n) {
                    self.event("switch.account_rewritten", json!({"to": n}));
                }
            }
            self.publish();
            self.wait();
        }
        self.event("daemon.stop", json!({}));
        Ok(())
    }
}

/// The mini-app writes its pid here while it runs.
pub fn app_running(paths: &Paths) -> bool {
    std::fs::read_to_string(paths.state.join("app.pid"))
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .map(crate::platform::pid_alive)
        .unwrap_or(false)
}

/// Start the mini-app on its own, unless it already runs.
pub fn launch_app(paths: &Paths) {
    if !app_running(paths) {
        crate::platform::launch_app();
    }
}

/// The service: control endpoint plus the daemon loop (used by `bascule daemon` and `bascule-daemon`).
pub fn run_service(
    paths: Paths,
    list: AccountList,
    settings: Settings,
    observe_only: bool,
) -> anyhow::Result<()> {
    crate::store::private_dir(&paths.state)?;
    if crate::control::is_running(&paths) {
        anyhow::bail!("the Claude Bascule service already runs");
    }
    let shared = Arc::new(Mutex::new(json!({})));
    let (tx, rx) = std::sync::mpsc::channel();
    let daemon = Daemon::new(
        paths.clone(),
        list,
        settings,
        observe_only,
        shared.clone(),
        rx,
    );
    crate::control::serve(
        paths,
        shared,
        tx,
        daemon.updates.clone(),
        daemon.web.clone(),
    )?;
    let mut daemon = daemon;
    daemon.run(&|| false)
}
