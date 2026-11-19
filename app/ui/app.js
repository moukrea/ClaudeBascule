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
  trash: "M3 6h18M8 6V4h8v2M19 6l-1 14H6L5 6M10 11v6M14 11v6", download: "M12 3v12M7 10l5 5 5-5M5 21h14",
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
const fmtWhen = (iso) => {
  const d = new Date(iso);
  return d.toDateString() === new Date().toDateString() ? fmtTime(d) : d.toLocaleString(lang, { weekday: "short", hour: "2-digit", minute: "2-digit" });
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
try { win().onResized(() => { syncMaximized(); moveTabIndicator(false); }); } catch { /* outside Tauri */ }
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
  const counts = { accounts: [S.accounts.length, false], sessions: ready ? [ready, true] : [(S.sessions || []).length, false], journal: null };
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
  root.append(el("div", { class: "head" },
    el("div", {}, el("h1", { text: tr("tab_accounts") }),
      el("div", { class: "sub", text: st.auto_switch && st.anticipate ? trn("accounts_sub_on", S.accounts.length, { p: st.threshold_percent }) : trn("accounts_sub_off", S.accounts.length) })),
    el("div", { class: "head-actions" },
      el("button", { class: "btn flat", type: "button", onclick: openAddAccount }, icon("plus"), tr("add_account")),
      S.accounts.length ? el("button", { class: "btn flat", type: "button", onclick: () => act(() => call("reread"), tr("reread_ok")) }, icon("refresh"), tr("reread")) : null)));
  if (!S.accounts.length) {
    root.append(el("div", { class: "empty" },
      el("div", { class: "empty-ico" }, icon("user")),
      el("h2", { text: tr("empty_title") }), el("p", { text: tr("empty_body") }),
      el("button", { class: "btn ink", type: "button", onclick: openAddAccount }, icon("plus"), tr("add_account"))));
    return;
  }
  // One account: a card of normal width rather than one stretched over the window.
  const cols = S.accounts.length === 1 ? "minmax(0, 560px)" : S.accounts.map((x) => (x.place === "active" ? "minmax(0, 1.12fr)" : "minmax(0, 1fr)")).join(" ");
  const cards = el("div", { class: "cards", style: `--n:${Math.min(4, S.accounts.length)};--cols:${cols}` });
  const rot = rotationOf(st);
  const sessions = (S.sessions || []).length;
  for (const a of S.accounts) {
    const active = a.place === "active";
    const wins = (a.usage && a.usage.windows) || [];
    const find = (n) => wins.find((w) => w.name === n);
    const is20x = /max_20x/.test(a.tier || "");
    const badges = [active ? el("span", { class: "badge live", text: tr("in_service") }) : el("span", { class: "badge", text: a.place === "resting" ? tr("resting") : tr("signed_out") })];
    if (plan(a)) badges.push(el("span", { text: plan(a) }));
    if (a.exhausted && a.exhausted.until_ms > Date.now()) badges.push(el("span", { class: "badge warn", text: tr("until", { t: fmtTime(a.exhausted.until_ms) }) }));
    if (!rot.includes(a.name)) badges.push(el("span", { text: tr("out_rot") }));
    const more = el("button", { class: "icon-btn", type: "button", "aria-label": "Menu" }, icon("more"));
    more.addEventListener("click", (ev) => {
      ev.stopPropagation();
      const inRot = rot.includes(a.name);
      menu(more, [
        !active && a.place === "resting" ? { icon: "swap", label: tr("switch_here"), onClick: () => confirmSwitch(a) } : null,
        { icon: "refresh", label: tr("reread"), onClick: () => act(() => call("reread"), tr("reread_ok")) },
        { icon: "smile", label: tr("change_icon"), onClick: () => openIconPicker(a) },
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
      el("div", { class: "c-id" }, el("div", { class: "c-name ellipsis", text: a.label, "data-tip": a.label, "data-tip-sub": tipSub || null }), el("div", { class: "c-state" }, badges)),
      more);
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
}
// A quota read failure in plain words; the raw error stays in the tooltip.
function usageError(e) {
  if (/429/.test(e)) return tr("err429");
  if (/HTTP 40[13]|no token/.test(e)) return tr("err_auth");
  return tr("err_network");
}
function confirmSwitch(a) {
  modal({ title: tr("switch_title", { a: a.label }), body: el("p", { text: tr("switch_body") }), actions: [
    { label: tr("cancel"), onClick: (m) => m.close() },
    { label: tr("switch_go"), primary: true, icon: "swap", onClick: (m) => { m.close(); act(() => call("switch", { to: a.name }), tr("switched", { a: a.label })); } },
  ] });
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
function confirmRemove(a) {
  modal({ title: tr("remove_title", { a: a.label }), body: el("p", { text: tr("remove_body") }), actions: [
    { label: tr("cancel"), onClick: (m) => m.close() },
    { label: tr("remove_go"), primary: true, danger: true, icon: "trash", onClick: (m) => { m.close(); act(() => call("accounts.remove", { name: a.name }), tr("removed")); } },
  ] });
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
  const m = modal({ title: tr("add_title"), body: el("div", { class: "form" }, el("p", { class: "muted", text: tr("add_body") }), input), actions: [
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
    if (/README/.test(line)) continue;
    if (/^#{1,6}\s/.test(line)) {
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
  "anticipate.no_room": "warn", "anticipate.model_idle": "warn", "usage.rate_limited": "warn" };
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
let settingsRepaint = null;
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
      return [row("next_account_rule", select({ value: d.next_account_rule, options: ["headroom", "order"].map((v) => ({ value: v, label: tr("rule_" + v), help: tr("rule_" + v + "_h") })), onChange: (v) => { d.next_account_rule = v; touch(); } })),
        row("accounts_list", list, { stack: true })];
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
  else if (S.waiting_return) banner(tr("b_all_out", { a: lab(S.waiting_return[0]), t: fmtTime(S.waiting_return[1]) }), true);
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
booting();
// A window left open for long: look again when it comes back to the front.
window.addEventListener("focus", () => {
  const up = (S && S.update) || {};
  if (S && S.settings && S.settings.check_updates !== false && (!up.checked_at || Date.now() - up.checked_at > 3600000)) call("update.check").catch(() => {});
});
refresh();
setInterval(refresh, 1500);
