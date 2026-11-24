# Claude Bascule: drive the Claude in Chrome extension through the desktop's accessibility bus
# (AT-SPI), like a screen reader does: sign it out, in, and approve claude.ai's consent.
# Plain D-Bus calls through Gio: libatspi loads a whole application's tree into its cache first,
# which hangs on a browser with many tabs. Only the pages concerned are walked here.
#   python3 -I - probe          -> {"chrome": bool, "content": bool}
#   python3 -I - resign <email> [login-seconds] -> {"stage": ...} lines, then {"ok": bool, "error": str|null}
import json, os, subprocess, sys, time

try:
    from gi.repository import Gio, GLib
except Exception as e:  # no GLib bindings for Python on this system
    print(json.dumps({"chrome": False, "content": False, "ok": False, "error": "no GLib bindings for Python: %s" % e}))
    sys.exit(0)

CIC = "fcoeoabgfenejglbffodgkkbkcdhcgfn"
CIC_CLIENT = "dae2cad8-15c5-43d2-9046-fcaecc135fa4"
LOGOUT = {"Abmelden", "Cerrar sesión", "Esci", "Keluar", "Log out", "Sair", "Se déconnecter", "लॉग आउट करें", "ログアウト", "로그아웃"}
SIGNIN = {"Accedi", "Anmelden", "Fazer login", "Iniciar sesión", "Log in", "Masuk", "Se connecter", "लॉग इन करें", "ログイン", "로그인"}
APPROVE = {"Autoriser", "Authorize", "Allow", "Autorisieren", "Zulassen", "Autorizar", "Permitir", "Autorizza", "Consenti", "Izinkan", "許可", "허용", "अनुमति दें"}
BROWSERS = ("Chrome", "Chromium", "Brave", "Edge", "Vivaldi")
DOCUMENTS = ("document web", "document frame")
BUTTONS = ("push button", "button")
ROOT = "/org/a11y/atspi/accessible/root"
ACC = "org.a11y.atspi.Accessible"
T = 3000  # milliseconds per call

_conn = None


def conn():
    global _conn
    if _conn is None:
        s = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        addr = s.call_sync("org.a11y.Bus", "/org/a11y/bus", "org.a11y.Bus", "GetAddress", None,
                           GLib.VariantType("(s)"), 0, T, None).unpack()[0]
        flags = Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION
        _conn = Gio.DBusConnection.new_for_address_sync(addr, flags, None, None)
    return _conn


def call(node, iface, method, args=None, rtype=None):
    try:
        r = conn().call_sync(node[0], node[1], iface, method, args,
                             GLib.VariantType(rtype) if rtype else None, 0, T, None)
        return r.unpack() if r is not None else None
    except Exception:
        return None


def name(node):
    r = call(node, "org.freedesktop.DBus.Properties", "Get", GLib.Variant("(ss)", (ACC, "Name")), "(v)")
    return (r[0] if r else None) or ""


def children(node):
    r = call(node, ACC, "GetChildren", None, "(a(so))")
    return [tuple(c) for c in r[0]] if r else []


def many(reqs):
    # Several calls at once (one round trip each would make a big browser slow to walk).
    out = [None] * len(reqs)
    if not reqs:
        return out
    loop, left = GLib.MainLoop(), [len(reqs)]

    def done(c, res, i):
        try:
            out[i] = c.call_finish(res).unpack()
        except Exception:
            pass
        left[0] -= 1
        if left[0] == 0:
            loop.quit()

    for i, (node, iface, method, args, rtype) in enumerate(reqs):
        conn().call(node[0], node[1], iface, method, args, GLib.VariantType(rtype) if rtype else None,
                    0, T, None, done, i)
    guard = GLib.timeout_add(T + 1000, loop.quit)
    loop.run()
    if left[0] == 0:  # else the guard fired, and is gone
        GLib.source_remove(guard)
    return out


def walk(roots, visit, labels=True, budget=8000):
    # Breadth first, one batch of calls per level: visit(node, role, name, text) -> "stop", "skip"
    # (do not go below this node) or None. Names and texts are only read when `labels`.
    level, n = list(roots), 0
    while level and n < budget:
        n += len(level)
        roles = many([(x, ACC, "GetRoleName", None, "(s)") for x in level])
        none = [None] * len(level)
        names = many([(x, "org.freedesktop.DBus.Properties", "Get", GLib.Variant("(ss)", (ACC, "Name")), "(v)") for x in level]) if labels else none
        texts = many([(x, "org.a11y.atspi.Text", "GetText", GLib.Variant("(ii)", (0, -1)), "(s)") for x in level]) if labels else none
        expand = []
        for x, r, nm, tx in zip(level, roles, names, texts):
            v = visit(x, r[0] if r else "", (nm[0] if nm else None) or "", tx[0] if tx else "")
            if v == "stop":
                return
            if v != "skip":
                expand.append(x)
        kids = many([(x, ACC, "GetChildren", None, "(a(so))") for x in expand])
        level = [tuple(c) for k in kids if k for c in k[0]]


def browsers():
    return [c for c in children(("org.a11y.atspi.Registry", ROOT)) if any(b in name(c) for b in BROWSERS)]


# Browser interface parts that hold no page (not walked into).
LEAVES = {"push button", "button", "toggle button", "notification", "page tab", "page tab list",
          "tool bar", "static", "label", "separator", "slider", "entry", "menu", "menu bar",
          "menu item", "check box", "radio button", "combo box", "image", "icon", "link", "alert",
          "status bar", "progress bar", "tool tip", "text", "list", "list item", "spin button"}


def documents():
    # The pages shown (one per window, plus extension views), without walking into any of them.
    docs = []

    def visit(node, r, nm, tx):
        if r in DOCUMENTS:
            docs.append(node)
            return "skip"
        return "skip" if r in LEAVES else None

    walk([(a[0], ROOT) for a in browsers()], visit, labels=False)
    uris = many([(d, "org.a11y.atspi.Document", "GetAttributeValue", GLib.Variant("(s)", ("URI",)), "(s)") for d in docs])
    return [(d, (u[0] if u else "")) for d, u in zip(docs, uris)]


def is_options(u):
    return u.startswith("chrome-extension://%s/options.html" % CIC)


def is_consent(u):
    # Claude in Chrome's sign-in page on claude.ai (its client id is in the address).
    return "/oauth/authorize" in u and (CIC_CLIENT in u or "client_id" not in u)


def scan(doc, labels, email=None):
    # A button among `labels` in the page, and whether the page shows `email`.
    found = {"button": None, "named": email is None}

    def visit(node, r, nm, tx):
        if found["button"] is None and r in BUTTONS and nm.strip() in labels:
            found["button"] = node
        if not found["named"] and email in (nm + " " + tx).lower():
            found["named"] = True
        if found["button"] and found["named"]:
            return "stop"
        return None

    walk([doc], visit)
    return found["button"], found["named"]


def page(pred):
    return [d for d, u in documents() if pred(u)]


def find(pred, labels):
    for doc in page(pred):
        b, _ = scan(doc, labels)
        if b:
            return b
    return None


def wait(fn, secs, step=0.5):
    end = time.time() + secs
    while True:
        r = fn()
        if r or time.time() >= end:
            return r
        time.sleep(step)


def press(node):
    r = call(node, "org.a11y.atspi.Action", "GetActions", None, "(a(sss))")
    idx = 0
    for i, a in enumerate(r[0] if r else []):
        if a[0] in ("press", "click", "activate"):
            idx = i
            break
    call(node, "org.a11y.atspi.Component", "GrabFocus", None, "(b)")
    call(node, "org.a11y.atspi.Action", "DoAction", GLib.Variant("(i)", (idx,)), "(b)")


def open_options():
    # Claude in Chrome's options in a new tab of the running browser (it went behind another tab).
    try:
        app = browsers()[0]
        pid = conn().call_sync("org.freedesktop.DBus", "/org/freedesktop/DBus",
                               "org.freedesktop.DBus", "GetConnectionUnixProcessID", GLib.Variant("(s)", (app[0],)),
                               GLib.VariantType("(u)"), 0, T, None).unpack()[0]
        exe = os.readlink("/proc/%d/exe" % pid)
        subprocess.Popen([exe, "chrome-extension://%s/options.html" % CIC], stdin=subprocess.DEVNULL,
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
        return True
    except Exception:
        return False


def probe():
    apps = browsers()
    return {"chrome": bool(apps), "content": bool(apps and documents())}


def resign(email, login_secs=600):
    email = email.lower()
    if not browsers():
        return "Chrome is not on the desktop's accessibility bus (is it started with accessibility on?)"
    # Claude in Chrome's options page was opened by Claude Bascule.
    if not wait(lambda: page(is_options), 8) and not (open_options() and wait(lambda: page(is_options), 15)):
        return "Claude in Chrome's options are not visible (is Chrome started with accessibility on?)"
    if not wait(lambda: find(is_options, LOGOUT | SIGNIN), 15):
        return "Claude in Chrome's options did not load"
    out = find(is_options, LOGOUT)
    if out:
        press(out)
        if not wait(lambda: find(is_options, SIGNIN), 8):
            out = find(is_options, LOGOUT)
            if out:
                press(out)
            if not wait(lambda: find(is_options, SIGNIN), 8):
                return "Claude in Chrome did not sign out"
    sign_in = find(is_options, SIGNIN)
    if not sign_in:
        return "Claude in Chrome did not offer to sign in"
    press(sign_in)
    return approve(email, login_secs)


def is_login(u):
    return u.startswith("https://claude.ai/login") or u.startswith("https://claude.ai/magic-link")


def stage(name):
    # Progress for Claude Bascule, which reads every line.
    print(json.dumps({"stage": name}), flush=True)


def approve(email, login_secs):
    # Claude in Chrome's sign-in opens claude.ai's consent page in a tab. When claude.ai is signed
    # out there, its sign-in page shows instead: the user signs in (for up to `login_secs`), then
    # the consent shows, or Claude in Chrome's sign-in is pressed again.
    end = time.time() + 35
    asked_login, pressed_again, wrong_since = False, 0.0, None
    while time.time() < end:
        docs = documents()
        consent = [d for d, u in docs if is_consent(u)]
        if consent:
            button, named = scan(consent[0], APPROVE, email)
            if button and named:
                break
            if button:
                # The page names the account a moment after its button shows.
                wrong_since = wrong_since or time.time()
                if time.time() - wrong_since > 10:
                    return "claude.ai is not signed in with %s in this browser" % email
        elif any(is_login(u) for d, u in docs):
            if not asked_login:
                asked_login = True
                stage("sign_in")
                end = time.time() + login_secs
        elif time.time() - pressed_again > 20 and (asked_login or time.time() > end - 20):
            # Signed in to claude.ai (or the first press was lost): sign Claude in Chrome in again.
            pressed_again = time.time()
            again = find(is_options, SIGNIN)
            if not again and open_options():
                again = wait(lambda: find(is_options, SIGNIN), 10)
            if again:
                press(again)
        time.sleep(0.5)
    else:
        if asked_login:
            return "claude.ai was not signed in to"
        return "claude.ai's sign-in page did not open"
    if asked_login:
        stage("approving")
    time.sleep(1.0)  # the page settles before it accepts the press
    # The first press may only focus the button: press again until the page goes.
    for _ in range(6):
        b = find(is_consent, APPROVE)
        if not b:
            return None
        press(b)
        if wait(lambda: not find(is_consent, APPROVE), 5):
            return None
    return "claude.ai's consent was not accepted"


if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else "probe"
    try:
        if cmd == "probe":
            print(json.dumps(probe()))
        elif cmd == "resign":
            err = resign(sys.argv[2], float(sys.argv[3]) if len(sys.argv) > 3 else 600)
            print(json.dumps({"ok": err is None, "error": err}))
    except Exception as e:
        print(json.dumps({"chrome": False, "content": False, "ok": False, "error": "accessibility: %s" % e}))
