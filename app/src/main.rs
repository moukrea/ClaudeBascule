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
