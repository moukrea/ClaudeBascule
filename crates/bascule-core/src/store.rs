//! Claude Code credential files: read and write `claudeAiOauth` and `oauthAccount` only.
//!
//! Rules (docs/internals.md): every other key is preserved (`mcpOAuth`...), writes are atomic
//! (temp file in the same directory, fsync, rename) with mode 0600, never through a symlink.
//! Tokens are only ever shown as a short fingerprint.

use anyhow::{bail, Context, Result};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Where one Claude Code identity lives: its credentials and its global config file.
/// For `~/.claude` the config is `~/.claude.json`; for a `CLAUDE_CONFIG_DIR` it is inside the dir.
/// Credentials are a file (`.credentials.json`) on Linux and Windows; on macOS Claude Code keeps
/// them in the login keychain (service, account), with the file as a fallback.
#[derive(Clone, Debug)]
pub struct Slot {
    pub dir: PathBuf,
    pub creds: PathBuf,
    pub conf: PathBuf,
    pub keychain: Option<(String, String)>,
}

impl Slot {
    /// A `CLAUDE_CONFIG_DIR`-style directory (vault entries, test dirs).
    pub fn config_dir(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        let keychain = cfg!(target_os = "macos").then(|| {
            (
                crate::platform::keychain_service(Some(&dir)),
                crate::platform::keychain_account(),
            )
        });
        Slot {
            creds: dir.join(".credentials.json"),
            conf: dir.join(".claude.json"),
            dir,
            keychain,
        }
    }

    /// A slot that only uses files (tests, and test dirs on any platform).
    pub fn files_only(mut self) -> Self {
        self.keychain = None;
        self
    }

    /// The default Claude Code home: `~/.claude` with `~/.claude.json` next to it.
    pub fn main(home: &Path) -> Self {
        Slot {
            dir: home.join(".claude"),
            creds: home.join(".claude").join(".credentials.json"),
            conf: home.join(".claude.json"),
            keychain: cfg!(target_os = "macos").then(|| {
                (
                    crate::platform::keychain_service(None),
                    crate::platform::keychain_account(),
                )
            }),
        }
    }

    /// The whole credentials document (keychain item or file).
    fn read_creds(&self) -> Result<Option<Map<String, Value>>> {
        if let Some((service, account)) = &self.keychain {
            if let Some(text) = crate::platform::keychain_read(service, account)? {
                return match serde_json::from_str::<Value>(&text)? {
                    Value::Object(m) => Ok(Some(m)),
                    _ => bail!("the keychain item is not a JSON object"),
                };
            }
        }
        read_object(&self.creds)
    }

    fn write_creds(&self, map: Map<String, Value>) -> Result<()> {
        if let Some((service, account)) = &self.keychain {
            // Where Claude Code keeps it: the keychain when the item exists or no file does.
            if crate::platform::keychain_read(service, account)?.is_some() || !self.creds.exists() {
                return crate::platform::keychain_write(
                    service,
                    account,
                    &serde_json::to_string(&Value::Object(map))?,
                );
            }
        }
        atomic_write(&self.creds, &Value::Object(map))
    }

    pub fn oauth(&self) -> Result<Option<Value>> {
        Ok(self
            .read_creds()?
            .and_then(|m| m.get("claudeAiOauth").cloned())
            .filter(|v| !v.is_null()))
    }

    pub fn set_oauth(&self, oauth: Option<&Value>) -> Result<()> {
        let mut map = self.read_creds()?.unwrap_or_default();
        match oauth {
            Some(v) => {
                map.insert("claudeAiOauth".into(), v.clone());
            }
            None => {
                if map.remove("claudeAiOauth").is_none() {
                    return Ok(());
                }
            }
        }
        self.write_creds(map)
    }

    pub fn account(&self) -> Result<Option<Value>> {
        Ok(read_object(&self.conf)?
            .and_then(|m| m.get("oauthAccount").cloned())
            .filter(|v| !v.is_null()))
    }

    pub fn set_account(&self, account: Option<&Value>) -> Result<()> {
        update_key(&self.conf, "oauthAccount", account)
    }

    pub fn email(&self) -> Option<String> {
        self.account().ok().flatten().and_then(|a| {
            a.get("emailAddress")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
    }
}

/// A token's fingerprint: the first 10 hex digits of its SHA-256.
pub fn fp(token: &str) -> String {
    if token.is_empty() {
        return "-".into();
    }
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().take(5).map(|b| format!("{b:02x}")).collect()
}

/// What can be said about an OAuth block without showing a token.
#[derive(Clone, Debug, serde::Serialize, PartialEq)]
pub struct TokenInfo {
    pub access: String,
    pub refresh: String,
    pub expires_at: Option<i64>,
    pub refresh_expires_at: Option<i64>,
    pub subscription: Option<String>,
    pub tier: Option<String>,
}

impl TokenInfo {
    pub fn of(oauth: &Value) -> Self {
        let s = |k: &str| {
            oauth
                .get(k)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };
        let o = |k: &str| oauth.get(k).and_then(Value::as_str).map(str::to_string);
        TokenInfo {
            access: fp(&s("accessToken")),
            refresh: fp(&s("refreshToken")),
            expires_at: oauth.get("expiresAt").and_then(Value::as_i64),
            refresh_expires_at: oauth.get("refreshTokenExpiresAt").and_then(Value::as_i64),
            subscription: o("subscriptionType"),
            tier: o("rateLimitTier"),
        }
    }
}

pub fn access_token(oauth: &Value) -> Option<&str> {
    oauth
        .get("accessToken")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
}

fn read_object(path: &Path) -> Result<Option<Map<String, Value>>> {
    match fs::read(path) {
        Ok(bytes) if bytes.iter().all(u8::is_ascii_whitespace) => Ok(Some(Map::new())),
        Ok(bytes) => match serde_json::from_slice::<Value>(&bytes)
            .with_context(|| format!("parse {}", path.display()))?
        {
            Value::Object(m) => Ok(Some(m)),
            _ => bail!("{} is not a JSON object", path.display()),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("read {}", path.display())),
    }
}

fn update_key(path: &Path, key: &str, value: Option<&Value>) -> Result<()> {
    let mut map = read_object(path)?.unwrap_or_default();
    match value {
        Some(v) => {
            map.insert(key.to_string(), v.clone());
        }
        None => {
            if map.remove(key).is_none() && !path.exists() {
                return Ok(());
            }
        }
    }
    atomic_write(path, &Value::Object(map))
}

/// Write JSON atomically with mode 0600; refuses a symlink target.
pub fn atomic_write(path: &Path, value: &Value) -> Result<()> {
    atomic_write_mode(path, value, 0o600)
}

fn atomic_write_mode(path: &Path, value: &Value, mode: u32) -> Result<()> {
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    atomic_write_text(path, &text, mode)
}

/// Write text atomically (temporary file, then rename) with the given Unix mode, never through a
/// symlink.
pub fn atomic_write_text(path: &Path, text: &str, mode: u32) -> Result<()> {
    if fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        bail!("refusing to write through a symlink: {}", path.display());
    }
    let dir = path.parent().context("no parent directory")?;
    fs::create_dir_all(dir)?;
    let name = path.file_name().context("no file name")?.to_string_lossy();
    let tmp = dir.join(format!(".{name}.bascule.{}.tmp", std::process::id()));
    let result = (|| -> Result<()> {
        let mut opts = OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(mode);
        }
        #[cfg(not(unix))]
        let _ = mode;
        let mut f = opts.open(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        fs::rename(&tmp, path)?;
        #[cfg(unix)]
        fs::File::open(dir)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.with_context(|| format!("write {}", path.display()))
}

/// Create a private directory (0700 on Unix; on Windows the user's profile already is).
pub fn private_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bascule-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn set_oauth_keeps_other_keys_and_mode() {
        let d = tmpdir("store");
        let slot = Slot::config_dir(&d).files_only();
        atomic_write(
            &slot.creds,
            &json!({"mcpOAuth": {"x": 1}, "claudeAiOauth": {"accessToken": "a"}}),
        )
        .unwrap();
        slot.set_oauth(Some(&json!({"accessToken": "b"}))).unwrap();
        let all: Value = serde_json::from_slice(&fs::read(&slot.creds).unwrap()).unwrap();
        assert_eq!(all["mcpOAuth"]["x"], 1);
        assert_eq!(all["claudeAiOauth"]["accessToken"], "b");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&slot.creds).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        slot.set_oauth(None).unwrap();
        assert!(slot.oauth().unwrap().is_none());
        let all: Value = serde_json::from_slice(&fs::read(&slot.creds).unwrap()).unwrap();
        assert_eq!(all["mcpOAuth"]["x"], 1);
        fs::remove_dir_all(d).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlink() {
        let d = tmpdir("symlink");
        let real = d.join("real.json");
        atomic_write(&real, &json!({})).unwrap();
        let link = d.join("link.json");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(atomic_write(&link, &json!({"a": 1})).is_err());
        fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn fingerprint_hides_token() {
        let f = fp("sk-ant-oat01-secret");
        assert_eq!(f.len(), 10);
        assert!(!f.contains("secret"));
        assert_eq!(fp(""), "-");
    }
}
