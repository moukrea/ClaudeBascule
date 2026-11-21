# Rules for agents working on Claude Bascule

- Write code comments, documentation and commit messages in English. User-facing strings go
  through `app/ui/i18n.js` (six languages) or the daemon's `tr()`; never hard-code one language.
- **Tokens never leave their place.** Never print, log, commit or send an access or refresh token.
  Show a short SHA-256 fingerprint instead (`store::fp`). `.gitignore` blocks credential files; do
  not weaken it.
- **One live copy of each refresh token.** Copying a refresh token creates two holders; the first
  to refresh invalidates the other. Move tokens, never duplicate them.
- **Backups carry the claude.ai session, never Claude Code's tokens** (`backup.rs`): the user asks
  for the file and it is written 0600; an import never overwrites what a machine already has.
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
