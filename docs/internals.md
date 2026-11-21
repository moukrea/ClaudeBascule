# How Claude Bascule works

For contributors. Facts about Claude Code come from reading its installed bundle and from
observation (Claude Code 2.1.x, November 2026); they can change with any Claude Code update.

## Where Claude Code keeps an identity

| | Linux, Windows | macOS |
| --- | --- | --- |
| Tokens | `~/.claude/.credentials.json`, key `claudeAiOauth` | login keychain, service `Claude Code-credentials`, account `$USER` (JSON text) |
| Account | `~/.claude.json`, key `oauthAccount` | same |

- With `CLAUDE_CONFIG_DIR=<dir>`, the files are `<dir>/.credentials.json` and `<dir>/.claude.json`;
  on macOS the keychain service becomes `Claude Code-credentials-<first 8 hex of sha256(dir)>`.
- `.credentials.json` holds other keys (MCP logins under `mcpOAuth`): only `claudeAiOauth` is ever
  written, every other key is kept.
- A running Claude Code re-reads its credentials when the file's modification time changes, before
  its next request. A switch therefore reaches every running session at its next request.
- The OAuth refresh is guarded (compare-and-swap): a refresh token must have **one** live copy;
  copying it makes two holders, and the first refresh invalidates the other.

## The vault

Each account has its own `CLAUDE_CONFIG_DIR` under the state folder (`accounts/<id>`). The active
account's tokens live in Claude Code's own place; resting accounts' tokens live in their vault
directory. A switch:

1. checks the outgoing tokens back into the outgoing account's vault directory,
2. writes the incoming account's `claudeAiOauth` and `oauthAccount` into Claude Code's place,
3. clears the incoming account's vault copy (one live copy),
4. verifies the result.

Writes are atomic (temporary file, then rename), mode 0600 on Unix, and never through a symlink.
Resting tokens are refreshed by running `claude auth status` with that account's
`CLAUDE_CONFIG_DIR`, before they expire. Signing in runs `claude auth login --email <email>` in the
account's vault directory.

## Quotas

`GET https://api.anthropic.com/api/oauth/usage?cedar_ember=1` with the account's access token,
`anthropic-beta: oauth-2025-04-20` and a `claude-cli/<version>` user agent. It returns the 5-hour
and weekly windows, model-scoped weekly windows (`limits[]`), and, with `cedar_ember=1`, the
offered resets (read only). A 429 is answered by backing off.

Between two reads, the numbers Claude Code's status line shows (through jaunt) refresh only after a
real request, so only sessions that worked since the last switch count.

## Switching rules

- **Anticipation**: when a window of the active account reaches the threshold (95 % by default,
  per-window thresholds possible). A model-scoped window (for example a model's weekly limit)
  counts only while a working session uses that model.
- **On limit**: when a session reports a usage limit, Claude Bascule waits a short collection
  window (30 s by default) for the other sessions to report theirs, switches once, and offers to
  resume the blocked sessions.
- **Next account**: the one with the most headroom on the windows that matter (or a fixed order).
  An exhausted account comes back after its reset time plus a margin.

## jaunt

[jaunt](https://jaunt.one) exposes the Claude Code sessions of a machine to services enrolled with
`jaunt agents enroll`. Claude Bascule connects to jaunt's local control endpoint (a Unix socket, or
on Windows a 127.0.0.1 port with a key in `%LOCALAPPDATA%\jaunt\control.json`), subscribes to
session events, and sends the resume message with jaunt's actions (never simulated keystrokes).
Without jaunt, switching still works; blocked sessions just aren't resumed for you.

## claude.ai in the browser

A claude.ai session is a set of cookies on `claude.ai` (`sessionKey`, `sessionKeyV3`, their `LC`
variants, `routingHint`, `lastActiveOrg`; the first ones are HttpOnly). The **Claude Bascule
Connector** extension (`extension/chrome`, fixed ID `comipnngbifajjpfbagbmlfgckllcobj` from the
manifest's `key`) reads and sets them with `chrome.cookies`, and talks to the service through the
browser's native messaging: the installer registers the host `io.github.moukrea.claudebascule`,
which is `bascule` itself (started with the extension's origin as argument; see
`crates/bascule/src/native_host.rs`). The host relays `web.hello`, `web.capture` and `web.result`
to the service, and long-polls `web.next` for commands (`switch`, `login`).

- A capture (sent when the session cookies change) is matched to an account by the email
  `https://claude.ai/api/account` returns, else by `lastActiveOrg` against each account's
  `organizationUuid`, and saved to `accounts/<id>/web-session.json` (private).
- A switch saves the browser's current session, removes the session cookies, sets the target's,
  reloads claude.ai tabs and checks the result.
- The Claude in Chrome extension signs in on its own (OAuth with PKCE through
  `claude.ai/oauth/authorize`, tokens in its own storage): it follows the browser's session at its
  next (silent) sign-in only.

## Backups

`bascule export` (and the app's export) writes `claude-bascule-<account|accounts>-<date>.json`
(0600) in the downloads folder (`XDG_DOWNLOAD_DIR` on Linux):

```json
{"format": "claude-bascule-accounts", "version": 1, "exported_at": "…", "app_version": "…",
  "accounts": [{"email": "…", "display_name": "…", "icon": "briefcase", "in_rotation": true,
    "claude_ai": {"cookies": [{"name": "sessionKey", "…": "…"}], "saved_at": 0}}]}
```

Only the claude.ai session cookies are kept. Claude Code's tokens are never exported: a refresh
token must have one live copy. An import adds missing accounts (signed out until they sign in),
keeps the rotation membership, and only fills what an existing account lacks (session, display
name, icon).

## For other programs

`bascule usage --json` prints each account's windows as the service last read them, numbers only:

```json
{"fetched_at": "2026-11-19T20:55:12Z", "accounts": [{"place": "active", "fetched_at": "2026-11-19T20:55:12Z",
  "windows": [{"name": "seven_day", "percent": 71.0, "resets_at": "2026-11-24T07:00:00Z"}]}]}
```

`place` is `active` or `rest`; window names are `five_hour`, `seven_day`, `seven_day_opus`,
`seven_day_sonnet` and `model:<name>`; a window whose reset time has passed shows 0. No name,
address, token or fingerprint, and no network request (it asks the local service). When the
service does not run, it exits with a non-zero code and prints nothing on stdout.

## Local endpoint

The service answers the app and the CLI on `<state>/bascule.sock` (Unix) or, on Windows, on a
127.0.0.1 port written with a random key in `<state>/control.json`; one JSON request per line.

## Folders

| | Linux | macOS | Windows |
| --- | --- | --- | --- |
| State (vault, log) | `~/.local/state/bascule` | `~/Library/Application Support/ClaudeBascule` | `%LOCALAPPDATA%\ClaudeBascule` |
| Settings, accounts | `~/.config/bascule` | same as state | `%APPDATA%\ClaudeBascule` |
| Programs | `~/.local/bin` | `~/Applications/Claude Bascule.app` | `%LOCALAPPDATA%\Programs\ClaudeBascule` |
| Service | systemd user unit `claude-bascule` | LaunchAgent `io.github.moukrea.claudebascule` | `HKCU\…\Run\ClaudeBascule` |
