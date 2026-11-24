# Rules for agents working on Claude Bascule

- Write code comments, documentation and commit messages in English. User-facing strings go
  through `app/ui/i18n.js` (six languages) or the daemon's `tr()`; never hard-code one language.
- **Tokens never leave their place.** Never print, log, commit or send an access or refresh token.
  Show a short SHA-256 fingerprint instead (`store::fp`). `.gitignore` blocks credential files; do
  not weaken it.
- **One live copy of each refresh token.** Copying a refresh token creates two holders; the first
  to refresh invalidates the other. Move tokens, never duplicate them.
- **Backups** (`backup.rs`, written 0600 on the user's request) carry the claude.ai session and
  Claude Code's sign-in. An import never overwrites what a machine already has; restoring a
  sign-in on another computer makes a second holder, which the import report says (the owner
  chose this: the first to renew keeps it).
- **A claude.ai session stays in the browser that signed in.** claude.ai refuses a session copied
  into another browser (`account_session_invalid`, seen on 2026-11-24) and that browser ends up
  signed out. Backups restore it only on the computer that made them; never copy one elsewhere.
- **The accessibility helper (`a11y_linux.py`) makes plain D-Bus calls** and walks only the pages
  it needs. libatspi loads a whole application's tree first and hangs on a browser with many tabs.
- **Never touch the real `~/.claude` (or the macOS keychain) during tests.** Use isolated folders
  through `BASCULE_CLAUDE_DIR`, `BASCULE_STATE`, `BASCULE_VAULT`, `BASCULE_CONFIG`. Record the
  fingerprint of the real credentials before and after any manual test and report a change.
- Only `claudeAiOauth` (credentials) and `oauthAccount` (`.claude.json`) are ever written; every
  other key is preserved. Writes are atomic, private, and never through a symlink.
- Facts about Claude Code internals come from reading the installed bundle and change with updates:
  note the Claude Code version with any new observation in `docs/internals.md`.
- Session detection and resume go through jaunt's service API; never simulate keystrokes when a
  jaunt action exists.
- PR titles follow Conventional Commits; every merge on `main` is released (CONTRIBUTING.md).
