// Claude Bascule Connector: tells Claude Bascule which account claude.ai is signed in with in this
// browser (and its session, kept by Claude Bascule in its private vault), and applies the session
// Claude Bascule asks for. It talks only to the Claude Bascule program on this computer, through
// the browser's native messaging. Cookies are never logged.
"use strict";

const HOST = "io.github.moukrea.claudebascule";
const SESSION = ["sessionKey", "sessionKeyV3", "sessionKeyLC", "sessionKeyV3LC", "routingHint", "lastActiveOrg"];

let port = null;
let nextId = 1;
const pending = new Map();
let applying = false; // our own cookie changes are not captures
let captureTimer = null;

function connect() {
  if (port) return;
  try {
    port = chrome.runtime.connectNative(HOST);
  } catch (e) {
    port = null;
    return;
  }
  port.onMessage.addListener(onMessage);
  port.onDisconnect.addListener(() => {
    void chrome.runtime.lastError;
    port = null;
    for (const [, p] of pending) p.reject(new Error("disconnected"));
    pending.clear();
  });
  rpc("web.hello", { version: chrome.runtime.getManifest().version, browser: navigator.userAgent }).catch(() => {});
  capture();
}

function rpc(method, params) {
  return new Promise((resolve, reject) => {
    if (!port) return reject(new Error("not connected"));
    const id = nextId++;
    pending.set(id, { resolve, reject });
    port.postMessage({ id, method, params });
  });
}

function onMessage(m) {
  if (m && m.push) { handle(m.push); return; }
  const p = m && pending.get(m.id);
  if (!p) return;
  pending.delete(m.id);
  if (m.error) p.reject(new Error(m.error)); else p.resolve(m.result);
}

const urlOf = (c) => `https://${c.domain.replace(/^\./, "")}${c.path || "/"}`;

async function sessionCookies() {
  const all = await chrome.cookies.getAll({ domain: "claude.ai" });
  return all.filter((c) => SESSION.includes(c.name) && c.domain.replace(/^\./, "") === "claude.ai");
}

// The signed-in account's email, read from claude.ai with the browser's own session.
async function accountEmail() {
  try {
    const r = await fetch("https://claude.ai/api/account", { credentials: "include", headers: { accept: "application/json" } });
    if (!r.ok) return null;
    const j = await r.json();
    return j.email_address || (j.account && j.account.email_address) || null;
  } catch (e) {
    return null;
  }
}

async function capture() {
  if (!port || applying) return null;
  const cookies = await sessionCookies();
  const signedIn = cookies.some((c) => c.name === "sessionKey" || c.name === "sessionKeyV3");
  const email = signedIn ? await accountEmail() : null;
  try {
    return await rpc("web.capture", { email, cookies });
  } catch (e) {
    return null;
  }
}

async function clearSession() {
  for (const c of await sessionCookies()) {
    await chrome.cookies.remove({ url: urlOf(c), name: c.name, storeId: c.storeId });
  }
}

async function setCookies(list) {
  for (const c of list) {
    // A cookie Chrome refuses must not stop the others (nor be silent).
    const d = { url: urlOf(c), name: c.name, value: c.value, path: c.path || "/", secure: !!c.secure, httpOnly: !!c.httpOnly };
    if (!c.hostOnly) d.domain = c.domain;
    if (c.sameSite && c.sameSite !== "unspecified") d.sameSite = c.sameSite;
    if (!c.session && c.expirationDate) d.expirationDate = c.expirationDate;
    if (d.sameSite === "no_restriction") d.secure = true;
    const r = await chrome.cookies.set(d).catch((e) => null);
    if (!r) rpc("web.result", { ok: false, to: null, error: `cookie ${c.name} refused: ${chrome.runtime.lastError ? chrome.runtime.lastError.message : "?"}` }).catch(() => {});
  }
}

async function reloadClaudeTabs() {
  const tabs = await chrome.tabs.query({ url: "https://claude.ai/*" });
  for (const t of tabs) chrome.tabs.reload(t.id).catch(() => {});
}

async function handle(cmd) {
  if (cmd.type === "switch") {
    let ok = false, error = null;
    try {
      await capture(); // the outgoing account's latest session is kept
      applying = true;
      await clearSession();
      await setCookies(cmd.cookies || []);
      applying = false;
      await reloadClaudeTabs();
      await new Promise((r) => setTimeout(r, 1500));
      const r = await capture();
      ok = !!r && r.account === cmd.to;
      if (!ok) error = "claude.ai did not accept the saved session: sign this account in again in the browser";
    } catch (e) {
      error = String(e);
    } finally {
      applying = false;
    }
    rpc("web.result", { ok, to: cmd.to, error }).catch(() => {});
  } else if (cmd.type === "reload") {
    chrome.runtime.reload();
  } else if (cmd.type === "cic_begin") {
    try {
      await cicBegin(cmd);
    } catch (e) {
      rpc("web.result", { ok: false, to: cmd.to, error: "Claude in Chrome move: " + e }).catch(() => {});
    }
  } else if (cmd.type === "cic_stop") {
    cic = null;
  } else if (cmd.type === "cic_cleanup") {
    // Claude in Chrome's options and leftover sign-in pages opened for the move.
    for (const t of await chrome.tabs.query({})) {
      const u = t.url || "";
      if (u.startsWith(`chrome-extension://${CIC_ID}/options.html`) || u.startsWith("https://claude.ai/oauth/authorize")) chrome.tabs.remove(t.id).catch(() => {});
    }
  } else if (cmd.type === "login") {
    try {
      await capture();
      applying = true;
      await clearSession();
    } finally {
      applying = false;
    }
    await chrome.tabs.create({ url: "https://claude.ai/login", active: true });
  }
}

// ---- Moving Claude in Chrome (another extension: it cannot be driven, only watched) ----
// claude.ai carries the target account while the user signs Claude in Chrome out and in again;
// its sign-in page (claude.ai/oauth/authorize for its client) closing or leaving means done.
const CIC_ID = "fcoeoabgfenejglbffodgkkbkcdhcgfn";
const CIC_CLIENT = "dae2cad8-15c5-43d2-9046-fcaecc135fa4";
let cic = null; // { id, authTab }

async function cicBegin(cmd) {
  // Sign-in pages left over from an earlier attempt would hide the options page.
  for (const t of await chrome.tabs.query({ url: "https://claude.ai/oauth/authorize*" })) chrome.tabs.remove(t.id).catch(() => {});
  try {
    await capture();
    applying = true;
    await clearSession();
    await setCookies(cmd.cookies || []);
  } finally {
    applying = false;
  }
  await reloadClaudeTabs();
  cic = { id: cmd.id, authTab: null };
  setTimeout(capture, 1500);
  // Claude Bascule itself opens Claude in Chrome's options (an extension may not).
}

function cicFinish(ok, error) {
  if (!cic) return;
  const id = cic.id;
  cic = null;
  rpc("web.cic_done", { id, ok, error: error || null }).catch(() => {});
}

chrome.tabs.onUpdated.addListener((tabId, info, tab) => {
  if (!cic) return;
  const url = info.url || tab.url || "";
  if (url.startsWith("https://claude.ai/oauth/authorize") && url.includes(CIC_CLIENT)) {
    cic.authTab = tabId;
  } else if (cic.authTab === tabId && info.url) {
    setTimeout(() => cicFinish(true), 1500);
  }
});
chrome.tabs.onRemoved.addListener((tabId) => {
  if (cic && cic.authTab === tabId) setTimeout(() => cicFinish(true), 1500);
});

// A sign-in, sign-out or account change on claude.ai is reported (once things settle).
chrome.cookies.onChanged.addListener((info) => {
  const c = info.cookie;
  if (applying || c.domain.replace(/^\./, "") !== "claude.ai" || !SESSION.includes(c.name)) return;
  clearTimeout(captureTimer);
  captureTimer = setTimeout(capture, 2000);
});

chrome.runtime.onStartup.addListener(connect);
chrome.runtime.onInstalled.addListener(connect);
chrome.alarms.create("reconnect", { periodInMinutes: 1 });
chrome.alarms.onAlarm.addListener((a) => { if (a.name === "reconnect") connect(); });
connect();
