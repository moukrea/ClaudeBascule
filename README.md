<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/header-dark.png">
    <img src="assets/header-light.png" alt="Claude Bascule" height="72">
  </picture>
</p>

<p align="center">
  <b>Claude Bascule is an independent, free and open-source tool. It is not made by Anthropic,<br>
  not affiliated with Anthropic, and not endorsed or sponsored by Anthropic.</b><br>
  <sub>Claude and Claude Code are trademarks of Anthropic.</sub>
</p>

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/screenshot-dark.png">
    <img src="assets/screenshot-light.png" alt="Claude Bascule: three accounts with their quotas" width="860">
  </picture>
</p>

If you use several Claude accounts with Claude Code, Claude Bascule moves Claude Code to another
account **before** you hit a usage limit, and offers to resume the work a limit interrupted.

- **Automatic switch**: when the account in use nears a limit (95 % by default), Claude Code moves
  to the account with the most room left. Your running sessions follow at their next message.
- **All your quotas at a glance**: 5-hour, weekly and per-model limits of every account, and the
  resets Anthropic offers you.
- **Switch by hand** whenever you like, in one click.
- **Resume interrupted sessions** with a message of your choice (needs [jaunt](https://jaunt.one)).
- **Everything adjustable**: thresholds, delays, the order of accounts, notifications, theme,
  language (English, French, German, Spanish, Italian, Portuguese).
- **Updates itself**: when a new version is out, the app offers it.

Your sign-in tokens stay on your computer. Claude Bascule only talks to Anthropic to read quotas,
as Claude Code does.

## Install

You need [Claude Code](https://docs.anthropic.com/claude-code) installed.

**macOS and Linux**: open a terminal and run

```sh
curl -fsSL https://github.com/moukrea/ClaudeBascule/releases/latest/download/install.sh | sh
```

**Windows**: open PowerShell and run

```powershell
irm https://github.com/moukrea/ClaudeBascule/releases/latest/download/install.ps1 | iex
```

The same command updates Claude Bascule. It installs for your user only (no administrator rights)
and starts with your session.

> Linux: x86_64, with WebKitGTK 4.1 (present on most desktops; the installer tells you if it is missing).
> macOS: the app is not notarised by Apple; installing with the command above avoids the warning.

## First steps

1. Open **Claude Bascule**. The account Claude Code is signed in with is already there.
2. Click **Add an account** and sign in to each of your other Claude accounts in your browser.
3. That's it: automatic switching is on (the lightning toggle at the top).

To resume interrupted sessions automatically, install [jaunt](https://jaunt.one) and click
**Connect jaunt** at the top of the window.

## Uninstall

- **Linux**: `systemctl --user disable --now claude-bascule; rm -f ~/.local/bin/bascule ~/.local/bin/bascule-app ~/.config/systemd/user/claude-bascule.service ~/.local/share/applications/claude-bascule.desktop`
- **macOS**: `launchctl bootout gui/$(id -u)/io.github.moukrea.claudebascule; rm -rf ~/Applications/Claude\ Bascule.app ~/Library/LaunchAgents/io.github.moukrea.claudebascule.plist ~/.local/bin/bascule`
- **Windows**: close the app, then in PowerShell `Remove-ItemProperty HKCU:\Software\Microsoft\Windows\CurrentVersion\Run ClaudeBascule; Remove-Item -Recurse "$env:LOCALAPPDATA\Programs\ClaudeBascule", "$([Environment]::GetFolderPath('Programs'))\Claude Bascule.lnk"`

Your settings and the saved accounts stay in a private folder ([docs/internals.md](docs/internals.md#folders))
until you delete it.

## Questions

- **Is it allowed?** Claude Bascule switches between accounts that are yours; check that your use
  of several accounts follows [Anthropic's terms](https://www.anthropic.com/legal).
- **Something is wrong?** [Open an issue](https://github.com/moukrea/ClaudeBascule/issues).
- **Want to help?** See [CONTRIBUTING.md](CONTRIBUTING.md).

MIT licence.
