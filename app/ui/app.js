// Bascule mini-app. Every value comes from the daemon (`state`), every action is a daemon call.
// Strings: i18n.js (en, fr, de, es, it, pt). DOM is built with textContent only.
"use strict";

const T = window.__TAURI__;
const invoke = (m, p) => T.core.invoke(m, p);
const call = (method, params = {}) => invoke("call", { method, params });
const win = () => T.window.getCurrentWindow();

// ---------- strings ----------
let S = null, lang = "en";
function tr(key, vars = {}) {
  const s = (I18N[lang] && I18N[lang][key]) ?? I18N.en[key] ?? key;
  return s.replace(/\{(\w+)\}/g, (_, k) => (vars[k] ?? ""));
}
const trn = (base, n, vars = {}) => tr(n === 1 ? base + "_one" : base + "_other", { n, ...vars });

// ---------- icons ----------
const ICON = {
  more: "M5 12h.01M12 12h.01M19 12h.01", swap: "M4 8h13l-3.5-3.5M20 16H7l3.5 3.5", key: "M15 7a4 4 0 1 1-3.9 5H3v3h3v3h3v-3h2.1A4 4 0 0 1 15 7z",
  refresh: "M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7", rotate: "M17 1l4 4-4 4M3 11V9a4 4 0 0 1 4-4h14M7 23l-4-4 4-4M21 13v2a4 4 0 0 1-4 4H3",
  gift: "M20 12v9H4v-9M2 7h20v5H2zM12 22V7M12 7H7.5a2.5 2.5 0 0 1 0-5C11 2 12 7 12 7zM12 7h4.5a2.5 2.5 0 0 0 0-5C13 2 12 7 12 7z",
  check: "M20 6 9 17l-5-5", alert: "M12 9v4M12 17h.01M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z",
  x: "M18 6 6 18M6 6l12 12", up: "M18 15l-6-6-6 6", down: "M6 9l6 6 6-6", send: "M22 2 11 13M22 2l-7 20-4-9-9-4z", plus: "M12 5v14M5 12h14", minus: "M5 12h14",
  bolt: "M13 2 4 14h7l-1 8 9-12h-7z",
  smile: "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20zM8 14s1.5 2 4 2 4-2 4-2M9 9h.01M15 9h.01",
  pencil: "M17 3a2.8 2.8 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5z",
  terminal: "M4 17l6-6-6-6M12 19h8",
  moreV: "M12 5h.01M12 12h.01M12 19h.01",
  puzzle: "M10 4a2 2 0 1 1 4 0v1h4a1 1 0 0 1 1 1v4h1a2 2 0 1 1 0 4h-1v4a1 1 0 0 1-1 1h-4v-1a2 2 0 1 0-4 0v1H6a1 1 0 0 1-1-1v-4h1a2 2 0 1 0 0-4H5V6a1 1 0 0 1 1-1h4z",
  globe: "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20zM2 12h20M12 2a15 15 0 0 1 4 10 15 15 0 0 1-4 10 15 15 0 0 1-4-10 15 15 0 0 1 4-10z",
  folder: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
  trash: "M3 6h18M8 6V4h8v2M19 6l-1 14H6L5 6M10 11v6M14 11v6", download: "M12 3v12M7 10l5 5 5-5M5 21h14", upload: "M12 15V3M7 8l5-5 5 5M5 21h14",
  link: "M10 13a5 5 0 0 0 7.5.5l3-3a5 5 0 0 0-7-7l-1.7 1.7M14 11a5 5 0 0 0-7.5-.5l-3 3a5 5 0 0 0 7 7l1.7-1.7",
  user: "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM19 8v6M22 11h-6",
};
// An account's icon (Lucide shapes), or null when it has none.
function avatarIcon(key) {
  const shapes = typeof AVATAR_ICONS !== "undefined" && AVATAR_ICONS[key];
  if (!shapes) return null;
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("aria-hidden", "true");
  svg.innerHTML = shapes;
  return svg;
}
const iconOf = (name) => ((S && S.settings && S.settings.avatar_icons) || {})[name];
function avatar(a, extra = "") {
  const key = iconOf(a.name);
  const svg = key ? avatarIcon(key) : null;
  const initial = (a.label || "?").replace(/[^\p{L}\p{N}]/gu, "").slice(0, 1).toUpperCase() || "?";
  const style = `--hue:${hueOf(a.name)}`;
  return svg ? el("div", { class: "avatar ico " + extra, style }, svg) : el("div", { class: "avatar " + extra, style, text: initial });
}
// Brand marks, filled (Simple Icons 15.0.0, CC0): Claude for claude.ai, Chrome for Claude in Chrome.
const BRAND = {
  claude: "m4.7144 15.9555 4.7174-2.6471.079-.2307-.079-.1275h-.2307l-.7893-.0486-2.6956-.0729-2.3375-.0971-2.2646-.1214-.5707-.1215-.5343-.7042.0546-.3522.4797-.3218.686.0608 1.5179.1032 2.2767.1578 1.6514.0972 2.4468.255h.3886l.0546-.1579-.1336-.0971-.1032-.0972L6.973 9.8356l-2.55-1.6879-1.3356-.9714-.7225-.4918-.3643-.4614-.1578-1.0078.6557-.7225.8803.0607.2246.0607.8925.686 1.9064 1.4754 2.4893 1.8336.3643.3035.1457-.1032.0182-.0728-.164-.2733-1.3539-2.4467-1.445-2.4893-.6435-1.032-.17-.6194c-.0607-.255-.1032-.4674-.1032-.7285L6.287.1335 6.6997 0l.9957.1336.419.3642.6192 1.4147 1.0018 2.2282 1.5543 3.0296.4553.8985.2429.8318.091.255h.1579v-.1457l.1275-1.706.2368-2.0947.2307-2.6957.0789-.7589.3764-.9107.7468-.4918.5828.2793.4797.686-.0668.4433-.2853 1.8517-.5586 2.9021-.3643 1.9429h.2125l.2429-.2429.9835-1.3053 1.6514-2.0643.7286-.8196.85-.9046.5464-.4311h1.0321l.759 1.1293-.34 1.1657-1.0625 1.3478-.8804 1.1414-1.2628 1.7-.7893 1.36.0729.1093.1882-.0183 2.8535-.607 1.5421-.2794 1.8396-.3157.8318.3886.091.3946-.3278.8075-1.967.4857-2.3072.4614-3.4364.8136-.0425.0304.0486.0607 1.5482.1457.6618.0364h1.621l3.0175.2247.7892.522.4736.6376-.079.4857-1.2142.6193-1.6393-.3886-3.825-.9107-1.3113-.3279h-.1822v.1093l1.0929 1.0686 2.0035 1.8092 2.5075 2.3314.1275.5768-.3218.4554-.34-.0486-2.2039-1.6575-.85-.7468-1.9246-1.621h-.1275v.17l.4432.6496 2.3436 3.5214.1214 1.0807-.17.3521-.6071.2125-.6679-.1214-1.3721-1.9246L14.38 17.959l-1.1414-1.9428-.1397.079-.674 7.2552-.3156.3703-.7286.2793-.6071-.4614-.3218-.7468.3218-1.4753.3886-1.9246.3157-1.53.2853-1.9004.17-.6314-.0121-.0425-.1397.0182-1.4328 1.9672-2.1796 2.9446-1.7243 1.8456-.4128.164-.7164-.3704.0667-.6618.4008-.5889 2.386-3.0357 1.4389-1.882.929-1.0868-.0062-.1579h-.0546l-6.3385 4.1164-1.1293.1457-.4857-.4554.0608-.7467.2307-.2429 1.9064-1.3114Z",
  chrome: "M12 0C8.21 0 4.831 1.757 2.632 4.501l3.953 6.848A5.454 5.454 0 0 1 12 6.545h10.691A12 12 0 0 0 12 0zM1.931 5.47A11.943 11.943 0 0 0 0 12c0 6.012 4.42 10.991 10.189 11.864l3.953-6.847a5.45 5.45 0 0 1-6.865-2.29zm13.342 2.166a5.446 5.446 0 0 1 1.45 7.09l.002.001h-.002l-5.344 9.257c.206.01.413.016.621.016 6.627 0 12-5.373 12-12 0-1.54-.29-3.011-.818-4.364zM12 16.364a4.364 4.364 0 1 1 0-8.728 4.364 4.364 0 0 1 0 8.728Z",
};
function brandIcon(name) {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("aria-hidden", "true");
  svg.setAttribute("class", "brand");
  const p = document.createElementNS("http://www.w3.org/2000/svg", "path");
  p.setAttribute("d", BRAND[name] || "");
  svg.append(p);
  return svg;
}
function icon(name) {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("aria-hidden", "true");
  const p = document.createElementNS("http://www.w3.org/2000/svg", "path");
  p.setAttribute("d", ICON[name] || "");
  svg.append(p);
  return svg;
}

// ---------- helpers ----------
const store = {
  get: (k, d) => { try { return localStorage.getItem("bascule." + k) || d; } catch { return d; } },
  set: (k, v) => { try { localStorage.setItem("bascule." + k, v); } catch { /* per-viewer convenience only */ } },
};
let page = store.get("page", "accounts");
function el(tag, attrs = {}, ...kids) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v === undefined || v === null || v === false) continue;
    if (k === "class") e.className = v;
    else if (k === "text") e.textContent = v;
    else if (k.startsWith("on")) e.addEventListener(k.slice(2), v);
    else e.setAttribute(k, v === true ? "" : v);
  }
  for (const c of kids.flat(Infinity)) if (c !== null && c !== undefined && c !== false) e.append(c.nodeType ? c : document.createTextNode(String(c)));
  return e;
}
const fill = (node, ...kids) => node.replaceChildren(...kids.flat(Infinity).filter((k) => k !== null && k !== undefined && k !== false));
const fmtTime = (ms) => new Date(ms).toLocaleTimeString(lang, { hour: "2-digit", minute: "2-digit" });
// A coming time: the hour today, the weekday and hour within a week, else the date and hour.
const fmtWhen = (when) => {
  const d = new Date(when);
  if (d.toDateString() === new Date().toDateString()) return fmtTime(d);
  if (d - Date.now() < 6 * 86400000) return d.toLocaleString(lang, { weekday: "short", hour: "2-digit", minute: "2-digit" });
  return d.toLocaleString(lang, { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
};
const fmtDate = (iso) => new Date(iso).toLocaleDateString(lang, { day: "numeric", month: "long", year: "numeric" });
const acct = (id) => ((S && S.accounts) || []).find((a) => a.name === id);
const lab = (id) => (id ? (acct(id) || {}).label || id : "");
const sess = (id) => (((S && S.sessions) || []).find((s) => s.session === id) || {}).title || id;
function plan(a) {
  const m = String(a.tier || "").match(/max_(\d+)x/);
  return m ? `Max ${m[1]}x` : a.subscription ? a.subscription[0].toUpperCase() + a.subscription.slice(1) : "";
}
// claude-fable-5-2 → Fable 5.2
function prettyModel(id) {
  const m = String(id || "").match(/^claude-([a-z]+)-(\d+)(?:-(\d+))?/);
  return m ? `${m[1][0].toUpperCase()}${m[1].slice(1)} ${m[2]}${m[3] ? "." + m[3] : ""}` : id || "—";
}
const winName = (w) => (w.startsWith("model:") ? w.slice(6) : tr("w_" + w));
const prettyWindows = (s) => String(s || "").replace(/(model:[^\s,]+|seven_day_opus|seven_day_sonnet|seven_day|five_hour)/g, (m) => winName(m));
function why(reason, manual) {
  if (manual || reason === "manual") return tr("why_manual");
  const r = String(reason || "");
  if (r.startsWith("anticipate:")) return tr("why_anticipate", { w: prettyWindows(r.slice(11).trim()) });
  if (r.startsWith("limit")) return tr("why_limit", { w: prettyWindows(r.replace(/^limit( \(hook\))?:/, "").trim()) });
  if (r.startsWith("wall:")) return tr("why_wall", { w: prettyWindows(r.slice(5).trim()) });
  if (r.startsWith("return")) return tr("why_return");
  return r;
}
const rotationOf = (st) => (st.rotation && st.rotation.length ? st.rotation : S.accounts.map((a) => a.name));
// Accounts in the order Settings → Accounts shows them: the rotation, then those out of it.
function orderedAccounts() {
  const rot = rotationOf(S.settings || {});
  const pos = (n) => { const i = rot.indexOf(n); return i < 0 ? rot.length + S.accounts.findIndex((a) => a.name === n) : i; };
  return [...S.accounts].sort((x, y) => pos(x.name) - pos(y.name));
}
function thresholdFor(name) {
  const k = name.startsWith("model:") || name === "seven_day_opus" || name === "seven_day_sonnet" ? "model" : name;
  return (S.settings.thresholds || {})[k] ?? S.settings.threshold_percent;
}

// ---------- appearance (per viewer) ----------
const THEMES = ["system", "light", "dark"];
let theme = store.get("theme", "system");
const isDark = () => (theme === "dark" ? true : theme === "light" ? false : matchMedia("(prefers-color-scheme: dark)").matches);
function applyTheme() {
  if (theme === "system") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = theme;
}
const ACCENTS = { coral: ["#e0532d", "#ff8a65"], amber: ["#c27a0a", "#f2b44f"], rose: ["#d83c79", "#ff7fae"], violet: ["#7055f0", "#a594ff"],
  blue: ["#3f63f0", "#86a0ff"], teal: ["#0d8c9c", "#43c8d6"], emerald: ["#11905e", "#4fcf93"], graphite: ["#1d1d1f", "#ededeb"] };
let accent = store.get("accent", "coral");
function applyAccent() {
  const [l, d] = ACCENTS[accent] || ACCENTS.coral;
  document.documentElement.style.setProperty("--accent-l", l);
  document.documentElement.style.setProperty("--accent-d", d);
}
// Window buttons, two settings: their order (Windows / Linux: minimise, maximise, close; macOS: close, minimise,
// maximise) and their side. Defaults: Windows / Linux order on the right; on a Mac, macOS order on the left.
const onMac = /Mac/i.test(navigator.platform || navigator.userAgent);
let wcOrder = store.get("wcorder", onMac ? "macos" : "classic");
let wcSide = store.get("wcside", onMac ? "left" : "right");
applyTheme();
applyAccent();

// ---------- the window: own title bar, buttons, resize ----------
const frame = document.getElementById("frame");
// Glyphs drawn on a 12-unit grid, centred, for the 14 px window buttons.
const WGLYPH = { close: "M3.6 3.6l4.8 4.8M8.4 3.6 3.6 8.4", min: "M3.2 6h5.6", max: "M6 3.2v5.6M3.2 6h5.6" };
function wglyph(name) {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 12 12");
  svg.setAttribute("aria-hidden", "true");
  const p = document.createElementNS("http://www.w3.org/2000/svg", "path");
  p.setAttribute("d", WGLYPH[name]);
  svg.append(p);
  return svg;
}
function renderWindowButtons() {
  frame.classList.toggle("wc-right", wcSide === "right");
  frame.classList.toggle("wc-left", wcSide !== "right");
  frame.classList.toggle("wc-classic", wcOrder !== "macos");
  frame.classList.toggle("wc-mac", wcOrder === "macos");
  const btn = (cls, glyph, label, fn) => el("button", { class: cls, type: "button", "aria-label": label, "data-tip": label, onclick: fn }, wglyph(glyph));
  const close = btn("c", "close", tr("win_close"), () => win().close());
  const min = btn("n", "min", tr("win_min"), () => win().minimize());
  const max = btn("x", "max", tr("win_max"), () => win().toggleMaximize());
  fill(document.getElementById("wc"), wcOrder === "macos" ? [close, min, max] : [min, max, close]);
}
async function syncMaximized() {
  try {
    const m = await win().isMaximized();
    frame.classList.toggle("max", m);
    document.body.classList.toggle("max", m);
  } catch { /* outside Tauri */ }
}
let resizeTimer = null;
try { win().onResized(() => { syncMaximized(); moveTabIndicator(false); clearTimeout(resizeTimer); resizeTimer = setTimeout(() => { if (S && page === "accounts") render(true); }, 150); }); } catch { /* outside Tauri */ }
window.addEventListener("resize", () => { clearTimeout(resizeTimer); resizeTimer = setTimeout(() => { if (S && page === "accounts") render(true); }, 150); });
syncMaximized();
window.addEventListener("focus", () => frame.classList.remove("unfocused"));
window.addEventListener("blur", () => frame.classList.add("unfocused"));
document.querySelectorAll("[data-rz]").forEach((h) => h.addEventListener("mousedown", (e) => { e.preventDefault(); win().startResizeDragging(h.dataset.rz).catch(() => {}); }));
renderWindowButtons();

// ---------- components ----------
function toggle({ checked, onChange, label, disabled }) {
  const input = el("input", { type: "checkbox", role: "switch", checked: !!checked, disabled: !!disabled });
  input.addEventListener("change", () => onChange && onChange(input.checked));
  return el("label", { class: "toggle" }, input, el("span", { class: "tg" }), label ? el("span", { text: label }) : null);
}
let openPop = null;
function closePop() { if (openPop) { openPop.remove(); openPop = null; } }
function popover(anchor, content, { align = "end", width } = {}) {
  closePop();
  hideTip();
  const pop = el("div", { class: "popover", role: "menu" }, content);
  if (width) pop.style.minWidth = width + "px";
  document.body.append(pop);
  const r = anchor.getBoundingClientRect();
  const pw = pop.offsetWidth, ph = pop.offsetHeight;
  let left = align === "end" ? r.right - pw : r.left;
  left = Math.max(8, Math.min(left, innerWidth - pw - 8));
  let top = r.bottom + 4;
  if (top + ph > innerHeight - 8) top = Math.max(8, r.top - ph - 4);
  Object.assign(pop.style, { left: left + "px", top: top + "px" });
  openPop = pop;
  const first = pop.querySelector("button");
  if (first) first.focus();
}
function menu(anchor, items) {
  popover(anchor, el("div", {}, items.filter(Boolean).map((it) => (it === "sep" ? el("div", { class: "menu-sep" })
    : el("button", { class: "menu-item" + (it.danger ? " danger" : ""), type: "button", role: "menuitem", onclick: () => { closePop(); it.onClick(); } }, it.icon ? icon(it.icon) : null, el("span", { text: it.label }))))));
}
function select({ value, options, onChange }) {
  const cur = () => options.find((o) => o.value === value) || options[0];
  const btn = el("button", { class: "select-btn", type: "button", "aria-haspopup": "listbox" });
  const paint = () => fill(btn, el("span", { class: "sv" }, el("b", { text: cur().label }), cur().help ? el("small", { text: cur().help }) : null), icon("down"));
  paint();
  btn.addEventListener("click", (ev) => {
    ev.stopPropagation();
    popover(btn, el("div", { role: "listbox" }, options.map((o) => el("button", { class: "opt" + (o.value === value ? " sel" : ""), type: "button", role: "option",
      onclick: () => { value = o.value; paint(); closePop(); onChange(o.value); } }, el("b", { text: o.label }), o.help ? el("small", { text: o.help }) : null))),
    { align: "start", width: btn.offsetWidth });
  });
  return btn;
}
// A segmented control whose selected pill slides, like the top tabs.
function segmented({ value, options, onChange }) {
  const box = el("div", { class: "seg2", role: "radiogroup" });
  const ind = el("span", { class: "seg-ind", "aria-hidden": "true" });
  box.append(ind);
  const btns = options.map((o) => {
    const b = el("button", { type: "button", role: "radio" }, o.label);
    b.addEventListener("click", () => { if (o.value === value) return; value = o.value; place(true); onChange(o.value); });
    box.append(b);
    return [o.value, b];
  });
  let tries = 0;
  function place(animate) {
    for (const [v, b] of btns) { b.classList.toggle("sel", v === value); b.setAttribute("aria-checked", String(v === value)); }
    const cur = (btns.find(([v]) => v === value) || btns[0])[1];
    if (!cur.offsetWidth) { if (tries++ < 20) requestAnimationFrame(() => place(false)); return; }
    box.classList.toggle("still", !animate);
    ind.style.width = cur.offsetWidth + "px";
    ind.style.transform = `translateX(${cur.offsetLeft}px)`;
    if (!animate) requestAnimationFrame(() => requestAnimationFrame(() => box.classList.remove("still")));
  }
  requestAnimationFrame(() => place(false));
  return box;
}
let modals = [];
function modal({ title, body, actions = [], wide, onClose }) {
  closePop();
  const api = { closable: true };
  const close = () => { scrim.remove(); modals = modals.filter((m) => m !== api); if (onClose) onClose(); };
  const bodyBox = el("div", { class: "m-body" }, body);
  const foot = el("div", { class: "m-foot" });
  const setActions = (acts, left) => {
    fill(foot, left ? el("span", { class: "left", text: left }) : null,
      acts.map((a) => el("button", { class: "btn" + (a.primary ? " ink" : "") + (a.danger ? " danger" : ""), type: "button", disabled: !!a.disabled, onclick: () => a.onClick(api) }, a.icon ? icon(a.icon) : null, a.label)));
    foot.hidden = !acts.length && !left;
    if (box) box.classList.toggle("no-foot", foot.hidden);
  };
  let box = null;
  setActions(actions);
  const xBtn = el("button", { class: "icon-btn", type: "button", "aria-label": tr("close"), onclick: () => api.close() }, icon("x"));
  box = el("div", { class: "modal" + (wide ? " wide" : "") + (foot.hidden ? " no-foot" : ""), role: "dialog", "aria-modal": "true" },
    el("div", { class: "m-head" }, el("h3", { text: title }), xBtn),
    bodyBox, foot);
  const scrim = el("div", { class: "scrim", onmousedown: (e) => { if (e.target === scrim && api.closable) api.close(); } }, box);
  document.getElementById("layer").append(scrim);
  Object.assign(api, { close, setBody: (b) => fill(bodyBox, b), setActions, box, setTitle: (t) => { box.querySelector(".m-head h3").textContent = t; },
    setClosable: (v) => { api.closable = v; xBtn.hidden = !v; } });
  modals.push(api);
  setTimeout(() => { const f = box.querySelector("input:not([type=checkbox]), textarea, .m-foot .btn.ink"); if (f) f.focus(); }, 30);
  return api;
}
function toast(text, kind) {
  const t = el("div", { class: "toast" + (kind === "bad" ? " bad" : "") }, icon(kind === "bad" ? "alert" : "check"), el("span", { text }));
  document.getElementById("toasts").append(t);
  setTimeout(() => t.remove(), 3200);
}
async function act(fn, ok) {
  try { const r = await fn(); if (ok) toast(ok); return r; } catch (e) { toast(String(e), "bad"); } finally { refresh(); }
}

// Tooltips: any element with data-tip (and optional data-tip-sub). Measured at the origin.
const tipEl = document.getElementById("tip");
let tipTimer = null;
function hideTip() { clearTimeout(tipTimer); tipEl.hidden = true; }
document.addEventListener("mouseover", (e) => {
  const t = e.target.closest && e.target.closest("[data-tip]");
  if (!t) return;
  clearTimeout(tipTimer);
  tipTimer = setTimeout(() => {
    fill(tipEl, el("span", { text: t.dataset.tip }), t.dataset.tipSub ? el("span", { class: "tip-sub", text: t.dataset.tipSub }) : null);
    Object.assign(tipEl.style, { left: "0px", top: "0px" });
    tipEl.hidden = false;
    const r = t.getBoundingClientRect();
    const w = tipEl.offsetWidth, h = tipEl.offsetHeight;
    const left = Math.max(8, Math.min(r.left + r.width / 2 - w / 2, innerWidth - w - 8));
    let top = r.top - h - 8;
    if (top < 8) top = r.bottom + 8;
    Object.assign(tipEl.style, { left: left + "px", top: top + "px" });
  }, 280);
});
document.addEventListener("mouseout", (e) => { if (e.target.closest && e.target.closest("[data-tip]")) hideTip(); });
document.addEventListener("scroll", hideTip, true);
document.addEventListener("mousedown", (e) => { hideTip(); if (openPop && !openPop.contains(e.target)) closePop(); });
document.addEventListener("keydown", (e) => {
  if (e.key !== "Escape") return;
  if (openPop) closePop();
  else if (modals.length && modals[modals.length - 1].closable) modals[modals.length - 1].close();
});

// ---------- top bar: tabs with a sliding pill ----------
const PAGES = ["accounts", "sessions", "journal"];
const tabsEl = document.getElementById("tabs");
const tabInd = tabsEl.querySelector(".tab-ind");
const tabBtns = Object.fromEntries(PAGES.map((p) => {
  const b = el("button", { class: "tab", type: "button", role: "tab", onclick: () => go(p) });
  tabsEl.append(b);
  return [p, b];
}));
function moveTabIndicator(animate = true) {
  const b = tabBtns[page];
  if (!b || !b.offsetWidth) return;
  tabsEl.classList.toggle("still", !animate);
  tabInd.style.width = b.offsetWidth + "px";
  tabInd.style.transform = `translateX(${b.offsetLeft}px)`;
  if (!animate) requestAnimationFrame(() => requestAnimationFrame(() => tabsEl.classList.remove("still")));
}
let firstBar = true;
function renderBar() {
  const ready = (S.resume || []).filter((e) => e.state === "ready").length;
  const rotNow = rotationOf(S.settings || {});
  const counts = { accounts: [S.accounts.filter((x) => rotNow.includes(x.name) || x.place === "active").length, false], sessions: ready ? [ready, true] : [(S.sessions || []).length, false], journal: null };
  for (const p of PAGES) {
    const c = counts[p];
    fill(tabBtns[p], tr("tab_" + p), c ? el("span", { class: "n" + (c[1] ? " hot" : ""), text: c[0] }) : null);
    tabBtns[p].classList.toggle("sel", p === page);
    tabBtns[p].setAttribute("aria-selected", String(p === page));
  }
  moveTabIndicator(!firstBar);
  firstBar = false;
  const j = S.jaunt === "connected" ? "ok" : S.jaunt === "absent" ? "absent" : S.jaunt === "not_enrolled" ? "enroll" : "down";
  const link = document.getElementById("link");
  const lampClass = { ok: "", absent: " idle", enroll: " warn", down: " off" }[j];
  const linkText = { ok: tr("jaunt_ok"), absent: tr("jaunt_absent"), enroll: tr("jaunt_enroll"), down: tr("jaunt_down") }[j];
  fill(link, el("span", { class: "lamp" + lampClass }), el("span", { class: "lt", text: linkText }));
  link.dataset.tip = { ok: tr("jaunt_tip"), absent: tr("jaunt_absent_tip"), enroll: tr("jaunt_enroll_tip"), down: String(S.jaunt) }[j];
  link.classList.toggle("clickable", j === "enroll");
  link.onclick = j === "enroll" ? openEnroll : null;
  // The browser (Claude Bascule Connector), like jaunt.
  const w = S.web || {};
  const webEl = document.getElementById("web");
  const cicLabel = w.cic_account ? lab(w.cic_account) : tr("unknown");
  const webUp = w.connected || w.cdp === "connected";
  fill(webEl, el("span", { class: "lamp" + (webUp ? "" : " warn") }), el("span", { class: "lt", text: webUp ? tr("web_link") : tr("web_link_off") }));
  webEl.dataset.tip = webUp ? tr("web_link_tip", { w: w.account ? lab(w.account) : tr("web_signed_out_short"), c: cicLabel }) : tr("web_link_off_tip");
  webEl.classList.add("clickable");
  webEl.onclick = () => (webUp ? openSettings("browser") : openBrowserGuide());
  const up = S.update || {};
  const updEl = document.getElementById("upd");
  updEl.hidden = !up.available;
  if (up.available) {
    fill(updEl, el("button", { class: "upd-pill", type: "button", "data-tip": tr("update_tip", { v: up.latest }), onclick: openUpdate }, icon("download"), el("span", { text: tr("update_pill") })));
  }
  const on = !!S.settings.auto_switch;
  const input = el("input", { type: "checkbox", role: "switch", checked: on, "aria-label": on ? tr("auto_on") : tr("auto_off") });
  input.addEventListener("change", () => act(() => call("settings.set", { settings: { auto_switch: input.checked } })));
  fill(document.getElementById("auto"), el("label", { class: "toggle auto", "data-tip": on ? tr("auto_on") : tr("auto_off") }, input, el("span", { class: "tg" }, el("span", { class: "knob" }, icon("bolt")))));
  const sb = document.getElementById("settings-btn");
  sb.dataset.tip = tr("settings");
  sb.setAttribute("aria-label", tr("settings"));
}
document.getElementById("settings-btn").addEventListener("click", () => openSettings());
if (document.fonts) document.fonts.ready.then(() => moveTabIndicator(false));
function go(p) {
  const changed = p !== page;
  page = p;
  store.set("page", p);
  render(true, changed);
}

// ---------- accounts ----------
let showDisabled = store.get("showdisabled", false) === true;
// A setting toggle showing a mark in its knob, explained by its tooltip (no label).
function followToggle(key, mark, tip) {
  const input = el("input", { type: "checkbox", role: "switch", checked: !!S.settings[key], "aria-label": tip });
  input.addEventListener("change", () => { S.settings[key] = input.checked; act(() => call("settings.set", { settings: { [key]: input.checked } })); });
  return el("label", { class: "toggle auto mark-toggle", "data-tip": tip }, input, el("span", { class: "tg" }, el("span", { class: "knob" }, mark)));
}
const HUES = ["#7c5cff", "#0ea5c6", "#e0559b", "#5b7cfa", "#c2410c"];
const hueOf = (name) => HUES[Math.max(0, S.accounts.findIndex((a) => a.name === name)) % HUES.length];
// A bar's colour follows its fill (OKLCH hues): green, yellow, orange, red.
const HUE_STOPS = [[0, 150], [50, 122], [75, 92], [90, 58], [100, 27]];
function levelHue(p) {
  p = Math.max(0, Math.min(100, p));
  for (let i = 1; i < HUE_STOPS.length; i++) {
    const [p1, h1] = HUE_STOPS[i], [p0, h0] = HUE_STOPS[i - 1];
    if (p <= p1) return Math.round(h0 + (h1 - h0) * ((p - p0) / (p1 - p0)));
  }
  return HUE_STOPS[HUE_STOPS.length - 1][1];
}
function meter(name, w, th, offText) {
  if (!w) {
    return el("div", { class: "meter off" }, el("div", { class: "m-top" }, el("span", { class: "m-name", text: name }), el("span", { class: "na", text: offText })), el("div", { class: "track" }));
  }
  const h = levelHue(w.percent);
  return el("div", { class: "meter" },
    el("div", { class: "m-top" }, el("span", { class: "m-name", text: name }),
      el("span", { class: "m-val", style: w.percent >= 70 ? `color:oklch(var(--lvl-tl) var(--lvl-c) ${h})` : null }, String(Math.round(w.percent)), el("small", { text: " %" }))),
    el("div", { class: "track" }, el("div", { class: "fill", style: `width:${Math.min(100, Math.max(w.percent, 1.5))}%;--h:${h}` }),
      el("span", { class: "cut", style: `left:${Math.min(99.5, th)}%`, "data-tip": tr("threshold_tip", { p: th }) })),
    el("div", { class: "mt-foot" }, el("span", { text: w.resets_at ? tr("resets_on", { t: fmtWhen(w.resets_at) }) : "" })));
}
let cardZ = 2;
function renderAccounts(root) {
  const st = S.settings;
  const live = S.accounts.filter((x) => rotationOf(st).includes(x.name) || x.place === "active").length;
  root.append(el("div", { class: "head" },
    el("div", {}, el("h1", { text: tr("tab_accounts") }),
      el("div", { class: "sub", text: st.auto_switch && st.anticipate ? trn("accounts_sub_on", live, { p: st.threshold_percent }) : trn("accounts_sub_off", live) })),
    el("div", { class: "head-actions" },
      webOn() && S.accounts.length ? followToggle("web_follow", brandIcon("claude"), tr("follow_web_tip")) : null,
      webOn() && S.accounts.length ? followToggle("cic_follow", icon("puzzle"), tr("follow_cic_tip")) : null,
      el("button", { class: "icon-btn", type: "button", "aria-label": tr("add_account"), "data-tip": tr("add_account"), onclick: openAddAccount }, icon("plus")),
      S.accounts.length ? el("button", { class: "icon-btn", type: "button", "aria-label": tr("reread"), "data-tip": tr("reread"), onclick: () => act(() => call("reread"), tr("reread_ok")) }, icon("refresh")) : null)));
  if (!S.accounts.length) {
    root.append(el("div", { class: "empty" },
      el("div", { class: "empty-ico" }, icon("user")),
      el("h2", { text: tr("empty_title") }), el("p", { text: tr("empty_body") }),
      el("button", { class: "btn ink", type: "button", onclick: openAddAccount }, icon("plus"), tr("add_account"))));
    return;
  }
  const rot = rotationOf(st);
  // Accounts taken out of rotation (disabled in the settings) stay off the page unless asked.
  const disabled = orderedAccounts().filter((x) => !rot.includes(x.name) && x.place !== "active");
  const shown = orderedAccounts().filter((x) => showDisabled || !disabled.includes(x));
  // One account: a card of normal width rather than one stretched over the window.
  // Cards wrap onto more lines rather than getting too narrow, in balanced rows (4 → 2 × 2).
  const n = Math.max(1, shown.length);
  const width = Math.max(320, (document.getElementById("page").clientWidth || 1000) - 56);
  const fit = Math.max(1, Math.floor((width + 20) / (290 + 20)));
  const rows = Math.ceil(n / Math.min(n, fit));
  const cols = Math.ceil(n / rows);
  const cards = el("div", { class: "cards" + (n === 1 ? " one" : ""), style: n > 1 ? `grid-template-columns: repeat(${cols}, minmax(0, 1fr))` : "" });
  const sessions = (S.sessions || []).length;
  for (const a of shown) {
    const active = a.place === "active";
    const wins = (a.usage && a.usage.windows) || [];
    const find = (n) => wins.find((w) => w.name === n);
    const is20x = /max_20x/.test(a.tier || "");
    // Where each Claude is (Claude Code, claude.ai, Claude in Chrome) shows as icons, top right.
    const badges = a.place === "signed_out" ? [el("span", { class: "badge", text: tr("signed_out") })] : [];
    if (plan(a)) badges.push(el("span", { text: plan(a) }));
    if (a.exhausted && a.exhausted.until_ms > Date.now()) badges.push(el("span", { class: "badge warn", text: tr("until", { t: fmtWhen(a.exhausted.until_ms) }) }));
    if (!rot.includes(a.name)) badges.push(el("span", { text: tr("out_rot") }));
    const more = el("button", { class: "icon-btn more", type: "button", "aria-label": "Menu" }, icon("moreV"));
    more.addEventListener("click", (ev) => {
      ev.stopPropagation();
      const inRot = rot.includes(a.name);
      menu(more, [
        !active && a.place === "resting" ? { icon: "swap", label: tr("switch_here"), onClick: () => confirmSwitch(a) } : null,
        { icon: "refresh", label: tr("reread"), onClick: () => act(() => call("reread"), tr("reread_ok")) },
        { icon: "pencil", label: tr("rename"), onClick: () => openRename(a) },
        { icon: "smile", label: tr("change_icon"), onClick: () => openIconPicker(a) },
        { icon: "download", label: tr("export_account"), onClick: () => exportAccounts([a.name]) },
        webCan(a) ? { icon: "globe", label: tr("web_switch"), onClick: () => act(() => call("web.switch", { to: a.name }), tr("web_switch_sent")) } : null,
        cicCan(a) ? { icon: "puzzle", label: tr("cic_switch"), onClick: () => startCic(a) } : null,
        { icon: "key", label: a.place === "signed_out" ? tr("sign_in") : tr("sign_in_again"), onClick: () => startLogin(a) },
        "sep",
        { icon: "rotate", label: inRot ? tr("from_rot") : tr("to_rot"), onClick: () => {
          let next = inRot ? rotationOf(S.settings).filter((n) => n !== a.name) : [...rotationOf(S.settings), a.name];
          if (next.length === S.accounts.length && next.every((n, i) => n === S.accounts[i].name)) next = [];
          act(() => call("settings.set", { settings: { rotation: next } }), tr("saved"));
        } },
        !active ? { icon: "trash", label: tr("remove_account"), danger: true, onClick: () => confirmRemove(a) } : null,
      ]);
    });
    const tipSub = a.has_alias || a.label === a.email ? plan(a) : a.email;
    const head = el("div", { class: "c-head" },
      avatar(a),
      el("div", { class: "c-id" }, el("div", { class: "c-name ellipsis", text: a.label, "data-tip": a.label, "data-tip-sub": tipSub || null, ondblclick: () => openRename(a) }), el("div", { class: "c-state" }, badges)),
      el("div", { class: "c-top" }, whereIcons(a), more));
    const body = el("div", { class: "c-body" },
      meter(tr("w_five_hour"), find("five_hour"), thresholdFor("five_hour"), is20x ? tr("na") : a.usage ? tr("not_running") : tr("not_read")),
      meter(tr("w_seven_day"), find("seven_day"), thresholdFor("seven_day"), a.usage ? tr("not_running") : tr("not_read")),
      wins.filter((w) => w.name === "seven_day_opus" || w.name === "seven_day_sonnet").map((w) => meter(tr("w_" + w.name), w, thresholdFor(w.name))),
      wins.filter((w) => w.name.startsWith("model:")).map((w) => meter(w.name.slice(6), w, thresholdFor(w.name))));
    const rs = a.usage && a.usage.resets;
    const left = (rs ? (rs.grants || []) : []).reduce((n, g) => n + g.resets_left, 0);
    const resetsBtn = rs ? el("button", { class: "resets" + (left ? " has" : ""), type: "button", onclick: () => openResets(a) }, icon("gift"), left ? trn("resets", left) : tr("resets_none")) : el("span", {});
    const action = active ? el("span", { class: "muted", text: trn("active", sessions) })
      : a.place === "resting" ? el("button", { class: "btn ink", type: "button", onclick: () => confirmSwitch(a) }, icon("swap"), tr("switch_here"))
      : el("button", { class: "btn ink", type: "button", onclick: () => startLogin(a) }, icon("key"), tr("sign_in"));
    const err = a.usage_error && a.place !== "signed_out" ? el("div", { class: "err", text: usageError(a.usage_error), "data-tip": a.usage_error }) : null;
    const card = el("article", { class: "card" + (active ? " active" : ""), style: `--hue:${hueOf(a.name)}` }, head, body, err, el("div", { class: "c-foot" }, resetsBtn, action));
    // The card hovered last is on top; the one left behind finishes shrinking underneath it.
    card.addEventListener("mouseenter", () => { card.style.zIndex = String(++cardZ); });
    cards.append(card);
  }
  root.append(cards);
  if (disabled.length) {
    root.append(el("div", { class: "hidden-note" }, el("span", { class: "muted", text: trn("hidden", disabled.length) }),
      el("button", { class: "btn flat", type: "button", onclick: () => { showDisabled = !showDisabled; store.set("showdisabled", showDisabled); render(true); } }, showDisabled ? tr("hide_hidden") : tr("show_hidden"))));
  }
}
// A quota read failure in plain words; the raw error stays in the tooltip.
function usageError(e) {
  if (/429/.test(e)) return tr("err429");
  if (/HTTP 40[13]|no token/.test(e)) return tr("err_auth");
  return tr("err_network");
}
// The three places an account can be in use, as icons in the card's top right corner: lit where
// it is, grey elsewhere (a click moves that one there at once), absent where it cannot go.
function whereIcons(a) {
  const w = S.web || {};
  const items = [
    { mark: icon("terminal"), on: a.place === "active", can: a.place === "resting",
      onTip: tr("where_cli_on"), offTip: a.place === "signed_out" ? tr("where_cli_signed_out") : tr("where_cli_off"),
      go: () => act(() => call("switch", { to: a.name, web: false, cic: false }), tr("switched", { a: a.label })) },
    webOn() && (webHas(a.name) || w.account === a.name) ? { mark: brandIcon("claude"), on: w.account === a.name, can: webCan(a),
      onTip: tr("where_web_on"), offTip: tr("where_web_off"),
      go: () => act(() => call("web.switch", { to: a.name }), tr("web_switch_sent")) } : null,
    webOn() && (webHas(a.name) || w.cic_account === a.name) ? { mark: icon("puzzle"), on: w.cic_account === a.name, can: cicCan(a),
      onTip: tr("where_cic_on"), offTip: tr("where_cic_off"),
      go: () => startCic(a) } : null,
  ].filter(Boolean);
  return el("div", { class: "where" }, items.map((it) => el("button", {
    class: "where-btn" + (it.on ? " on" : ""), type: "button", disabled: !it.on && !it.can,
    "data-tip": it.on ? it.onTip : it.offTip, "aria-label": it.on ? it.onTip : it.offTip,
    onclick: (e) => { e.stopPropagation(); if (!it.on && it.can) it.go(); } }, it.mark)));
}
// Switching to an account by hand: Claude Code, claude.ai and Claude in Chrome, each on or off.
function confirmSwitch(a) {
  const w = S.web || {};
  const rows = [{ key: "cli", mark: icon("terminal"), label: tr("sw_cli"), here: a.place === "active", can: a.place === "resting" }];
  if (webOn() && webHas(a.name)) {
    rows.push({ key: "web", mark: brandIcon("claude"), label: "claude.ai", here: w.account === a.name, can: webCan(a) });
    rows.push({ key: "cic", mark: icon("puzzle"), label: "Claude in Chrome", here: w.cic_account === a.name, can: cicCan(a) });
  }
  const on = Object.fromEntries(rows.map((r) => [r.key, r.can]));
  const list = el("div", { class: "sw-list" }, rows.map((r) => el("div", { class: "sw-row" + (r.can || r.here ? "" : " off") },
    el("span", { class: "sw-mark" }, r.mark), el("span", { class: "sw-label", text: r.label }),
    r.here ? el("span", { class: "chip live", text: tr("sw_here") })
      : toggle({ checked: r.can, disabled: !r.can, onChange: (v) => { on[r.key] = v; } }))));
  modal({ title: tr("switch_title", { a: a.label }), body: el("div", { class: "form" }, el("p", { text: tr("switch_body") }), list), actions: [
    { label: tr("cancel"), onClick: (m) => m.close() },
    { label: tr("switch_go"), primary: true, icon: "swap", onClick: (m) => {
      m.close();
      if (on.cli) act(() => call("switch", { to: a.name, web: !!on.web, cic: !!on.cic }), tr("switched", { a: a.label }));
      else {
        if (on.web) act(() => call("web.switch", { to: a.name }), tr("web_switch_sent"));
        if (on.cic) startCic(a);
      }
    } },
  ] });
}
// A display name for an account (shown instead of its address); empty = the address.
function openRename(a) {
  const names = (S.settings && S.settings.display_names) || {};
  const input = el("input", { type: "text", value: names[a.name] || "", placeholder: a.email, maxlength: 60, spellcheck: "false" });
  const save = (m) => {
    const next = { ...names };
    const v = input.value.trim();
    if (v) next[a.name] = v; else delete next[a.name];
    m.close();
    // Shown at once; the service saves it meanwhile.
    S.settings.display_names = next;
    for (const x of S.accounts) if (x.name === a.name) { x.label = v || (S.settings.hide_emails ? x.label : x.email); x.has_alias = !!v; }
    lastJson = "";
    render(true);
    act(() => call("settings.set", { settings: { display_names: next } }));
  };
  const m = modal({ title: tr("rename_title"), body: el("div", { class: "form" }, el("p", { class: "muted", text: tr("rename_help", { e: a.email }) }), input), actions: [
    { label: tr("cancel"), onClick: (mm) => mm.close() },
    { label: tr("save"), primary: true, onClick: save },
  ] });
  input.addEventListener("keydown", (e) => { if (e.key === "Enter") save(m); });
  setTimeout(() => input.select(), 40);
}
// Pick an icon for an account (or its initial); saved at once.
function openIconPicker(a, after) {
  const cur = iconOf(a.name) || "";
  const pick = (key, m) => {
    const icons = { ...((S.settings && S.settings.avatar_icons) || {}) };
    if (key) icons[a.name] = key; else delete icons[a.name];
    m.close();
    // Shown at once everywhere (cards, settings); the service saves it meanwhile.
    S.settings.avatar_icons = icons;
    lastJson = "";
    render(true);
    if (after) after();
    act(() => call("settings.set", { settings: { avatar_icons: icons } }));
  };
  const grid = el("div", { class: "icon-grid", role: "radiogroup" });
  const m = modal({ title: tr("icon_title", { a: a.label }), body: grid, actions: [] });
  const initial = (a.label || "?").replace(/[^\p{L}\p{N}]/gu, "").slice(0, 1).toUpperCase() || "?";
  grid.append(el("button", { class: "icon-tile letter" + (!cur ? " sel" : ""), type: "button", role: "radio", "aria-checked": String(!cur), "data-tip": tr("icon_initial"), onclick: () => pick("", m) }, initial));
  for (const key of Object.keys(AVATAR_ICONS)) {
    grid.append(el("button", { class: "icon-tile" + (key === cur ? " sel" : ""), type: "button", role: "radio", "aria-checked": String(key === cur), "aria-label": key, onclick: () => pick(key, m) }, avatarIcon(key)));
  }
}
// claude.ai in the browser (the Connector extension)
const webHas = (name) => ((S.web && S.web.sessions) || []).includes(name);
// The browser is reachable through the extension or through Chrome control (remote debugging).
const webOn = () => !!(S.web && (S.web.connected || S.web.cdp === "connected" || S.web.cdp === "available"));
const webCan = (a) => !!(webOn() && webHas(a.name) && S.web.account !== a.name);
const cicMoving = (f) => !!f && ["waiting", "working", "sign_in"].includes(f.stage);
const cicCan = (a) => !!(webOn() && webHas(a.name) && S.web.cic_account !== a.name && !cicMoving(S.web.cic_flow));
const OS_TIP = { linux: "guide_tip_linux", macos: "guide_tip_macos", windows: "guide_tip_windows" };

// Install guide for the browser extension: opens the browser's extensions page, says what to do on
// this system, and turns into "connected" by itself.
let guideModal = null, guideStatus = null;
async function openBrowserGuide() {
  const w = S.web || {};
  let list = [];
  try { list = await invoke("browsers"); } catch (e) { list = []; }
  BROWSERS = list;
  const copy = (text, said) => {
    const done = () => { toast(tr(said || "guide_copied")); };
    if (navigator.clipboard && navigator.clipboard.writeText) navigator.clipboard.writeText(text).then(done, () => fallback());
    else fallback();
    function fallback() { const t = el("textarea", { style: "position:fixed;opacity:0" }); t.value = text; document.body.append(t); t.select(); try { document.execCommand("copy"); done(); } catch (e) {} t.remove(); }
  };
  // The browser chosen in the settings, else the first found (Chrome first); picking another one
  // here saves it, and shows the folder that browser loads (a Flatpak browser has its own copy).
  const want = (S.settings && S.settings.browser) || "";
  let pick = list.find((b) => b.id === want) || list.find((b) => want && b.id.split("-")[0] === want.split("-")[0]) || list[0] || null;
  const steps = el("div", { class: "steps web-steps" });
  const choose = (b) => { pick = b; paint(); call("settings.set", { settings: { browser: b.id } }).catch((e) => toast(String(e), "bad")); };
  const paint = () => {
    const path = pick ? pick.extension_dir : w.extension_dir;
    fill(steps,
      el("div", { class: "step" }, el("span", { class: "sn", text: "1" }), el("div", {}, el("div", { class: "st", text: list.length ? tr("guide_s1") : tr("guide_s1_none") }),
        list.length > 1 ? el("div", { class: "path-row" }, list.map((b) => el("button", { class: "btn" + (b === pick ? " ink" : " flat"), type: "button", "aria-pressed": b === pick ? "true" : "false", onclick: () => choose(b) }, b.name))) : null,
        // Chrome opens a blank tab for chrome://extensions given on its command line: the address
        // is copied, to paste into the tab the button opens.
        pick ? el("div", { class: "path-row" }, el("code", { class: "path", text: pick.page }),
          el("button", { class: "btn" + (list.length > 1 ? "" : " ink"), type: "button", onclick: () => { copy(pick.page, "guide_addr_copied"); invoke("open_extensions_page", { id: pick.id }).catch((e) => toast(String(e), "bad")); } }, icon("globe"), tr("guide_open", { b: pick.name }))) : null)),
      el("div", { class: "step" }, el("span", { class: "sn", text: "2" }), el("div", { class: "st", text: tr("guide_s2") })),
      el("div", { class: "step" }, el("span", { class: "sn", text: "3" }), el("div", {}, el("div", { class: "st", text: tr("guide_s3") }),
        path ? el("div", { class: "path-row" }, el("code", { class: "path", text: path }),
          el("button", { class: "btn", type: "button", onclick: () => copy(path) }, icon("check"), tr("guide_copy")),
          el("button", { class: "btn flat", type: "button", onclick: () => invoke("open_folder", { path }).catch((e) => toast(String(e), "bad")) }, icon("folder"), tr("web_open_folder"))) : el("div", { class: "muted", text: tr("web_no_folder") }),
        el("div", { class: "muted small", text: tr(OS_TIP[S.platform] || "guide_tip_linux") }))));
  };
  paint();
  guideStatus = el("div", { class: "guide-status" });
  const body = el("div", { class: "guide" },
    el("p", { class: "muted", text: tr("guide_intro") }),
    steps,
    el("p", { class: "muted small" }, tr("guide_alt"), " ", el("a", { href: "#", onclick: (e) => { e.preventDefault(); if (guideModal) guideModal.close(); openSettings("browser", ["cic_follow"]); } }, tr("cdp_title"))),
    guideStatus);
  guideModal = modal({ title: tr("guide_title"), body, wide: true, actions: [{ label: tr("close"), onClick: (m) => m.close() }], onClose: () => { guideModal = null; guideStatus = null; } });
  paintGuideStatus();
}
function paintGuideStatus() {
  if (!guideStatus) return;
  const w = S.web || {};
  fill(guideStatus, w.connected
    ? el("div", { class: "ok-line" }, icon("check"), tr("guide_ok", { b: browserName(w.browser) }))
    : el("div", { class: "wait" }, el("span", { class: "spinner" }), tr("guide_wait")));
}

// Moving Claude in Chrome: Claude Bascule prepares claude.ai, the user signs the extension out and
// in again, Claude Bascule notices and puts claude.ai back.
let cicModal = null, cicShown = 0, cicSignShown = 0;
function startCic(a) {
  act(() => call("web.cic_begin", { to: a.name }));
}
function cicView(f) {
  if (f.stage === "done") return el("div", { class: "ok-line" }, icon("check"), tr("cic_ok", { a: lab(f.to) }));
  if (f.stage === "failed") return el("p", { text: tr("cic_fail", { e: f.error || "?" }) });
  if (f.stage === "sign_in") return el("div", { class: "upd" }, el("p", { text: tr("cic_sign_in", { a: lab(f.to) }) }),
    el("div", { class: "wait" }, el("span", { class: "spinner" }), tr("cic_wait")));
  if (f.mode === "auto") return el("div", { class: "upd" }, el("div", { class: "wait" }, el("span", { class: "spinner" }), tr("cic_auto", { a: lab(f.to) })),
    el("div", { class: "progress indeterminate" }, el("span")), el("p", { class: "muted small", text: tr("cic_auto_note") }));
  return el("div", { class: "steps web-steps" },
    el("div", { class: "step" }, el("span", { class: "sn done" }, icon("check")), el("div", { class: "st", text: tr("cic_s1", { a: lab(f.to) }) })),
    el("div", { class: "step" }, el("span", { class: "sn", text: "1" }), el("div", { class: "st", text: tr("cic_s2") })),
    el("div", { class: "step" }, el("span", { class: "sn", text: "2" }), el("div", { class: "st", text: tr("cic_s3") })),
    f.restore ? el("div", { class: "step" }, el("span", { class: "sn", text: "3" }), el("div", { class: "st", text: tr("cic_s4", { b: lab(f.restore) }) })) : null,
    el("div", { class: "wait" }, el("span", { class: "spinner" }), tr("cic_wait")));
}
function syncCic() {
  const f = S.web && S.web.cic_flow;
  if (!f) { if (cicModal) { cicModal.close(); cicModal = null; } return; }
  // Shown when a move starts, and again when it needs the user to sign in to claude.ai.
  if (!cicModal && cicMoving(f) && (cicShown !== f.id || (f.stage === "sign_in" && cicSignShown !== f.id))) {
    cicShown = f.id;
    if (f.stage === "sign_in") cicSignShown = f.id;
    cicModal = modal({ title: tr("cic_title", { a: lab(f.to) }), body: cicView(f), actions: [], onClose: () => { cicModal = null; } });
    const wd = win(); wd.unminimize().catch(() => {}); wd.show().catch(() => {}); wd.setFocus().catch(() => {});
  }
  if (!cicModal) return;
  cicModal.setBody(cicView(f));
  cicModal.setActions(f.stage === "waiting"
    ? [{ label: tr("cancel"), onClick: () => act(() => call("web.cic_cancel")) }, { label: tr("cic_done_btn"), primary: true, onClick: () => act(() => call("web.cic_done", { id: f.id, ok: true })) }]
    : f.stage === "working" || f.stage === "sign_in" ? [{ label: tr("close"), onClick: (m) => m.close() }]
    : f.stage === "failed" ? [{ label: tr("close"), onClick: (m) => m.close() }, { label: tr("cic_retry"), primary: true, onClick: (m) => { m.close(); act(() => call("web.cic_begin", { to: f.to })); } }]
    : [{ label: tr("close"), primary: true, onClick: (m) => m.close() }]);
  // A move Claude Bascule did on its own closes by itself once done.
  if (f.stage === "done" && f.mode === "auto" && !cicModal._closing) { cicModal._closing = true; const m = cicModal; setTimeout(() => m.close(), 2500); }
}
function browserName(ua) {
  ua = ua || "";
  return /Edg\//.test(ua) ? "Edge" : /OPR\//.test(ua) ? "Opera" : /Vivaldi/.test(ua) ? "Vivaldi" : /Chrome\//.test(ua) ? "Chrome" : tr("web_browser");
}
function confirmRemove(a) {
  modal({ title: tr("remove_title", { a: a.label }), body: el("p", { text: tr("remove_body") }), actions: [
    { label: tr("cancel"), onClick: (m) => m.close() },
    { label: tr("remove_go"), primary: true, danger: true, icon: "trash", onClick: (m) => { m.close(); act(() => call("accounts.remove", { name: a.name }), tr("removed")); } },
  ] });
}
// ---------- backups ----------
// A JSON file with accounts, their name, icon, rotation and claude.ai session (never Claude
// Code's tokens: each computer keeps its own sign-in). Written by the service in Downloads.
async function exportAccounts(names) {
  let r;
  try { r = await call("accounts.export", names.length ? { names } : {}); } catch (e) { toast(String(e), "bad"); return; }
  const notes = [el("p", { class: "muted small", text: tr(r.accounts === 1 ? "export_warn_one" : "export_warn_other") }), el("p", { class: "muted small", text: tr("export_cli") })];
  if (r.with_web < r.accounts) notes.unshift(el("p", { class: "muted small", text: tr(r.accounts === 1 ? "export_no_web_one" : "export_no_web_other") }));
  modal({ title: tr("export_title"), body: el("div", { class: "form" },
    el("p", { text: trn("export_done", r.accounts) }), el("div", { class: "path", text: r.path }), ...notes),
  actions: [
    { label: tr("close"), onClick: (m) => m.close() },
    { label: tr("show_folder"), primary: true, icon: "folder", onClick: (m) => { invoke("open_folder", { path: r.dir }).catch((e) => toast(String(e), "bad")); m.close(); } },
  ] });
}
function importBackup() {
  const input = el("input", { type: "file", accept: ".json,application/json", style: "display:none" });
  input.addEventListener("change", async () => {
    const f = input.files && input.files[0];
    input.remove();
    if (!f) return;
    let r;
    try {
      r = await call("accounts.import", { data: await f.text() });
    } catch (e) {
      const msg = String(e);
      toast(/not a Claude Bascule backup/.test(msg) ? tr("import_bad") : /newer Claude Bascule/.test(msg) ? tr("import_newer") : msg, "bad");
      return;
    }
    // The service lists new accounts (and their restored sign-ins) at its next turn.
    const ready = () => r.accounts.every((x) => acct(x.name) && (!x.cli.startsWith("restored") || acct(x.name).place !== "signed_out"));
    for (let i = 0; i < 20 && !ready(); i++) { await refresh(); await new Promise((ok) => setTimeout(ok, 250)); }
    showImport(r);
  }, { once: true });
  document.body.append(input);
  input.click();
}
function showImport(r) {
  let m = null;
  const rows = r.accounts.map((x) => {
    const a = acct(x.name) || { name: x.name, label: x.name };
    // A restored sign-in may take a moment to show on the account: no sign-in button for it.
    const toSign = a.place === "signed_out" && !x.cli.startsWith("restored");
    return el("div", { class: "acct-item imp-item" }, avatar(a, "sm"),
      el("div", { class: "imp-text" }, el("span", { class: "em ellipsis", text: a.label, "data-tip": a.email || "" }),
        el("span", { class: "muted small", text: tr("imp_cli_" + x.cli) }), el("span", { class: "muted small", text: tr("imp_web_" + x.web) })),
      el("span", { class: "chip" + (x.status === "added" ? " live" : ""), text: tr("import_" + x.status) }),
      toSign ? el("button", { class: "btn ink", type: "button", onclick: () => { m.close(); startLogin(a); } }, icon("key"), tr("sign_in")) : el("span"));
  });
  const toSign = r.accounts.some((x) => (acct(x.name) || {}).place === "signed_out" && !x.cli.startsWith("restored"));
  const body = rows.length
    ? el("div", { class: "form" }, el("div", { class: "acct-list" }, rows), toSign ? el("p", { class: "muted small", text: tr("import_login_note") }) : null)
    : el("p", { text: tr("import_none") });
  m = modal({ title: tr("import_title"), body, actions: [{ label: tr("close"), primary: true, onClick: (mm) => mm.close() }] });
}
function openAddAccount() {
  const input = el("input", { type: "email", placeholder: tr("add_ph"), autocomplete: "off", spellcheck: "false" });
  const go = async (m) => {
    const email = input.value.trim();
    if (!/^[^\s@]+@[^\s@]+$/.test(email)) { input.focus(); input.classList.add("bad"); return; }
    try {
      const r = await call("accounts.add", { email });
      m.close();
      await refresh();
      startLogin({ name: r.name, label: email });
    } catch (e) { toast(String(e), "bad"); }
  };
  input.addEventListener("input", () => input.classList.remove("bad"));
  const m = modal({ title: tr("add_title"), body: el("div", { class: "form" }, el("p", { class: "muted", text: tr("add_body") }), input,
    el("button", { class: "btn flat add-import", type: "button", onclick: () => { m.close(); importBackup(); } }, icon("upload"), tr("import_backup"))), actions: [
    { label: tr("cancel"), onClick: (mm) => mm.close() },
    { label: tr("add_go"), primary: true, onClick: go },
  ] });
  input.addEventListener("keydown", (e) => { if (e.key === "Enter") go(m); });
}
let enrolling = false;
function openEnroll() {
  const m = modal({ title: tr("enroll_title"), body: el("p", { text: tr("enroll_body") }), actions: [
    { label: tr("cancel"), onClick: (mm) => mm.close() },
    { label: tr("enroll_go"), primary: true, icon: "link", onClick: async (mm) => {
      if (enrolling) return;
      enrolling = true;
      mm.setBody(el("div", { class: "wait" }, el("span", { class: "spinner" }), tr("enroll_wait")));
      mm.setActions([{ label: tr("cancel"), onClick: (x) => x.close() }]);
      try { await call("jaunt.enroll"); mm.close(); toast(tr("enroll_ok")); }
      catch (e) { mm.setBody(el("p", { text: String(e) })); mm.setActions([{ label: tr("close"), onClick: (x) => x.close() }]); }
      finally { enrolling = false; refresh(); }
    } },
  ] });
  return m;
}
function notesView(md) {
  const box = el("div", { class: "notes" });
  let list = null;
  for (const raw of String(md || "").split(/\r?\n/)) {
    const line = raw.replace(/\*\*(.+?)\*\*/g, "$1").replace(/\[([^\]]+)\]\([^)]+\)/g, "$1").replace(/`([^`]+)`/g, "$1").trim();
    if (!line) { list = null; continue; }
    if (/^Install or update/.test(line)) continue;
    if (/^#\s/.test(line)) { list = null; box.append(el("div", { class: "notes-ver", text: line.replace(/^#\s*/, "") })); continue; }
    if (/^#{2,6}\s/.test(line)) {
      list = null;
      const h = line.replace(/^#+\s*/, "");
      const k = { "New": "notes_new", "Fixes": "notes_fixes", "Faster": "notes_faster", "Breaking changes": "notes_breaking", "Other changes": "notes_other" }[h];
      box.append(el("h5", { text: k ? tr(k) : h }));
      continue;
    }
    if (/^[-*]\s/.test(line)) { if (!list) { list = el("ul"); box.append(list); } list.append(el("li", { text: line.slice(2) })); continue; }
    list = null;
    box.append(el("p", { text: line }));
  }
  return box;
}
// The update: what is new, then an installing state the window cannot leave (the installer
// closes and reopens the app); if the app is still here after two minutes, it did not work.
function updHero(title, sub) {
  return el("div", { class: "upd-hero" }, el("div", { class: "mark big", "aria-hidden": "true" }, el("span", { class: "glyph" })),
    el("div", { class: "upd-hero-t" }, el("div", { class: "upd-v", text: title }), el("div", { class: "muted", text: sub })));
}
function openUpdate() {
  // What the service knew may be an hour old: look again before offering.
  const m0 = modal({ title: tr("update_avail_title"), body: el("div", { class: "wait" }, el("span", { class: "spinner" }), tr("update_checking")), actions: [] });
  call("update.check").then((fresh) => {
    if (!modals.includes(m0)) return;
    m0.close();
    if (fresh) S.update = fresh;
    if (fresh && !fresh.available && !fresh.error) {
      modal({ title: tr("update_none_title"), body: el("p", { text: tr("update_none", { v: fresh.current }) }), actions: [{ label: tr("close"), primary: true, onClick: (m) => m.close() }] });
      return;
    }
    showUpdate();
  }).catch(() => { if (modals.includes(m0)) { m0.close(); showUpdate(); } });
}
function showUpdate() {
  const up = S.update || {};
  const ask = () => [
    { label: tr("update_later"), onClick: (m) => m.close() },
    { label: tr("update_go"), primary: true, icon: "download", onClick: install },
  ];
  const offer = () => el("div", { class: "upd" },
    updHero(`Claude Bascule ${up.latest}`, tr("update_body", { c: up.current })),
    up.notes ? el("div", { class: "upd-notes" }, el("div", { class: "upd-h", text: tr("update_notes") }), notesView(up.notes)) : null);
  async function install(m) {
    m.setTitle(tr("update_installing_title"));
    m.setClosable(false);
    m.setActions([]);
    m.setBody(el("div", { class: "upd" },
      updHero(tr("update_installing", { v: up.latest }), tr("update_restart_note")),
      el("div", { class: "progress indeterminate", role: "progressbar", "aria-label": tr("update_installing_title") }, el("span"))));
    const failed = (why) => {
      m.setTitle(tr("update_avail_title"));
      m.setClosable(true);
      m.setBody(el("div", { class: "upd" }, updHero(`Claude Bascule ${up.latest}`, tr("update_failed")), why ? el("p", { class: "muted small", text: why }) : null));
      m.setActions(ask());
    };
    try { await invoke("apply_update"); } catch (e) { failed(String(e)); return; }
    setTimeout(() => { if (modals.includes(m)) failed(""); }, 120000);
  }
  modal({ title: tr("update_avail_title"), body: offer(), actions: ask() });
}
function openResets(a) {
  const rs = a.usage.resets;
  const clearName = (c) => ({ five_hour: tr("clear_five_hour"), seven_day: tr("clear_seven_day"), seven_day_overage_included: tr("clear_model_week") })[c] || winName(c);
  const list = el("div", { class: "grants" });
  for (const g of [...(rs.grants || [])].sort((x, y) => y.resets_left - x.resets_left)) {
    const used = g.resets_left === 0;
    const when = used ? tr("r_used") : g.paused ? tr("r_paused") : g.usable_now ? tr("r_now") : g.use_requires_limit ? tr("r_at_limit") : tr("r_later");
    list.append(el("div", { class: "grant" + (g.usable_now && !used ? " now" : "") + (used ? " used" : "") },
      el("span", { class: "gn", text: `${g.resets_left}/${g.resets_total}` }),
      el("div", {}, el("div", { class: "gl", text: g.label }),
        el("div", { class: "gd", text: [when, g.ends_at ? tr("r_until", { d: fmtDate(g.ends_at) }) : null, g.clears && g.clears.length ? tr("r_clears", { c: g.clears.map(clearName).join(", ") }) : null].filter(Boolean).join(" · ") }))));
  }
  modal({ title: tr("resets_title", { a: a.label }), body: el("div", { style: "display:grid;gap:16px" }, list, el("p", { class: "muted", text: tr("r_note") })), actions: [{ label: tr("close"), onClick: (m) => m.close() }] });
}

// ---------- sign-in ----------
let loginModal = null, loginFor = null, loginKey = "";
function startLogin(a) {
  loginFor = a.name;
  loginKey = "";
  act(() => call("login.start", { account: a.name }));
  loginModal = modal({ title: tr("login_title", { a: a.label }), body: el("div", { class: "wait" }, el("span", { class: "spinner" }), tr("waiting")),
    onClose: () => { if (S && S.login && S.login.status === "running") call("login.cancel").catch(() => {}); loginModal = null; } });
}
function renderLogin() {
  if (!loginModal || !S.login || S.login.account !== loginFor) return;
  const lg = S.login;
  const key = `${lg.status}|${lg.url}|${lg.message}|${lang}`;
  if (key === loginKey) return;
  loginKey = key;
  if (lg.status === "running") {
    const code = el("input", { type: "text", autocomplete: "off", spellcheck: "false" });
    loginModal.setBody(el("div", { class: "steps" },
      el("div", { class: "step" }, el("span", { class: "sn", text: "1" }), el("div", {}, el("div", { class: "st", text: tr("step1") }),
        el("button", { class: "btn ink", type: "button", disabled: !lg.url, onclick: () => invoke("open_url", { url: lg.url }) }, tr("open_page")))),
      el("div", { class: "step" }, el("span", { class: "sn", text: "2" }), el("div", {}, el("div", { class: "st", text: tr("step2") }),
        el("div", { class: "composer" }, code, el("button", { class: "btn", type: "button", onclick: () => act(() => call("login.code", { code: code.value })) }, tr("validate"))))),
      el("div", { class: "wait" }, el("span", { class: "spinner" }), tr("waiting"))));
    loginModal.setActions([{ label: tr("cancel"), onClick: (m) => m.close() }]);
  } else if (lg.status === "done") {
    loginModal.setBody(el("p", { text: tr("login_done", { a: lab(lg.account) }) }));
    loginModal.setActions([{ label: tr("close"), primary: true, onClick: (m) => m.close() }]);
  } else if (lg.status === "error") {
    const a = acct(lg.account);
    loginModal.setBody(el("p", { text: `${tr("login_fail")} ${lg.message || ""}` }));
    loginModal.setActions([{ label: tr("close"), onClick: (m) => m.close() }, { label: tr("retry"), primary: true, onClick: (m) => { m.close(); if (a) startLogin(a); } }]);
  }
}

// ---------- sessions ----------
const drafts = {};
const sendOne = (id, text) => act(() => call("resume.send", { sessions: [id], texts: { [id]: text } }));
function renderSessions(root) {
  const entries = S.resume || [];
  const sendable = entries.filter((e) => ["ready", "needs_you", "failed"].includes(e.state));
  const n = (S.sessions || []).length;
  root.append(el("div", { class: "head" },
    el("div", {}, el("h1", { text: tr("tab_sessions") }), el("div", { class: "sub", text: entries.length ? trn("sessions_sub_r", n, { r: entries.length }) : trn("sessions_sub", n) })),
    sendable.length >= 2 ? el("button", { class: "btn ink", type: "button", onclick: () => act(() => call("resume.send", { sessions: sendable.map((e) => e.session),
      texts: Object.fromEntries(sendable.map((e) => [e.session, drafts[e.session] ?? S.default_message])) })) }, icon("send"), trn("send_all", sendable.length)) : null));
  root.append(el("h2", { class: "sec", text: tr("to_resume") }));
  if (!entries.length) root.append(el("div", { class: "calm" }, el("span", { class: "ic" }, icon("check")), tr("no_resume")));
  else {
    const list = el("div", { class: "stack" });
    for (const e of entries) {
      const canSend = ["ready", "needs_you", "failed"].includes(e.state);
      const cls = { resumed: "go", ready: "caution", failed: "stop", needs_you: "stop" }[e.state] || "";
      const more = el("button", { class: "icon-btn", type: "button", "aria-label": "Menu" }, icon("more"));
      more.addEventListener("click", (ev) => { ev.stopPropagation(); menu(more, [{ icon: "x", label: tr("dismiss"), onClick: () => act(() => call("resume.dismiss", { sessions: [e.session] })) }]); });
      const input = el("input", { type: "text", value: drafts[e.session] ?? e.text ?? S.default_message, "aria-label": tr("send") });
      input.addEventListener("input", () => { drafts[e.session] = input.value; });
      input.addEventListener("keydown", (k) => { if (k.key === "Enter" && canSend) sendOne(e.session, input.value); });
      list.append(el("div", { class: "tile" },
        el("div", { style: "min-width:0;display:grid;gap:4px" },
          el("div", { class: "t" }, el("span", { class: "ellipsis", text: e.title || e.session, "data-tip": e.title || e.session, "data-tip-sub": e.cwd || null }), el("span", { class: "state " + cls, text: tr("st_" + e.state) })),
          el("div", { class: "s ellipsis", text: [tr("blocked", { t: fmtTime(e.blocked_at_ms) }), e.note].filter(Boolean).join(" · ") })),
        more,
        canSend ? el("div", { class: "composer" }, input, el("button", { class: "btn ink", type: "button", onclick: () => sendOne(e.session, input.value) }, icon("send"), tr("send"))) : null));
    }
    root.append(list);
  }
  root.append(el("h2", { class: "sec", text: tr("all_sessions") }));
  const ss = S.sessions || [];
  if (!ss.length) { root.append(el("div", { class: "calm", text: tr("no_sessions") })); return; }
  const panel = el("div", { class: "panel" }, el("div", { class: "prow h" }, el("span", { text: tr("col_session") }), el("span", { class: "md", text: tr("col_model") }),
    el("span", { text: tr("col_state") }), el("span", { class: "lt", text: tr("col_turn") }), el("span", {})));
  for (const s of ss) {
    const lt = s.lastTurn || {};
    const more = el("button", { class: "icon-btn", type: "button", "aria-label": "Menu" }, icon("more"));
    more.addEventListener("click", (ev) => { ev.stopPropagation(); menu(more, [{ icon: "plus", label: tr("add_resume"), onClick: () => act(() => call("resume.add", { sessions: [s.session] })) }]); });
    const turn = lt.reason ? tr("lt_" + lt.reason) + (lt.limit_kind ? ` (${lt.limit_kind})` : "") + (lt.ended_at ? ` · ${fmtTime(lt.ended_at)}` : "") : "—";
    panel.append(el("div", { class: "prow" },
      el("span", { class: "ellipsis", text: s.title || s.session, "data-tip": s.title || s.session, "data-tip-sub": s.cwd || null }),
      el("span", { class: "md muted", text: s.model ? prettyModel(s.model) : "—", "data-tip": s.model || null }),
      el("span", {}, el("span", { class: "state " + (s.status === "busy" ? "go" : ""), text: tr("status_" + (s.status || "unknown")) })),
      el("span", { class: "lt ellipsis" + (lt.reason === "limit" ? " stop" : ""), text: turn }), more));
  }
  root.append(panel);
}

// ---------- journal ----------
let filter = "all";
const KIND = { switch: "sw", "session.limit": "lim", "resume.resumed": "ok", "resume.failed": "lim", "resume.needs_you": "lim", all_exhausted: "lim",
  "anticipate.no_room": "warn", "anticipate.model_idle": "warn", "usage.rate_limited": "warn", "cic.switched": "sw", "cic.failed": "lim", "web.switched": "sw", "web.switch_failed": "lim", "web.no_session": "warn" };
const group = (k) => (k.startsWith("switch") ? "switch" : k.includes("limit") || k === "all_exhausted" || k.startsWith("anticipate.") ? "limit" : k.startsWith("resume") ? "resume" : "system");
function evText(e) {
  const d = e.data || {};
  switch (e.kind) {
    case "switch": return tr("ev_switch", { from: lab(d.from) || "?", to: lab(d.to), why: why(d.reason, d.manual) });
    case "session.limit": return tr("ev_limit", { s: (d.title || d.session) + (d.lastTurn && d.lastTurn.limit_kind ? ` (${d.lastTurn.limit_kind})` : "") });
    case "resume.sent": return tr("ev_resume_sent", { s: sess(d.session) });
    case "resume.resumed": return tr("ev_resume_resumed", { s: sess(d.session) });
    case "resume.failed": return tr("ev_resume_failed", { s: sess(d.session) });
    case "resume.needs_you": return tr("ev_resume_needs_you", { s: sess(d.session), d: d.dialog || "" });
    case "all_exhausted": return tr("ev_all_exhausted");
    case "anticipate.no_room": return tr("ev_no_room", { w: prettyWindows(d.over) });
    case "anticipate.model_idle": return tr("ev_model_idle", { w: prettyWindows(d.over) });
    case "login": return tr("ev_login", { a: lab(d.account || d.name) });
    case "login.adopted": return tr("ev_adopted", { a: lab(d.account) });
    case "settings.reloaded": return tr("ev_settings");
    case "daemon.start": return tr("ev_start");
    case "jaunt.down": return tr("ev_jaunt_down");
    case "jaunt.lost": return tr("ev_jaunt_lost");
    case "switch.failed": return tr("ev_switch_failed", { a: lab(d.to) });
    case "web.account": return tr("ev_web_account", { a: lab(d.account) });
    case "web.switched": return tr("ev_web_switched", { a: lab(d.account) });
    case "web.switch_failed": return tr("ev_web_switch_failed", { a: lab(d.account) }) + (d.error ? ` · ${d.error}` : "");
    case "cic.switched": return tr("ev_cic_switched", { a: lab(d.account) });
    case "cic.failed": return tr("ev_cic_failed", { a: lab(d.account) }) + (d.error ? ` · ${d.error}` : "");
    case "web.no_session": return tr("ev_web_no_session", { a: lab(d.account) });
    case "web.session_expired": return tr("ev_web_session_expired", { a: lab(d.account) });
    case "cic.sign_in": return tr("ev_cic_sign_in", { a: lab(d.account) });
    case "usage.rate_limited": return tr("ev_rate_limited", { a: lab(d.account), m: Math.max(1, Math.round((d.retry_after_s || 600) / 60)) });
    default: return null;
  }
}
function renderJournal(root) {
  const listBox = el("div", {});
  const paintList = (animate) => {
    const evs = (S.events || []).filter((e) => evText(e) && (filter === "all" || group(e.kind) === filter)).sort((x, y) => y.at - x.at);
    listBox.className = animate ? "fade-in" : "";
    if (!evs.length) { fill(listBox, el("div", { class: "calm", text: tr("no_events") })); return; }
    const panel = el("div", { class: "panel" });
    let day = "";
    for (const e of evs) {
      const d = new Date(e.at);
      const dl = d.toDateString() === new Date().toDateString() ? tr("today") : d.toLocaleDateString(lang, { weekday: "long", day: "numeric", month: "long" });
      if (dl !== day) { day = dl; panel.append(el("div", { class: "day", text: dl })); }
      panel.append(el("div", { class: "ev" }, el("time", { text: fmtTime(e.at) }), el("span", { class: "k " + (KIND[e.kind] || "") }), el("span", { text: evText(e) })));
    }
    fill(listBox, panel);
  };
  // Changing the filter repaints the list only, so the control keeps its sliding pill.
  root.append(el("div", { class: "head" }, el("h1", { text: tr("tab_journal") }),
    segmented({ value: filter, options: ["all", "switch", "limit", "resume", "system"].map((k) => ({ value: k, label: tr("f_" + k) })), onChange: (v) => { filter = v; paintList(true); } })));
  paintList(false);
  // Refused quota reads that keep coming back: suggest reading less often.
  const since = Date.now() - 6 * 3600000;
  const refused = (S.events || []).filter((e) => e.kind === "usage.rate_limited" && e.at >= since).length;
  if (refused >= 3) {
    root.append(el("div", { class: "hint" }, icon("alert"), el("span", { text: trn("hint_rate_limited", refused) }),
      el("button", { class: "btn flat", type: "button", onclick: () => openSettings("delays", ["active_poll_seconds", "rate_limit_backoff_minutes"]) }, tr("hint_open_delays"))));
  }
  root.append(listBox);
}

// ---------- settings ----------
let settingsRepaint = null, settingsTabNow = null, lastWeb = "";
function openSettings(startTab = "switch", focusKeys = []) {
  const d = JSON.parse(JSON.stringify(S.settings));
  d.thresholds = d.thresholds || {};
  d.display_names = d.display_names || {};
  const order = rotationOf(d).concat(S.accounts.map((a) => a.name).filter((n) => !rotationOf(d).includes(n)));
  const included = new Set(rotationOf(d));
  let perWindow = Object.keys(d.thresholds).length > 0;
  let tab = startTab;
  let flashKeys = focusKeys; // highlighted at the first paint only
  let dirty = false;
  let m = null;
  const actions = () => [{ label: tr("cancel"), onClick: (mm) => mm.close() }, { label: tr("save"), primary: true, disabled: !dirty, onClick: save }];
  const touch = () => { dirty = true; if (m) m.setActions(actions(), tr("unsaved")); };
  const row = (key, control, { stack, disabled } = {}) => {
    const help = I18N.en["h_" + key] ? tr("h_" + key) : "";
    return el("div", { class: "row" + (stack ? " stack" : "") + (disabled ? " disabled" : "") + (flashKeys.includes(key) ? " flash" : ""), "data-key": key }, el("div", { class: "rl", text: tr("s_" + key) }), help ? el("div", { class: "rh", text: help }) : null, el("div", { class: "rc" }, control));
  };
  const tg = (key, opts = {}) => row(key, toggle({ checked: d[key], onChange: (v) => { d[key] = v; touch(); paint(); } }), opts);
  const STEP = { collect_seconds: 5, spontaneous_restart_seconds: 5, confirm_seconds: 5, reset_margin_seconds: 30, min_switch_gap_seconds: 1, active_poll_seconds: 30, rate_limit_backoff_minutes: 5, reread_resting_minutes: 1, observe_poll_seconds: 5 };
  const num = (key, unit, { scale = 1, min = 0 } = {}) => {
    const step = STEP[key] || 1;
    const input = el("input", { type: "text", inputmode: "numeric", value: Math.round(d[key] / scale), "aria-label": tr("s_" + key) });
    const set = (v) => { v = Math.max(min, Math.round(v)); input.value = v; d[key] = v * scale; minus.disabled = v <= min; touch(); };
    const minus = el("button", { class: "step-btn", type: "button", "aria-label": "−", disabled: Math.round(d[key] / scale) <= min, onclick: () => set(Number(input.value || 0) - step) }, icon("minus"));
    const plus = el("button", { class: "step-btn", type: "button", "aria-label": "+", onclick: () => set(Number(input.value || 0) + step) }, icon("plus"));
    input.addEventListener("input", () => { input.value = input.value.replace(/[^0-9]/g, ""); if (input.value !== "") { d[key] = Math.max(min, Number(input.value)) * scale; minus.disabled = Number(input.value) <= min; touch(); } });
    input.addEventListener("blur", () => set(Number(input.value || min)));
    input.addEventListener("keydown", (e) => { if (e.key === "ArrowUp") { e.preventDefault(); set(Number(input.value || 0) + step); } if (e.key === "ArrowDown") { e.preventDefault(); set(Number(input.value || 0) - step); } });
    return row(key, el("div", { class: "stepper" }, minus, el("label", { class: "step-val" }, input, el("span", { text: unit })), plus));
  };
  const slider = (get, set) => {
    const out = el("output", { text: `${get()} %` });
    const r = el("input", { type: "range", min: 50, max: 100, step: 1, value: get() });
    r.addEventListener("input", () => { set(Number(r.value)); out.textContent = `${r.value} %`; touch(); });
    return el("div", { class: "slider" }, r, out);
  };
  const panes = {
    switch: () => [tg("auto_switch"), tg("anticipate", { disabled: !d.auto_switch }), tg("switch_on_limit", { disabled: !d.auto_switch }), tg("anticipate_model_windows", { disabled: !d.auto_switch || !d.anticipate })],
    thresholds: () => [row("threshold_percent", slider(() => d.threshold_percent, (v) => { d.threshold_percent = v; })),
      row("per_window", toggle({ checked: perWindow, onChange: (v) => {
        perWindow = v;
        if (!v) d.thresholds = {};
        else for (const k of ["five_hour", "seven_day", "model"]) d.thresholds[k] = d.thresholds[k] ?? d.threshold_percent;
        touch(); paint();
      } })),
      perWindow ? ["five_hour", "seven_day", "model"].map((k) => row("t_" + k, slider(() => d.thresholds[k] ?? d.threshold_percent, (v) => { d.thresholds[k] = v; }))) : null],
    delays: () => [num("collect_seconds", tr("unit_s")), num("spontaneous_restart_seconds", tr("unit_s")), num("confirm_seconds", tr("unit_s"), { min: 1 }), num("reset_margin_seconds", tr("unit_s")),
      num("min_switch_gap_seconds", tr("unit_min"), { scale: 60 }), num("active_poll_seconds", tr("unit_s"), { min: 60 }), num("rate_limit_backoff_minutes", tr("unit_min"), { min: 1 }), num("reread_resting_minutes", tr("unit_min"), { min: 1 }), num("observe_poll_seconds", tr("unit_s"), { min: 5 })],
    accounts: () => {
      const list = el("div", { class: "acct-list" });
      order.forEach((n, i) => {
        const a = acct(n) || { email: n };
        const alias = el("input", { type: "text", value: d.display_names[n] || "", placeholder: tr("alias_ph") });
        alias.addEventListener("input", () => { if (alias.value.trim()) d.display_names[n] = alias.value; else delete d.display_names[n]; touch(); });
        list.append(el("div", { class: "acct-item" },
          toggle({ checked: included.has(n), onChange: (v) => { if (v) included.add(n); else included.delete(n); touch(); } }),
          el("button", { class: "av-btn", type: "button", "data-tip": tr("change_icon"), onclick: () => openIconPicker(acct(n) || { name: n, label: n }, paint) }, avatar(acct(n) || { name: n, label: n }, "sm")),
          el("span", { class: "em ellipsis", text: a.email, "data-tip": a.email }), alias,
          el("button", { class: "icon-btn", type: "button", "aria-label": tr("up"), disabled: i === 0, onclick: () => { [order[i - 1], order[i]] = [order[i], order[i - 1]]; touch(); paint(); } }, icon("up")),
          el("button", { class: "icon-btn", type: "button", "aria-label": tr("down"), disabled: i === order.length - 1, onclick: () => { [order[i + 1], order[i]] = [order[i], order[i + 1]]; touch(); paint(); } }, icon("down")),
          el("button", { class: "icon-btn danger", type: "button", "aria-label": tr("remove_account"), "data-tip": tr("remove_account"), disabled: (acct(n) || {}).place === "active", onclick: () => { m.close(); confirmRemove(acct(n) || { name: n, label: n }); } }, icon("trash"))));
      });
      list.append(el("button", { class: "btn flat add-row", type: "button", onclick: () => { m.close(); openAddAccount(); } }, icon("plus"), tr("add_account")));
      const backup = el("div", { class: "btn-row" },
        el("button", { class: "btn", type: "button", onclick: () => { m.close(); exportAccounts([]); } }, icon("download"), tr("export_all")),
        el("button", { class: "btn", type: "button", onclick: () => { m.close(); importBackup(); } }, icon("upload"), tr("import_backup")));
      return [row("next_account_rule", select({ value: d.next_account_rule, options: ["headroom", "order"].map((v) => ({ value: v, label: tr("rule_" + v), help: tr("rule_" + v + "_h") })), onChange: (v) => { d.next_account_rule = v; touch(); } })),
        row("accounts_list", list, { stack: true }), row("backup", backup, { stack: true })];
    },
    browser: () => {
      const w = S.web || {};
      const status = w.connected
        ? [tr("web_connected", { b: browserName(w.browser) }), w.account ? tr("web_on", { a: lab(w.account) }) : w.unknown_email ? tr("web_unknown", { e: w.unknown_email }) : tr("web_signed_out")].join(" · ")
        : tr("web_not_connected");
      const out = [el("div", { class: "about" },
        el("div", { class: "web-ico" + (w.connected ? " on" : "") }, icon("globe")),
        el("div", {}, el("div", { class: "about-name", text: "Claude Bascule Connector" }), el("div", { class: "about-st" + (w.connected ? " hot" : ""), text: status })),
        el("span", {}))];
      if (!w.connected) {
        out.push(el("div", {}, el("button", { class: "btn ink", type: "button", onclick: () => { m.close(); openBrowserGuide(); } }, icon("globe"), tr("guide_title"))));
      }
      // Which browser Claude Bascule works with, when there is a choice.
      if (BROWSERS.length > 1) {
        const cur = BROWSERS.find((b) => b.id === d.browser) || BROWSERS.find((b) => d.browser && b.id.split("-")[0] === d.browser.split("-")[0]) || BROWSERS[0];
        out.push(row("browser", select({ value: cur.id, options: BROWSERS.map((b) => ({ value: b.id, label: b.name })), onChange: (v) => { d.browser = v; touch(); } })));
      }
      out.push(tg("web_follow"), tg("cic_follow"));
      // Unattended moves through Chrome's accessibility (Linux): the recommended way.
      const ax = w.a11y || "unsupported";
      if (ax !== "unsupported") {
        const axText = ax === "ready" ? tr("ax_ready") : ax === "restart" ? tr("ax_restart") : tr("ax_off");
        out.push(el("div", { class: "about cdp" },
          el("div", { class: "web-ico" + (ax === "ready" ? " on" : "") }, icon("puzzle")),
          el("div", {}, el("div", { class: "about-name", text: tr("ax_title") }), el("div", { class: "about-st" + (ax === "ready" ? " hot" : ""), text: axText }),
            el("div", { class: "muted small", text: tr("ax_help") })),
          el("div", { class: "about-act" }, ax === "off" ? el("button", { class: "btn ink", type: "button", onclick: () => act(() => call("a11y.prepare"), tr("ax_prepared")) }, icon("check"), tr("ax_prepare")) : null)));
      }
      const cs = w.cdp || "off";
      const csText = cs === "off" ? tr("cdp_off") : cs === "available" ? tr("cdp_available") : cs === "connecting" ? tr("cdp_connecting") : cs === "connected" ? tr("cdp_connected") : tr("cdp_error", { e: cs });
      out.push(el("div", { class: "about cdp" },
        el("div", { class: "web-ico" + (cs === "connected" ? " on" : "") }, icon("puzzle")),
        el("div", {}, el("div", { class: "about-name", text: tr("cdp_title") }), el("div", { class: "about-st" + (cs === "connected" ? " hot" : ""), text: csText }),
          el("div", { class: "muted small", text: tr("cdp_help") })),
        el("div", { class: "about-act" },
          cs === "off" ? el("button", { class: "btn ink", type: "button", onclick: async () => { const b = (await invoke("browsers").catch(() => []))[0]; invoke("open_remote_debugging", { id: b ? b.id : "chrome" }).catch((e) => toast(String(e), "bad")); } }, icon("globe"), tr("cdp_enable"))
            : cs !== "connected" ? el("button", { class: "btn ink", type: "button", disabled: cs === "connecting", onclick: () => act(() => call("cdp.connect")) }, icon("link"), tr("cdp_connect")) : null)));
      out.push(row("cic_account", select({ value: w.cic_account || "", options: [{ value: "", label: tr("unknown") }, ...S.accounts.map((a) => ({ value: a.name, label: a.label }))],
        onChange: (v) => { if (v) act(() => call("web.cic_set", { account: v })); } })));
      const list = el("div", { class: "acct-list" });
      for (const a of orderedAccounts()) {
        const here = w.account === a.name, has = webHas(a.name);
        list.append(el("div", { class: "acct-item web-item" },
          avatar(a, "sm"),
          el("span", { class: "em ellipsis", text: a.label, "data-tip": a.email }),
          el("span", { class: "chip" + (here ? " live" : has ? "" : " off"), text: here ? tr("web_here") : has ? tr("web_saved") : tr("web_none") }),
          webCan(a) ? el("button", { class: "btn", type: "button", onclick: () => act(() => call("web.switch", { to: a.name }), tr("web_switch_sent")) }, icon("swap"), tr("web_switch"))
            : el("button", { class: "btn" + (has ? "" : " ink"), type: "button", disabled: !webOn() || here, onclick: () => act(() => call("web.login", { account: a.name }), tr("web_login_sent")) }, icon("key"), tr("web_login"))));
      }
      out.push(row("web_accounts", list, { stack: true }));
      out.push(el("p", { class: "muted small", text: tr("web_cic_note") }));
      return out;
    },
    resume: () => {
      const ta = el("textarea", { rows: 2, placeholder: S.default_message });
      ta.value = d.resume_message || "";
      const eff = el("div", { class: "muted" });
      const upd = () => { eff.textContent = tr("effective", { m: ta.value.trim() || S.default_message }); };
      ta.addEventListener("input", () => { d.resume_message = ta.value; upd(); touch(); });
      upd();
      return [tg("auto_send"), row("resume_message", el("div", { style: "display:grid;gap:6px;width:100%" }, ta, eff), { stack: true })];
    },
    // Applied at once: theme, colour and window buttons stay with this viewer; the language goes to the service (notifications follow).
    appearance: () => {
      const sw = el("div", { class: "swatches", role: "radiogroup" });
      const paintSw = () => fill(sw, Object.entries(ACCENTS).map(([k, [l, dk]]) => el("button", {
        class: "swatch" + (k === accent ? " sel" : "") + (k === "graphite" && isDark() ? " light-ink" : ""),
        type: "button", role: "radio", "aria-checked": String(k === accent), "aria-label": tr("accent_" + k), "data-tip": tr("accent_" + k),
        style: `--sw:${isDark() ? dk : l}`, onclick: () => { accent = k; store.set("accent", k); applyAccent(); paintSw(); } }, icon("check"))));
      paintSw();
      const sys = S.system_language && LANG_NAMES[S.system_language] ? LANG_NAMES[S.system_language] : "English";
      return [
        row("theme", segmented({ value: theme, options: THEMES.map((t) => ({ value: t, label: tr("theme_" + t) })), onChange: (v) => { theme = v; store.set("theme", v); applyTheme(); paintSw(); } })),
        row("accent", sw, { stack: true }),
        row("buttons_order", segmented({ value: wcOrder, options: ["classic", "macos"].map((v) => ({ value: v, label: tr("buttons_" + v) })), onChange: (v) => { wcOrder = v; store.set("wcorder", v); renderWindowButtons(); } })),
        row("buttons_side", segmented({ value: wcSide, options: ["left", "right"].map((v) => ({ value: v, label: tr("side_" + v) })), onChange: (v) => { wcSide = v; store.set("wcside", v); renderWindowButtons(); } })),
        row("language", select({ value: S.settings.language || "", options: [{ value: "", label: tr("lang_system", { l: sys }) }, ...Object.entries(LANG_NAMES).map(([v, label]) => ({ value: v, label }))],
          onChange: (v) => act(() => call("settings.set", { settings: { language: v } })) })),
      ];
    },
    advanced: () => [tg("notify"), tg("read_resets"), tg("hide_emails")],
    about: () => {
      const up = S.update || {};
      const status = up.available ? tr("update_avail", { v: up.latest }) : up.error ? tr("update_err", { e: up.error }) : up.checked_at ? tr("up_to_date") : "";
      const when = up.checked_at ? tr("checked_at", { t: fmtTime(up.checked_at) }) : "";
      const checkBtn = el("button", { class: "btn", type: "button", onclick: async () => {
        checkBtn.disabled = true;
        try { const r = await call("update.check"); S.update = r; } catch (e) { toast(String(e), "bad"); }
        paint();
      } }, icon("refresh"), tr("check_now"));
      return [
        el("div", { class: "about" },
          el("div", { class: "mark big", "aria-hidden": "true" }, el("span", { class: "glyph" })),
          el("div", {}, el("div", { class: "about-name", text: "Claude Bascule" }), el("div", { class: "muted", text: tr("about_version", { v: up.current || "?" }) }),
            el("div", { class: "about-st" + (up.available ? " hot" : up.error ? " bad" : ""), text: [status, when].filter(Boolean).join(" · ") })),
          el("div", { class: "about-act" }, up.available ? el("button", { class: "btn ink", type: "button", onclick: () => { m.close(); openUpdate(); } }, icon("download"), tr("update_go")) : checkBtn)),
        tg("check_updates"),
        el("p", { class: "muted small" }, el("a", { href: "#", onclick: (e) => { e.preventDefault(); invoke("open_url", { url: "https://github.com/moukrea/ClaudeBascule" }); } }, tr("source")), " · ", tr("disclaimer")),
      ];
    },
  };
  const shell = el("div", { class: "settings-shell" });
  function paint() {
    fill(shell,
      el("div", { class: "stabs", role: "tablist" }, Object.keys(panes).map((k) => el("button", { class: "stab" + (k === tab ? " sel" : ""), type: "button", role: "tab", onclick: () => { tab = k; paint(); } }, tr("stab_" + k)))),
      el("div", { class: "pane" }, el("h4", { text: tr("stab_" + tab) }), panes[tab]()));
    if (m) { m.setTitle(tr("settings")); m.setActions(actions(), dirty ? tr("unsaved") : null); }
  }
  async function save(mm) {
    const inc = order.filter((n) => included.has(n));
    d.rotation = inc.length === S.accounts.length && inc.every((n, i) => n === S.accounts[i].name) ? [] : inc;
    if (!perWindow) d.thresholds = {};
    delete d.language; // set on its own, at once
    delete d.avatar_icons; // idem
    try { await call("settings.set", { settings: d }); toast(tr("saved")); mm.close(); } catch (e) { toast(String(e), "bad"); }
    refresh();
  }
  paint();
  m = modal({ title: tr("settings"), wide: true, body: shell, actions: actions(), onClose: () => { settingsRepaint = null; } });
  flashKeys = [];
  if (focusKeys.length) setTimeout(() => { const r = shell.querySelector(`[data-key="${focusKeys[0]}"]`); if (r) r.scrollIntoView({ block: "center", behavior: "smooth" }); }, 80);
  settingsRepaint = paint;
  settingsTabNow = () => tab;
}

// ---------- render loop ----------
function banner(text, bad) {
  const b = document.getElementById("banner");
  b.hidden = !text;
  b.className = "banner" + (bad ? " bad" : "");
  fill(b, icon("alert"), el("span", { text: text || "" }));
}
let lastReady = 0, lastLang = null;
function render(force, pageChanged) {
  lang = I18N[S.language] ? S.language : "en";
  document.documentElement.lang = lang;
  const langChanged = lang !== lastLang;
  if (langChanged) { lastLang = lang; renderWindowButtons(); if (settingsRepaint) settingsRepaint(); }
  renderBar();
  if (langChanged) { moveTabIndicator(false); document.getElementById("legal").textContent = tr("disclaimer"); }
  if (S.observe_only) banner(tr("b_observe"));
  else if (S.waiting_return) banner(tr("b_all_out", { a: lab(S.waiting_return[0]), t: fmtWhen(S.waiting_return[1]) }), true);
  else banner("");
  const main = document.getElementById("page");
  const typing = main.contains(document.activeElement) && document.activeElement.tagName === "INPUT";
  if (!openPop && (force || !typing)) {
    const scroll = main.scrollTop;
    const root = el("div", { id: "page-in", class: pageChanged ? "enter" : "" });
    ({ accounts: renderAccounts, sessions: renderSessions, journal: renderJournal })[page](root);
    fill(main, root);
    main.scrollTop = pageChanged ? 0 : scroll;
  }
  renderLogin();
  paintGuideStatus();
  syncCic();
  // The browser pane follows the extension's state live.
  const wj = JSON.stringify(S.web || {});
  if (wj !== lastWeb) { lastWeb = wj; if (settingsRepaint && settingsTabNow && settingsTabNow() === "browser") settingsRepaint(); }
  const ready = (S.resume || []).filter((e) => e.state === "ready").length;
  if (ready > lastReady) {
    if (page !== "sessions") go("sessions");
    const w = win();
    w.unminimize().catch(() => {}); w.show().catch(() => {}); w.setFocus().catch(() => {}); w.requestUserAttention(2).catch(() => {});
  }
  lastReady = ready;
}
let lastJson = "";
// Until the service has answered once, the window shows that it is connecting (it may be
// starting at the same time); only later does a silence mean the service is down.
const bootAt = Date.now();
let everOk = false;
lang = (() => { const l = (navigator.language || "en").slice(0, 2); return I18N[l] ? l : "en"; })();
function booting() {
  frame.classList.add("booting");
  fill(document.getElementById("page"), el("div", { class: "boot" }, el("span", { class: "spinner" }), tr("b_connecting")));
}
async function refresh() {
  let st = null;
  try { st = await call("state"); } catch (e) { st = null; }
  if (!st || !st.accounts || !st.settings) {
    if (!everOk && Date.now() - bootAt < 20000) booting();
    else banner(tr("b_daemon"), true);
    return;
  }
  if (!everOk) {
    everOk = true; frame.classList.remove("booting"); lastJson = "";
    // A window opened after a while: look for a new version now rather than at the next hourly check.
    const up = st.update || {};
    if (st.settings.check_updates !== false && (!up.checked_at || Date.now() - up.checked_at > 600000)) call("update.check").catch(() => {});
  }
  S = st;
  const j = JSON.stringify({ ...st, at: 0 });
  if (j !== lastJson) {
    lastJson = j;
    try { render(); } catch (e) { console.error(e); }
  }
}
// Browsers installed here (Flatpak ones too on Linux), for the browser choice and the guide.
let BROWSERS = [];
invoke("browsers").then((l) => { BROWSERS = l || []; }).catch(() => {});
booting();
// A window left open for long: look again when it comes back to the front.
window.addEventListener("focus", () => {
  const up = (S && S.update) || {};
  if (S && S.settings && S.settings.check_updates !== false && (!up.checked_at || Date.now() - up.checked_at > 3600000)) call("update.check").catch(() => {});
});
refresh();
setInterval(refresh, 1500);
