//! Browsers installed with Flatpak (Linux). Their sandbox does not see Claude Bascule's files nor
//! its socket, but it shares the app's own folder (`~/.var/app/<id>`, at the same path). There,
//! Claude Bascule keeps a copy of the Connector extension, a copy of `bascule` as the browser's
//! native messaging host, and a socket of the service that answers the browser's requests only.
//! No Flatpak permission is changed.

use crate::store::private_dir;
use anyhow::Result;
use std::path::{Path, PathBuf};

pub struct App {
    /// Flatpak application id.
    pub id: &'static str,
    /// Its configuration folder under `config/` (where native messaging hosts are read).
    pub conf: &'static str,
    pub name: &'static str,
    /// The browser family, as `platform::browsers` names it.
    pub family: &'static str,
}

pub const APPS: &[App] = &[
    App {
        id: "com.google.Chrome",
        conf: "google-chrome",
        name: "Google Chrome",
        family: "chrome",
    },
    App {
        id: "org.chromium.Chromium",
        conf: "chromium",
        name: "Chromium",
        family: "chromium",
    },
    App {
        id: "com.brave.Browser",
        conf: "BraveSoftware/Brave-Browser",
        name: "Brave",
        family: "brave",
    },
    App {
        id: "com.microsoft.Edge",
        conf: "microsoft-edge",
        name: "Microsoft Edge",
        family: "edge",
    },
    App {
        id: "com.vivaldi.Vivaldi",
        conf: "vivaldi",
        name: "Vivaldi",
        family: "vivaldi",
    },
];

/// The native messaging host's name, as the extension asks for it.
pub const HOST: &str = "io.github.moukrea.claudebascule";
/// The Connector's fixed extension id (from its manifest's key).
pub const EXTENSION_ID: &str = "comipnngbifajjpfbagbmlfgckllcobj";

/// The command Flatpak exports for an installed app (user installation first).
pub fn launcher(home: &Path, app: &App) -> Option<PathBuf> {
    [
        home.join(".local/share/flatpak/exports/bin"),
        PathBuf::from("/var/lib/flatpak/exports/bin"),
    ]
    .into_iter()
    .map(|d| d.join(app.id))
    .find(|p| p.is_file())
}

/// Claude Bascule's folder inside the app's own folder.
pub fn data_dir(home: &Path, app: &App) -> PathBuf {
    home.join(".var/app")
        .join(app.id)
        .join("data")
        .join("claude-bascule")
}

/// The copy of the Connector the browser can load.
pub fn extension_dir(home: &Path, app: &App) -> PathBuf {
    data_dir(home, app).join("chrome-extension")
}

/// The service's socket for that browser.
pub fn socket(home: &Path, app: &App) -> PathBuf {
    data_dir(home, app).join("bascule.sock")
}

/// Write `bytes` to `path` unless it already holds them.
fn put(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    if std::fs::read(path).ok().as_deref() != Some(bytes) {
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode))?;
        }
        std::fs::rename(&tmp, path)?;
    }
    #[cfg(not(unix))]
    let _ = mode;
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let target = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &target)?;
        } else {
            put(&target, &std::fs::read(e.path())?, 0o644)?;
        }
    }
    Ok(())
}

/// For each Flatpak browser installed: the host (a copy of `exe`), its manifest, and a copy of
/// the extension at `extension` (when given). Returns the sockets the service should answer on.
pub fn setup(home: &Path, exe: &Path, extension: Option<&Path>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for app in APPS.iter().filter(|a| launcher(home, a).is_some()) {
        let r = (|| -> Result<PathBuf> {
            let dir = data_dir(home, app);
            private_dir(&dir)?;
            let host = dir.join("bascule");
            put(&host, &std::fs::read(exe)?, 0o755)?;
            if let Some(src) = extension {
                copy_dir(src, &extension_dir(home, app))?;
            }
            let hosts = home
                .join(".var/app")
                .join(app.id)
                .join("config")
                .join(app.conf)
                .join("NativeMessagingHosts");
            std::fs::create_dir_all(&hosts)?;
            let manifest = serde_json::json!({
                "name": HOST,
                "description": "Claude Bascule",
                "path": host,
                "type": "stdio",
                "allowed_origins": [format!("chrome-extension://{EXTENSION_ID}/")],
            });
            put(
                &hosts.join(format!("{HOST}.json")),
                format!("{}\n", serde_json::to_string_pretty(&manifest)?).as_bytes(),
                0o644,
            )?;
            Ok(socket(home, app))
        })();
        if let Ok(s) = r {
            out.push(s);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_up_an_installed_flatpak_browser_only() {
        let home = std::env::temp_dir().join(format!("bascule-flatpak-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let exports = home.join(".local/share/flatpak/exports/bin");
        std::fs::create_dir_all(&exports).unwrap();
        std::fs::write(exports.join("com.google.Chrome"), "#!/bin/sh\n").unwrap();
        let exe = home.join("bascule");
        std::fs::write(&exe, b"binary").unwrap();
        let ext = home.join("ext");
        std::fs::create_dir_all(ext.join("icons")).unwrap();
        std::fs::write(ext.join("manifest.json"), "{}").unwrap();
        std::fs::write(ext.join("icons/16.png"), "png").unwrap();

        let sockets = setup(&home, &exe, Some(&ext));
        let chrome = &APPS[0];
        assert_eq!(sockets, vec![socket(&home, chrome)]);
        assert_eq!(
            std::fs::read(data_dir(&home, chrome).join("bascule")).unwrap(),
            b"binary"
        );
        assert!(extension_dir(&home, chrome).join("icons/16.png").is_file());
        let m: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(home.join(
                ".var/app/com.google.Chrome/config/google-chrome/NativeMessagingHosts/io.github.moukrea.claudebascule.json",
            ))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            m["path"].as_str().unwrap(),
            data_dir(&home, chrome).join("bascule").to_str().unwrap()
        );
        // Chromium is not installed here: nothing is written for it.
        assert!(!home.join(".var/app/org.chromium.Chromium").exists());
        let _ = std::fs::remove_dir_all(&home);
    }
}
