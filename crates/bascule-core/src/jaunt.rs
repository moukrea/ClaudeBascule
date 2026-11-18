//! jaunt's service API over its local control socket (docs/internals.md).
//! One JSON object per line: `service.connect`, then `{type: "rpc", id, method, params}`;
//! replies `{type: "reply", id, ok, result|error}`, events `{type: "agent.event", topic, event}`.

use crate::platform::{self, Stream};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::time::{Duration, Instant};

pub const SERVICE: &str = "bascule";

pub struct Jaunt {
    writer: Stream,
    reader: BufReader<Stream>,
    /// Windows: the key every line carries.
    key: Option<String>,
    partial: String,
    next_id: u64,
    events: VecDeque<Value>,
    pub welcome: Value,
}

impl Jaunt {
    pub fn connect(paths: &crate::paths::Paths) -> Result<Self> {
        let key_file = &paths.jaunt_key;
        let token = std::fs::read_to_string(key_file)
            .with_context(|| format!("read {} (not enrolled in jaunt yet)", key_file.display()))?;
        let ep = platform::connect(&paths.jaunt_socket, &paths.jaunt_json)?;
        let stream = ep.stream;
        let mut j = Jaunt {
            writer: stream.try_clone()?,
            reader: BufReader::new(stream),
            key: ep.key,
            partial: String::new(),
            next_id: 0,
            events: VecDeque::new(),
            welcome: Value::Null,
        };
        j.write(&json!({"method": "service.connect", "params": {"service": SERVICE, "token": token.trim()}}))?;
        let w = j
            .read(Duration::from_secs(10))?
            .context("jaunt closed the connection")?;
        if w.get("type").and_then(Value::as_str) != Some("welcome") {
            bail!(
                "jaunt refused the service: {}",
                w.get("error").unwrap_or(&w)
            );
        }
        j.welcome = w;
        Ok(j)
    }

    fn write(&mut self, v: &Value) -> Result<()> {
        let mut line = serde_json::to_vec(&platform::keyed(v.clone(), &self.key))?;
        line.push(b'\n');
        self.writer.write_all(&line)?;
        Ok(())
    }

    /// One line, or None on timeout. Keeps a partial line across timeouts.
    fn read(&mut self, timeout: Duration) -> Result<Option<Value>> {
        self.reader
            .get_ref()
            .set_read_timeout(Some(timeout.max(Duration::from_millis(10))))?;
        match self.reader.read_line(&mut self.partial) {
            Ok(0) => bail!("jaunt closed the connection"),
            Ok(_) => {
                let line = std::mem::take(&mut self.partial);
                Ok(Some(
                    serde_json::from_str(line.trim_end()).context("jaunt sent invalid JSON")?,
                ))
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn call(&mut self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        self.next_id += 1;
        let id = format!("b{}", self.next_id);
        self.write(&json!({"type": "rpc", "id": id, "method": method, "params": params}))?;
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                bail!("{method}: no reply in {}s", timeout.as_secs());
            }
            let Some(m) = self.read(left)? else { continue };
            if m.get("type").and_then(Value::as_str) == Some("reply")
                && m.get("id").and_then(Value::as_str) == Some(&id)
            {
                if m.get("ok").and_then(Value::as_bool) == Some(true) {
                    return Ok(m.get("result").cloned().unwrap_or(Value::Null));
                }
                bail!(
                    "{method}: {}",
                    m.get("error").and_then(Value::as_str).unwrap_or("refused")
                );
            }
            self.events.push_back(m);
        }
    }

    pub fn next_event(&mut self, timeout: Duration) -> Result<Option<Value>> {
        if let Some(e) = self.events.pop_front() {
            return Ok(Some(e));
        }
        self.read(timeout)
    }

    pub fn subscribe(&mut self, topics: &[&str]) -> Result<Value> {
        self.call(
            "agents.subscribe",
            json!({"topics": topics}),
            Duration::from_secs(150),
        )
    }

    pub fn summary(&mut self, sessions: Option<&[String]>) -> Result<Vec<SessionSummary>> {
        let params = match sessions {
            Some(s) => json!({"sessions": s}),
            None => json!({}),
        };
        let r = self.call("agents.rich.summary", params, Duration::from_secs(150))?;
        Ok(r.get("sessions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(SessionSummary::from_value)
            .collect())
    }

    pub fn keys(&mut self, s: &SessionSummary, keys: Value, op: &str) -> Result<Value> {
        self.call(
            "agents.rich.keys",
            json!({"session": s.session, "instance": s.instance, "keys": keys, "screen": s.screen_id, "op": op}),
            Duration::from_secs(150),
        )
    }

    /// Submit a one-line message (`{status: sent|queued|steered|held}`).
    pub fn send(&mut self, s: &SessionSummary, text: &str, op: &str) -> Result<Value> {
        self.call(
            "agents.rich.send",
            json!({"session": s.session, "instance": s.instance, "conversation": s.conversation, "text": text, "op": op}),
            Duration::from_secs(150),
        )
    }
}

#[derive(Clone, Debug, Serialize, Default, PartialEq)]
pub struct LastTurn {
    pub reason: Option<String>,
    pub limit_kind: Option<String>,
    pub resets_at: Option<f64>,
    pub auto_resume_at: Option<f64>,
    pub ended_at: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct SessionSummary {
    pub session: String,
    pub runtime: Option<String>,
    pub conversation: Option<String>,
    pub instance: Option<String>,
    pub title: String,
    pub cwd: Option<String>,
    pub status: Option<String>,
    pub screen_kind: Option<String>,
    pub screen_id: Option<String>,
    pub screen_template: Option<String>,
    pub screen_title: Option<String>,
    pub screen_hints: Vec<String>,
    pub last_turn: LastTurn,
    /// `limits.windows` as jaunt reports them (status line: weekly only, docs/internals.md).
    pub limits: Value,
    pub raw: Value,
}

impl SessionSummary {
    pub fn from_value(v: &Value) -> Self {
        let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
        let screen = v.get("screen").cloned().unwrap_or(Value::Null);
        let lt = v.get("lastTurn").cloned().unwrap_or(Value::Null);
        SessionSummary {
            session: s("session").unwrap_or_default(),
            runtime: s("runtime"),
            conversation: s("conversation"),
            instance: s("instance"),
            title: s("title").unwrap_or_default(),
            cwd: s("cwd"),
            status: s("status"),
            screen_kind: s("screenKind"),
            screen_id: screen.get("id").and_then(Value::as_str).map(str::to_string),
            screen_template: screen
                .get("template")
                .and_then(Value::as_str)
                .map(str::to_string),
            screen_title: screen
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_string),
            screen_hints: screen
                .get("hints")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
            last_turn: LastTurn {
                reason: lt.get("reason").and_then(Value::as_str).map(str::to_string),
                limit_kind: lt
                    .get("limitKind")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                resets_at: lt.get("resetsAt").and_then(Value::as_f64),
                auto_resume_at: lt.get("autoResumeAt").and_then(Value::as_f64),
                ended_at: lt.get("endedAt").and_then(Value::as_f64),
            },
            limits: v.get("limits").cloned().unwrap_or(Value::Null),
            raw: v.clone(),
        }
    }

    pub fn is_claude(&self) -> bool {
        self.runtime.as_deref() == Some("claude")
    }

    /// The last turn ended on a usage limit.
    pub fn hit_limit(&self) -> bool {
        self.last_turn.reason.as_deref() == Some("limit")
    }

    /// A dialog covers the prompt and says Esc dismisses it.
    pub fn dialog_with_esc(&self) -> bool {
        self.screen_kind.as_deref() == Some("dialog")
            && self.screen_hints.iter().any(|h| h.contains("Esc"))
    }

    /// What the observer compares to decide that something changed.
    pub fn digest(&self) -> String {
        format!(
            "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
            self.status,
            self.last_turn.reason,
            self.last_turn.limit_kind,
            self.last_turn.resets_at,
            self.screen_template,
            self.screen_title
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_from_observed_shape() {
        let v = json!({"session": "RevV", "runtime": "claude", "conversation": "8239", "instance": "claude:fg:1:2",
            "status": "idle", "screenKind": "dialog",
            "screen": {"id": "59", "kind": "dialog", "title": "Teach auto mode about your environment?", "hints": ["Enter to confirm", "Esc to cancel"]},
            "lastTurn": {"endedAt": 1794994423150.0, "reason": "limit", "limitKind": "weekly", "resetsAt": 1795503600.0, "autoResumeAt": null},
            "limits": {"windows": [{"name": "Weekly", "usedPercent": 80}]}});
        let s = SessionSummary::from_value(&v);
        assert!(s.is_claude() && s.hit_limit() && s.dialog_with_esc());
        assert_eq!(s.last_turn.limit_kind.as_deref(), Some("weekly"));
        assert_eq!(s.screen_id.as_deref(), Some("59"));
    }
}
