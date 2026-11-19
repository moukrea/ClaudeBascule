# Claude Bascule installer for Windows. Installs or updates, for the current user only.
#
#   irm https://github.com/moukrea/ClaudeBascule/releases/latest/download/install.ps1 | iex
#
# Options (environment):
#   CLAUDE_BASCULE_VERSION=1.2.3     a given release instead of the latest
#   CLAUDE_BASCULE_ARCHIVE=path      install from a local archive (development)
#   CLAUDE_BASCULE_NO_LAUNCH=1       do not open the app at the end

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

$repo = 'moukrea/ClaudeBascule'
$asset = 'claude-bascule-windows-x86_64.zip'
$dir = Join-Path $env:LOCALAPPDATA 'Programs\ClaudeBascule'
$tmp = Join-Path ([IO.Path]::GetTempPath()) ('claude-bascule-' + [guid]::NewGuid())
New-Item -ItemType Directory -Force -Path $tmp | Out-Null

try {
    $zip = Join-Path $tmp $asset
    if ($env:CLAUDE_BASCULE_ARCHIVE) {
        Copy-Item $env:CLAUDE_BASCULE_ARCHIVE $zip
    } else {
        if ($env:CLAUDE_BASCULE_VERSION) {
            $base = "https://github.com/$repo/releases/download/v$($env:CLAUDE_BASCULE_VERSION.TrimStart('v'))"
        } else {
            $base = "https://github.com/$repo/releases/latest/download"
        }
        Write-Host 'Downloading Claude Bascule...'
        Invoke-WebRequest "$base/$asset" -OutFile $zip -UseBasicParsing
        Invoke-WebRequest "$base/SHA256SUMS" -OutFile (Join-Path $tmp 'SHA256SUMS') -UseBasicParsing
        $line = Get-Content (Join-Path $tmp 'SHA256SUMS') | Where-Object { $_ -like "* $asset" } | Select-Object -First 1
        if (-not $line) { throw 'the release has no checksum for this download' }
        $want = ($line -split ' ')[0].ToLower()
        $got = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
        if ($want -ne $got) { throw 'the download is corrupted (checksum mismatch)' }
    }

    if (-not (Get-Command claude -ErrorAction SilentlyContinue)) {
        Write-Warning 'Claude Code (claude) was not found on this machine: install it first, Claude Bascule switches its account.'
    }

    # Stop the running copies (the window and the service) so their files can be replaced.
    Get-Process -Name bascule-app, bascule -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 800

    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    Expand-Archive -Path $zip -DestinationPath $dir -Force

    $app = Join-Path $dir 'bascule-app.exe'

    # The browser extension (Claude Bascule Connector) and its native messaging host.
    $hostId = 'io.github.moukrea.claudebascule'
    $hostJson = Join-Path $dir 'native-host.json'
    @{ name = $hostId; description = 'Claude Bascule'; path = (Join-Path $dir 'bascule.exe'); type = 'stdio';
       allowed_origins = @('chrome-extension://comipnngbifajjpfbagbmlfgckllcobj/') } | ConvertTo-Json | Set-Content -Encoding ASCII $hostJson
    foreach ($b in 'Google\Chrome', 'Chromium', 'BraveSoftware\Brave-Browser', 'Microsoft\Edge', 'Vivaldi') {
        $key = "HKCU:\Software\$b\NativeMessagingHosts\$hostId"
        New-Item -Path $key -Force | Out-Null
        Set-ItemProperty -Path $key -Name '(default)' -Value $hostJson
    }

    # The service starts with the session (no console window).
    Set-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name 'ClaudeBascule' -Value "`"$app`" --service"

    # Start menu shortcut.
    $lnk = Join-Path ([Environment]::GetFolderPath('Programs')) 'Claude Bascule.lnk'
    $shell = New-Object -ComObject WScript.Shell
    $s = $shell.CreateShortcut($lnk)
    $s.TargetPath = $app
    $s.WorkingDirectory = $dir
    $s.IconLocation = "$app,0"
    $s.Description = 'Switch Claude Code between your accounts before a usage limit'
    $s.Save()

    # The command line tool on the user's PATH.
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (-not (($userPath -split ';') -contains $dir)) {
        [Environment]::SetEnvironmentVariable('Path', ((@($userPath, $dir) | Where-Object { $_ }) -join ';'), 'User')
    }

    Start-Process -FilePath $app -ArgumentList '--service' -WindowStyle Hidden
    # Wait for the service to answer before opening the window.
    # (PowerShell 5 turns a native command's error output into errors: not fatal here.)
    $cli = Join-Path $dir 'bascule.exe'
    $ErrorActionPreference = 'Continue'
    for ($i = 0; $i -lt 30; $i++) {
        try { & $cli state *> $null } catch { }
        if ($LASTEXITCODE -eq 0) { break }
        Start-Sleep -Milliseconds 500
    }
    $ErrorActionPreference = 'Stop'
    if (-not $env:CLAUDE_BASCULE_NO_LAUNCH) {
        Start-Process -FilePath $app
    }
    Write-Host 'Claude Bascule is installed.' -ForegroundColor Green
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
