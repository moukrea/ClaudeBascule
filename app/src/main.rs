//! Claude Bascule's mini-app: one window over the service's local endpoint (bascule-core::control).
//! `bascule-app --service` runs the service itself, without a window (used at login on Windows,
//! where a console program would show a terminal).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use bascule_core::config::{AccountList, Settings};
use bascule_core::control;
use bascule_core::paths::Paths;
use bascule_core::platform;
use serde_json::Value;

/// Any service method (state, switch, resume.*, settings.*, login.*, accounts.*, update.*), off the main thread.
#[tauri::command]
async fn call(method: String, params: Value) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        control::call(&Paths::from_env(), &method, params).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Open a page (sign-in, source code) in the default browser.
#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    platform::open_url(&url).map_err(|e| format!("{e:#}"))
}

/// Install the latest release over this one: the platform's install command, detached, which
/// replaces the binaries, restarts the service and opens the app again.
#[tauri::command]
fn apply_update() -> Result<(), String> {
    let (program, args) = bascule_core::update::install_command();
    // Linux: the window often runs in its own systemd unit, whose processes all stop with it; the
    // installer closes the window, so it runs in a unit of its own (with the session's display,
    // to open the new window).
    #[cfg(target_os = "linux")]
    {
        let mut cmd = std::process::Command::new("systemd-run");
        cmd.args(["--user", "--collect", "--quiet", "--unit"])
            .arg(format!("claude-bascule-update-{}", std::process::id()));
        for k in [
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "XDG_RUNTIME_DIR",
            "DBUS_SESSION_BUS_ADDRESS",
            "XDG_CURRENT_DESKTOP",
            "XDG_SESSION_TYPE",
            "PATH",
            "HOME",
        ] {
            if let Ok(v) = std::env::var(k) {
                cmd.arg(format!("--setenv={k}={v}"));
            }
        }
        cmd.arg("--setenv=CLAUDE_BASCULE_RELAUNCH=1")
            .arg(&program)
            .args(&args);
        if platform::detach(&mut cmd)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return Ok(());
        }
    }
    let mut cmd = std::process::Command::new(program);
    cmd.args(args).env("CLAUDE_BASCULE_RELAUNCH", "1");
    platform::detach(&mut cmd)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn service() -> anyhow::Result<()> {
    let paths = Paths::from_env();
    let list = AccountList::load(&paths.accounts_file())?;
    let settings = Settings::load(&paths.settings_file())?;
    bascule_core::daemon::run_service(paths, list, settings, false)
}

fn main() {
    if std::env::args().any(|a| a == "--service") {
        if let Err(e) = service() {
            eprintln!("claude-bascule: {e:#}");
            std::process::exit(1);
        }
        return;
    }
    // Linux: name the program like its desktop entry (claude-bascule.desktop), so docks and
    // launchers tie the window to its icon (Wayland app id, X11 WM_CLASS). GTK has no name
    // otherwise, as Tauri initialises it without arguments.
    #[cfg(target_os = "linux")]
    {
        gtk::glib::set_prgname(Some("claude-bascule"));
        // The safe wrapper refuses to run before GDK is initialised (Tauri does that later),
        // while the C function is meant to be called first.
        unsafe { gtk::gdk::ffi::gdk_set_program_class(c"claude-bascule".as_ptr()) };
    }
    let paths = Paths::from_env();
    let _ = bascule_core::store::private_dir(&paths.state);
    let pid_file = paths.state.join("app.pid");
    let _ = std::fs::write(&pid_file, std::process::id().to_string());
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![call, open_url, apply_update])
        .run(tauri::generate_context!())
        .expect("Claude Bascule could not start");
    let _ = std::fs::remove_file(pid_file);
}
