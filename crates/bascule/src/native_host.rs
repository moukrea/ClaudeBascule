//! The browser's native messaging host: Chrome (or Edge, Brave, Chromium) starts `bascule` with
//! the extension's origin as argument; messages are 32-bit length-prefixed JSON on stdin/stdout.
//! It relays the Connector extension's reports to the service and the service's commands to the
//! extension. Cookies only pass through; nothing is written here or logged.

use bascule_core::control;
use bascule_core::paths::Paths;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// What the extension may ask the service.
const ALLOWED: &[&str] = &["web.hello", "web.capture", "web.result"];

fn read_msg(r: &mut impl Read) -> Option<Value> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len).ok()?;
    let n = u32::from_le_bytes(len) as usize;
    if n > 8 * 1024 * 1024 {
        return None;
    }
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf).ok()?;
    serde_json::from_slice(&buf).ok()
}

fn write_msg(out: &Mutex<std::io::Stdout>, v: &Value) {
    let bytes = serde_json::to_vec(v).unwrap_or_default();
    let mut o = out.lock().unwrap();
    let _ = o.write_all(&(bytes.len() as u32).to_le_bytes());
    let _ = o.write_all(&bytes);
    let _ = o.flush();
}

pub fn run() {
    let paths = Paths::from_env();
    let out = Arc::new(Mutex::new(std::io::stdout()));
    {
        let (out, paths) = (out.clone(), paths.clone());
        std::thread::spawn(move || loop {
            match control::call(&paths, "web.next", json!({"timeout_ms": 20000})) {
                Ok(Value::Null) => {}
                Ok(cmd) => write_msg(&out, &json!({"push": cmd})),
                Err(_) => std::thread::sleep(Duration::from_secs(3)),
            }
        });
    }
    let mut stdin = std::io::stdin().lock();
    while let Some(m) = read_msg(&mut stdin) {
        let id = m.get("id").cloned().unwrap_or(Value::Null);
        let method = m.get("method").and_then(Value::as_str).unwrap_or("");
        let reply = if !ALLOWED.contains(&method) {
            json!({"id": id, "error": "not allowed"})
        } else {
            let params = m.get("params").cloned().unwrap_or(json!({}));
            match control::call(&paths, method, params) {
                Ok(r) => json!({"id": id, "result": r}),
                Err(e) => json!({"id": id, "error": format!("{e:#}")}),
            }
        };
        write_msg(&out, &reply);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames() {
        let v = json!({"id": 1, "method": "web.hello"});
        let bytes = serde_json::to_vec(&v).unwrap();
        let mut framed = (bytes.len() as u32).to_le_bytes().to_vec();
        framed.extend(bytes);
        assert_eq!(read_msg(&mut framed.as_slice()).unwrap(), v);
        assert!(read_msg(&mut [1u8, 0].as_slice()).is_none());
    }
}
