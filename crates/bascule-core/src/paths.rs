//! Where Claude Bascule keeps things on a machine.
//!
//! | | Linux | macOS | Windows |
//! | --- | --- | --- | --- |
//! | state (vault, logs) | `~/.local/state/bascule` | `~/Library/Application Support/ClaudeBascule` | `%LOCALAPPDATA%\ClaudeBascule` |
//! | settings, accounts | `~/.config/bascule` | same as state | `%APPDATA%\ClaudeBascule` |

use crate::store::Slot;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Paths {
    pub home: PathBuf,
    /// Runtime state, private: vault, state.json, logs. Never in git.
    pub state: PathBuf,
    /// One `CLAUDE_CONFIG_DIR` per account.
    pub vault: PathBuf,
    /// Settings and the account list.
    pub config_dir: PathBuf,
    /// The live Claude Code identity Claude Bascule switches.
    pub main: Slot,
    /// jaunt's local control endpoint: Unix socket, or `control.json` (Windows).
    pub jaunt_socket: PathBuf,
    pub jaunt_json: PathBuf,
    /// Claude Bascule's jaunt service key (`jaunt agents enroll bascule ... --out`).
    pub jaunt_key: PathBuf,
    /// Vault entries use files only (tests), never the macOS keychain.
    pub files_only: bool,
}

pub fn home_dir() -> PathBuf {
    let key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(key)
        .or_else(|| std::env::var_os("HOME"))
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

impl Paths {
    /// Defaults, with test overrides: `BASCULE_STATE`, `BASCULE_VAULT`, `BASCULE_CONFIG`, `BASCULE_CLAUDE_DIR`
    /// (a `CLAUDE_CONFIG_DIR`-style directory used instead of `~/.claude`, files only).
    pub fn from_env() -> Self {
        let home = home_dir();
        let env = |k: &str| {
            std::env::var_os(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let (state_default, config_default) = if cfg!(windows) {
            let local = env("LOCALAPPDATA").unwrap_or_else(|| home.join("AppData").join("Local"));
            let roaming = env("APPDATA").unwrap_or_else(|| home.join("AppData").join("Roaming"));
            (local.join("ClaudeBascule"), roaming.join("ClaudeBascule"))
        } else if cfg!(target_os = "macos") {
            let d = home
                .join("Library")
                .join("Application Support")
                .join("ClaudeBascule");
            (d.clone(), d)
        } else {
            (
                env("XDG_STATE_HOME")
                    .unwrap_or_else(|| home.join(".local").join("state"))
                    .join("bascule"),
                env("XDG_CONFIG_HOME")
                    .unwrap_or_else(|| home.join(".config"))
                    .join("bascule"),
            )
        };
        let state = env("BASCULE_STATE").unwrap_or(state_default);
        let config_dir = env("BASCULE_CONFIG").unwrap_or(config_default);
        let main = match env("BASCULE_CLAUDE_DIR") {
            Some(dir) => Slot::config_dir(dir).files_only(),
            None => Slot::main(&home),
        };
        let (jaunt_socket, jaunt_json) = crate::platform::jaunt_paths(&home);
        Paths {
            vault: env("BASCULE_VAULT").unwrap_or_else(|| state.join("accounts")),
            jaunt_key: state.join("jaunt-service.key"),
            jaunt_socket: env("BASCULE_JAUNT_SOCKET").unwrap_or(jaunt_socket),
            jaunt_json,
            home,
            state,
            config_dir,
            main,
            files_only: false,
        }
    }

    pub fn account_slot(&self, name: &str) -> Slot {
        let slot = Slot::config_dir(self.vault.join(name));
        if self.files_only {
            slot.files_only()
        } else {
            slot
        }
    }

    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    pub fn accounts_file(&self) -> PathBuf {
        self.config_dir.join("accounts.toml")
    }

    pub fn state_file(&self) -> PathBuf {
        self.state.join("state.json")
    }

    pub fn log_file(&self) -> PathBuf {
        self.state.join("events.jsonl")
    }

    pub fn snapshot_file(&self) -> PathBuf {
        self.state.join("snapshot.json")
    }
}
