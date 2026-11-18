#!/usr/bin/env python3
"""Render the README images with made-up accounts and quotas (no real data):
assets/header-{light,dark}.png and assets/screenshot-{light,dark}.png.

    python3 tools/readme-shots.py      # needs chromium (or google-chrome) and Pillow
"""
import json, shutil, subprocess, sys, tempfile, time
from pathlib import Path
from PIL import Image, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
UI = ROOT / "app/ui"
OUT = ROOT / "assets"
CHROME = next((c for c in ("chromium-browser", "chromium", "google-chrome") if shutil.which(c)), None)

NOW = int(time.time() * 1000)
DAY = 86400_000


def iso(ms):
    return time.strftime("%Y-%m-%dT%H:%M:%S+00:00", time.gmtime(ms / 1000))


def win(name, pct, days):
    sev = "critical" if pct >= 95 else "warning" if pct >= 75 else "normal"
    return {"active": True, "name": name, "percent": float(pct), "resets_at": iso(NOW + days * DAY), "severity": sev}


def account(name, email, place, tier, windows, resets_left):
    grants = [{"clears": ["seven_day"], "ends_at": iso(NOW + 20 * DAY), "kind": "weekly", "label": "One full limit reset",
               "paused": False, "resets_left": resets_left, "resets_total": 1, "usable_now": False, "use_requires_limit": True}]
    return {"name": name, "email": email, "label": email, "has_alias": False, "place": place, "in_rotation": True, "exhausted": None,
            "subscription": "max", "tier": tier, "token": {"expires_at": NOW + DAY, "refresh_expires_at": NOW + 30 * DAY},
            "usage": {"fetched_at": NOW - 60_000, "windows": windows,
                      "resets": {"at_limit": False, "cooldown_until": None, "eligible": True, "grants": grants}},
            "usage_error": None}


def fake_state(lang):
    settings = {"active_poll_seconds": 300, "anticipate": True, "anticipate_model_windows": True, "auto_send": False, "auto_switch": True,
                "collect_seconds": 30, "confirm_seconds": 20, "display_names": {}, "hide_emails": False, "language": lang,
                "min_switch_gap_seconds": 300, "next_account_rule": "headroom", "notify": True, "observe_poll_seconds": 30,
                "read_resets": True, "refresh_days_before": 3, "reread_resting_minutes": 15, "reset_margin_seconds": 120,
                "resume_message": "", "rotation": [], "spontaneous_restart_seconds": 10, "switch_on_limit": True,
                "threshold_percent": 95.0, "thresholds": {}, "check_updates": True}
    accounts = [
        account("alex", "alex@example.com", "active", "default_claude_max_20x", [win("seven_day", 71, 3), win("model:Fable", 88, 3)], 1),
        account("studio", "studio@example.org", "resting", "default_claude_max_20x", [win("seven_day", 34, 5), win("model:Fable", 41, 5)], 0),
        account("lab", "lab@example.net", "resting", "default_claude_max_5x", [win("five_hour", 12, 0.15), win("seven_day", 9, 6)], 1),
    ]
    sessions = [{"session": f"s{i}", "title": t, "cwd": f"~/code/{c}", "model": "claude-fable-5-2", "status": st, "dialog": None,
                 "lastTurn": {"auto_resume_at": None, "ended_at": NOW - 300_000, "limit_kind": None, "reason": "final_message", "resets_at": None}}
                for i, (t, c, st) in enumerate([("Refactor the billing module", "shop", "busy"), ("Write the release notes", "docs", "idle")])]
    events = [{"at": NOW - 3 * 3600_000, "kind": "daemon.start", "data": {}},
              {"at": NOW - 2 * 3600_000, "kind": "switch", "data": {"from": "lab", "to": "alex", "reason": "anticipate: seven_day 95%", "manual": False}}]
    return {"accounts": accounts, "active": "alex", "at": NOW, "default_message": "Continue.", "events": events, "jaunt": "connected",
            "language": lang, "last_switch": None, "login": {}, "observe_only": False, "resume": [], "sessions": sessions,
            "settings": settings, "system_language": lang, "version": "1.0.0", "waiting_return": None,
            "update": {"current": "1.0.0", "available": False}, "platform": "linux"}


def shoot(html, png, w, h, dark):
    flags = ["--headless=new", "--disable-gpu", "--no-sandbox", "--hide-scrollbars", "--allow-file-access-from-files", f"--window-size={w},{h}", "--force-device-scale-factor=2",
             "--virtual-time-budget=5000", "--default-background-color=00000000", f"--screenshot={png}"]
    if dark:
        flags += ["--force-dark-mode", "--blink-settings=preferredColorScheme=0"]
    with tempfile.TemporaryDirectory() as prof:
        subprocess.run([CHROME, f"--user-data-dir={prof}", *flags, f"file://{html}"], check=True, capture_output=True, timeout=120)


def with_shadow(png, dark):
    """The window on a transparent canvas with a soft shadow, as on a desktop."""
    win = Image.open(png).convert("RGBA")
    pad = 120
    canvas = Image.new("RGBA", (win.width + 2 * pad, win.height + 2 * pad), (0, 0, 0, 0))
    shadow = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
    alpha = win.getchannel("A").point(lambda a: int(a * (0.55 if dark else 0.28)))
    shadow.paste(Image.new("RGBA", win.size, (0, 0, 0, 255)), (pad, pad + 36), alpha)
    canvas.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(48)))
    canvas.alpha_composite(win, (pad, pad))
    canvas.save(png, optimize=True)


def main():
    if not CHROME:
        sys.exit("chromium not found")
    with tempfile.TemporaryDirectory() as d:
        d = Path(d)
        shutil.copytree(UI, d / "ui")
        ui = d / "ui"
        for theme in ("light", "dark"):
            st = fake_state("en")
            shim = ("localStorage.setItem('bascule.theme', '%s'); localStorage.setItem('bascule.accent', 'coral');\n" % theme
                    + "localStorage.setItem('bascule.wcorder', 'macos'); localStorage.setItem('bascule.wcside', 'left'); localStorage.setItem('bascule.page', 'accounts');\n"
                    + "window.__STATE__ = " + json.dumps(st) + ";\n"
                    + "const noop = () => Promise.resolve();\n"
                    + "window.__TAURI__ = { core: { invoke: (cmd, args) => cmd === 'call' && args.method === 'state' ? Promise.resolve(window.__STATE__) : Promise.resolve(null) },\n"
                    + "  window: { getCurrentWindow: () => ({ unminimize: noop, show: noop, setFocus: noop, requestUserAttention: noop, isMaximized: () => Promise.resolve(false), onResized: noop, onFocusChanged: noop, close: noop, minimize: noop, toggleMaximize: noop, startResizeDragging: noop }) } };\n")
            (ui / "shim.js").write_text(shim)
            html = (UI / "index.html").read_text().replace('<script src="i18n.js"></script>', '<script src="shim.js"></script><script src="i18n.js"></script>')
            page = ui / f"shot-{theme}.html"
            page.write_text(html)
            png = OUT / f"screenshot-{theme}.png"
            shoot(page, png, 1080, 700, theme == "dark")
            with_shadow(png, theme == "dark")
            # Header: the icon and the name in the app's font.
            ink = "#f2f2f0" if theme == "dark" else "#141414"
            (ui / f"header-{theme}.html").write_text(f"""<!doctype html><meta charset="utf-8"><style>
@font-face {{ font-family: Plex; src: url(fonts/ibm-plex-sans-latin-600-normal.woff2); font-weight: 600; }}
html, body {{ margin: 0; background: transparent; }}
.h {{ display: flex; align-items: center; justify-content: center; gap: 26px; height: 160px; font: 600 60px Plex; letter-spacing: -.03em; color: {ink}; }}
img {{ width: 104px; height: 104px; }}
</style><div class="h"><img src="{ROOT / 'assets/icon.svg'}"><span>Claude Bascule</span></div>""")
            hp = OUT / f"header-{theme}.png"
            shoot(ui / f"header-{theme}.html", hp, 640, 160, theme == "dark")
            im = Image.open(hp)
            im.crop(im.getchannel("A").getbbox()).save(hp, optimize=True)
    print("written:", *sorted(p.name for p in OUT.glob("*-*.png")))


if __name__ == "__main__":
    main()
