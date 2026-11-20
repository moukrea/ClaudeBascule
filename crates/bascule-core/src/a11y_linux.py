# Claude Bascule: drive the Claude in Chrome extension through the desktop's accessibility layer
# (AT-SPI), like a screen reader does: sign it out, in, and approve claude.ai's consent.
#   python3 -I - probe          -> {"chrome": bool, "content": bool}
#   python3 -I - resign <email> -> {"ok": bool, "error": str|null}
import json, sys, time

try:
    import gi
    gi.require_version("Atspi", "2.0")
    from gi.repository import Atspi
except Exception as e:  # no AT-SPI bindings on this system
    print(json.dumps({"chrome": False, "content": False, "ok": False, "error": "no AT-SPI bindings: %s" % e}))
    sys.exit(0)

LOGOUT = {"Abmelden", "Cerrar sesión", "Esci", "Keluar", "Log out", "Sair", "Se déconnecter", "लॉग आउट करें", "ログアウト", "로그아웃"}
SIGNIN = {"Accedi", "Anmelden", "Fazer login", "Iniciar sesión", "Log in", "Masuk", "Se connecter", "लॉग इन करें", "ログイン", "로그인"}
APPROVE = {"Autoriser", "Authorize", "Allow", "Autorisieren", "Zulassen", "Autorizar", "Permitir", "Autorizza", "Consenti", "Izinkan", "許可", "허용", "अनुमति दें"}
BROWSERS = ("Chrome", "Chromium", "Brave", "Edge", "Vivaldi")


def browsers():
    d = Atspi.get_desktop(0)
    out = []
    for i in range(d.get_child_count()):
        try:
            a = d.get_child_at_index(i)
            if a and any(b in (a.get_name() or "") for b in BROWSERS):
                out.append(a)
        except Exception:
            pass
    return out


def walk(root, visit, limit=60000):
    stack, n = [root], 0
    while stack and n < limit:
        node = stack.pop()
        n += 1
        try:
            if visit(node):
                return node
            count = node.get_child_count()
        except Exception:
            continue
        for j in range(min(count, 1000)):
            try:
                c = node.get_child_at_index(j)
                if c:
                    stack.append(c)
            except Exception:
                pass
    return None


def is_button(n, labels):
    try:
        return n.get_role_name() in ("button", "push button") and (n.get_name() or "").strip() in labels
    except Exception:
        return False


def find(labels):
    for a in browsers():
        hit = walk(a, lambda n: is_button(n, labels))
        if hit:
            return hit
    return None


def node_text(n):
    # Chrome gives static text through the Text interface; names hold labels.
    out = n.get_name() or ""
    try:
        t = n.get_text_iface()
        if t:
            out += " " + (t.get_text(0, t.get_character_count()) or "")
    except Exception:
        pass
    return out.lower()


def has_text(s):
    s = s.lower()
    for a in browsers():
        if walk(a, lambda n: s in node_text(n)):
            return True
    return False


def press(n):
    try:
        n.get_component_iface().grab_focus()
    except Exception:
        pass
    try:
        ai = n.get_action_iface()
        for i in range(ai.get_n_actions()):
            if ai.get_action_name(i) in ("press", "click", "activate"):
                return ai.do_action(i)
        return ai.do_action(0)
    except Exception:
        return False


def wait(labels, secs):
    end = time.time() + secs
    while time.time() < end:
        n = find(labels)
        if n:
            return n
        time.sleep(0.4)
    return None


def probe():
    apps = browsers()
    content = any(walk(a, lambda n: n.get_role_name() in ("document web", "document frame")) for a in apps)
    return {"chrome": bool(apps), "content": bool(content)}


def resign(email):
    # Claude in Chrome's options page is open (Claude Bascule's extension opened it).
    if not wait(LOGOUT | SIGNIN, 20):
        return "Claude in Chrome's options are not visible (is Chrome started with accessibility on?)"
    b = find(LOGOUT)
    if b:
        press(b)
        if not wait(SIGNIN, 12):
            return "Claude in Chrome did not sign out"
    b = wait(SIGNIN, 10)
    if not b:
        return "Claude in Chrome did not offer to sign in"
    press(b)
    if not wait(APPROVE, 30):
        return "claude.ai's sign-in page did not open"
    # The page names the signed-in account a moment after its buttons appear.
    end = time.time() + 10
    while time.time() < end and not has_text(email):
        time.sleep(0.5)
    if not has_text(email):
        return "claude.ai is not signed in with %s in this browser" % email
    time.sleep(1.0)  # the page settles before it accepts the press
    # The first press may only focus the button: press again until the page goes.
    for _ in range(6):
        b = find(APPROVE)
        if not b:
            return None
        press(b)
        end = time.time() + 5
        while time.time() < end and find(APPROVE):
            time.sleep(0.5)
    return None if not find(APPROVE) else "claude.ai's consent was not accepted"


if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else "probe"
    if cmd == "probe":
        print(json.dumps(probe()))
    elif cmd == "resign":
        err = resign(sys.argv[2])
        print(json.dumps({"ok": err is None, "error": err}))
