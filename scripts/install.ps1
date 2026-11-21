# Installs the built binaries for the current user:
#   %LOCALAPPDATA%\Programs\perch\      the two exes
#   Start Menu\Programs\Perch.lnk  the window
#   WindowsApps\perch.cmd               `perch` in any terminal (that folder is already on PATH)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$dir = Join-Path $env:LOCALAPPDATA "Programs\perch"
New-Item -ItemType Directory -Force $dir | Out-Null

foreach ($exe in "perch.exe", "perch-app.exe") {
    $src = Join-Path $root "target\release\$exe"
    if (-not (Test-Path $src)) { throw "missing $src - run: cargo build --release --workspace" }
    Copy-Item $src $dir -Force
}

$ws = New-Object -ComObject WScript.Shell
$lnkPath = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\Perch.lnk"
Copy-Item (Join-Path $root "crates\perch-app\assets\logo.ico") (Join-Path $dir "perch.ico") -Force
$lnk = $ws.CreateShortcut($lnkPath)
$lnk.TargetPath = Join-Path $dir "perch-app.exe"
$lnk.WorkingDirectory = $dir
$lnk.Description = "Mouse, keyboard and webcam settings"
$lnk.IconLocation = Join-Path $dir "perch.ico"
$lnk.Save()

$shim = Join-Path $env:LOCALAPPDATA "Microsoft\WindowsApps\perch.cmd"
"@echo off`r`n`"$dir\perch.exe`" %*" | Set-Content -Encoding ASCII $shim

Write-Output "installed $dir"
Write-Output "shortcut  $lnkPath"
Write-Output "command   $shim"
