<#
.SYNOPSIS
    Installs castorterm for the current user on Windows.

.DESCRIPTION
    Builds the release binary (unless -NoBuild), copies it to
    %LOCALAPPDATA%\Programs\castorterm, adds that folder to the user PATH
    (so agent hooks can run `castorterm --status`) and creates a Start menu
    shortcut (and a desktop one with -Desktop). Idempotent: run it again to
    update. Nothing needs administrator rights.

    Windows does not let programs pin to the taskbar: open Start, right-click
    castorterm and choose "Pin to taskbar" (or pin it while it runs).

.EXAMPLE
    pwsh scripts/install-windows.ps1
    pwsh scripts/install-windows.ps1 -NoBuild -Desktop
#>
param(
    [switch]$NoBuild,
    [switch]$Desktop
)
$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$installDir = Join-Path $env:LOCALAPPDATA "Programs\castorterm"
$exe = Join-Path $installDir "castorterm.exe"

if (-not $NoBuild) {
    Write-Host "Building castorterm (release)..."
    cargo build --release --manifest-path (Join-Path $repo "Cargo.toml")
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
}
$built = Join-Path $repo "target\release\castorterm.exe"
if (-not (Test-Path $built)) { throw "Not found: $built (build it or drop -NoBuild)" }

# The installed copy cannot be replaced while it runs.
$running = Get-Process castorterm -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -and ($_.Path -ieq $exe) }
if ($running) { throw "castorterm is running from ${exe}: close it and run the installer again" }

New-Item -ItemType Directory -Force $installDir | Out-Null
Copy-Item $built $exe -Force
Write-Host "Installed $exe"

# User PATH, without duplicates.
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
$entries = @($userPath -split ";" | Where-Object { $_ })
if (-not ($entries | Where-Object { $_.TrimEnd("\") -ieq $installDir })) {
    [Environment]::SetEnvironmentVariable("Path", (($entries + $installDir) -join ";"), "User")
    Write-Host "Added $installDir to the user PATH (open a new terminal to use it)"
}

function New-Shortcut([string]$Path) {
    $shell = New-Object -ComObject WScript.Shell
    $link = $shell.CreateShortcut($Path)
    $link.TargetPath = $exe
    $link.WorkingDirectory = $env:USERPROFILE
    $link.IconLocation = "$exe,0"
    $link.Description = "castorterm - terminal for AI agents"
    $link.Save()
    Write-Host "Shortcut: $Path"
}

$startMenu = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\castorterm.lnk"
New-Shortcut $startMenu
if ($Desktop) {
    New-Shortcut (Join-Path ([Environment]::GetFolderPath("Desktop")) "castorterm.lnk")
}

Write-Host ""
Write-Host "Done. To pin it: open Start, right-click castorterm, 'Pin to taskbar'."
