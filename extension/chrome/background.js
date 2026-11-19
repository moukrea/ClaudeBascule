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
    const d = { url: urlOf(c), name: c.name, value: c.value, path: c.path || "/", secure: !!c.secure, httpOnly: !!c.httpOnly };
    if (!c.hostOnly) d.domain = c.domain;
    if (c.sameSite && c.sameSite !== "unspecified") d.sameSite = c.sameSite;
    if (!c.session && c.expirationDate) d.expirationDate = c.expirationDate;
    await chrome.cookies.set(d);
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
