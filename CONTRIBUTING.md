# Contributing

## Layout

| Path | What |
| --- | --- |
| `crates/bascule-core` | Everything that matters: vault and switch (`store`, `vault`), quotas (`usage`), rules (`engine`), the service (`daemon`, `control`), jaunt client, platform specifics (`platform`), self-update (`update`). |
| `crates/bascule` | The `bascule` command line (status, switch, login, daemon…). |
| `app` | The window (Tauri 2, static HTML/CSS/JS in `app/ui`, no build step). `bascule-app --service` runs the service without a window. |
| `dist` | Installers and packaging used by the release. |
| `tools` | Icons, README images, version calculation, local install. |
| `docs/internals.md` | How it works: where Claude Code keeps tokens, the switch, quotas, rules. |

## Build and try

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
tools/dev-install.sh        # Linux: build, package and install like a release
```

Linux needs `libwebkit2gtk-4.1-dev` (Debian/Ubuntu) or `webkit2gtk4.1-devel` (Fedora) to build the window.

## Rules

- **Tokens never leave their place.** Never print, log, commit or send an access or refresh token;
  show a short SHA-256 fingerprint instead.
- **One live copy of each refresh token.** Move tokens, never duplicate them.
- **Never touch your real `~/.claude` in tests**: point `BASCULE_CLAUDE_DIR`, `BASCULE_STATE`,
  `BASCULE_VAULT` and `BASCULE_CONFIG` at test folders.
- Only `claudeAiOauth` (credentials) and `oauthAccount` (`.claude.json`) are written; every other key
  is preserved. Atomic writes, private permissions, no symlinks.
- UI strings exist in all six languages (`app/ui/i18n.js`); CI checks it.

## Pull requests and releases

- The PR title follows [Conventional Commits](https://www.conventionalcommits.org): `feat: …`,
  `fix(app): …`, `docs: …`, `feat!: …` for a breaking change. PRs are squash-merged, so the title
  becomes the commit on `main`.
- CI runs formatting, lint and tests on Linux, macOS and Windows for every PR.
- **Every merge on `main` is released.** The version follows the merged titles since the last
  release: a breaking change bumps the major, `feat` the minor, anything else the patch.
