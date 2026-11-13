param(
    [switch]$SkipInstall
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $root

$cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
if (-not (Test-Path $cargo)) { $cargo = 'cargo' }

Write-Host '==> Building x64 release' -ForegroundColor Cyan
& $cargo build --release --target x86_64-pc-windows-msvc
if ($LASTEXITCODE -ne 0) { throw 'x64 build failed' }

Write-Host '==> Building ARM64 release' -ForegroundColor Cyan
& $cargo build --release --target aarch64-pc-windows-msvc
if ($LASTEXITCODE -ne 0) { throw 'ARM64 build failed' }

$dist = Join-Path $root 'dist'
New-Item -ItemType Directory -Force (Join-Path $dist 'x64') | Out-Null
New-Item -ItemType Directory -Force (Join-Path $dist 'arm64') | Out-Null
Copy-Item (Join-Path $root 'target\x86_64-pc-windows-msvc\release\beanxsound.exe') (Join-Path $dist 'x64\beanxsound.exe') -Force
Copy-Item (Join-Path $root 'target\aarch64-pc-windows-msvc\release\beanxsound.exe') (Join-Path $dist 'arm64\beanxsound.exe') -Force

$iscc = @(
    (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe'),
    'C:\Program Files (x86)\Inno Setup 6\ISCC.exe',
    'C:\Program Files\Inno Setup 6\ISCC.exe'
) | Where-Object { Test-Path $_ } | Select-Object -First 1

if (-not $iscc) { throw 'Inno Setup 6 (ISCC.exe) not found.' }

Write-Host '==> Building installer' -ForegroundColor Cyan
& $iscc '/Qp' (Join-Path $root 'installer\beanxsound.iss')
if ($LASTEXITCODE -ne 0) { throw 'Installer build failed' }

$setup = Get-ChildItem (Join-Path $dist 'beanxsound-setup-*.exe') | Sort-Object LastWriteTime -Descending | Select-Object -First 1
Write-Host "Installer ready: $($setup.FullName) ($([math]::Round($setup.Length / 1MB, 1)) MB)" -ForegroundColor Green
