//! Signing an account in through the tool : `claude auth login`
//! in the account's own `CLAUDE_CONFIG_DIR` (docs/internals.md). Claude Code opens the browser
//! itself; when the page shows a code instead, it is passed on stdin.

use crate::config::Account;
use crate::paths::Paths;
use crate::store::private_dir;
use anyhow::{bail, Context, Result};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::mpsc::{channel, Receiver};

pub struct Login {
    pub child: Child,
    stdin: Option<ChildStdin>,
    /// Lines Claude Code prints (the sign-in URL among them).
    pub lines: Receiver<String>,
}

impl Login {
    pub fn start(paths: &Paths, account: &Account) -> Result<Self> {
        let slot = paths.account_slot(&account.name);
        private_dir(&paths.vault)?;
        private_dir(&slot.dir)?;
        let mut child = crate::platform::claude()
            .args(["auth", "login", "--email", &account.email])
            .env("CLAUDE_CONFIG_DIR", &slot.dir)
            .env("DISABLE_AUTOUPDATER", "1")
            .current_dir(&slot.dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("run claude auth login")?;
        let (tx, rx) = channel();
        for pipe in [
            child
                .stdout
                .take()
                .map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
            child.stderr.take().map(|p| Box::new(p) as _),
        ]
        .into_iter()
        .flatten()
        {
            let tx = tx.clone();
            std::thread::spawn(move || {
                // The prompt "Paste code here if prompted >" has no newline: split on '>' too.
                let mut r = BufReader::new(pipe);
                let mut buf = vec![];
                loop {
                    buf.clear();
                    match r.read_until(b'\n', &mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {
                            let _ = tx.send(String::from_utf8_lossy(&buf).trim_end().to_string());
                        }
                    }
                }
            });
        }
        Ok(Login {
            stdin: child.stdin.take(),
            child,
            lines: rx,
        })
    }

    pub fn submit_code(&mut self, code: &str) -> Result<()> {
        let stdin = self.stdin.as_mut().context("login already finished")?;
        stdin.write_all(code.trim().as_bytes())?;
        stdin.write_all(b"\n")?;
        stdin.flush()?;
        Ok(())
    }

    /// The URL in a line Claude Code printed, if any.
    pub fn url_in(line: &str) -> Option<String> {
        line.split_whitespace()
            .find(|w| w.starts_with("https://"))
            .map(str::to_string)
    }
}

/// After `claude auth login` exits: the vault entry must hold tokens for the expected email.
/// A sign-in with another account is undone (its tokens dropped) and reported.
pub fn verify(paths: &Paths, account: &Account) -> Result<()> {
    let slot = paths.account_slot(&account.name);
    if slot.oauth()?.is_none() {
        bail!("no tokens after sign-in: the login did not finish");
    }
    match slot.email() {
        Some(e) if e.eq_ignore_ascii_case(&account.email) => Ok(()),
        other => {
            slot.set_oauth(None)?;
            bail!("signed in as {} instead of {}: tokens dropped, sign in again with the right account", other.unwrap_or_else(|| "an unknown account".into()), account.email)
        }
    }
}
