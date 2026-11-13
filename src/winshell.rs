use std::ffi::c_void;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{
    GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn is_elevated() -> bool {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut returned = 0u32;
        let result = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut c_void),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        );
        let _ = CloseHandle(token);
        result.is_ok() && elevation.TokenIsElevated != 0
    }
}

fn powershell(script: &str) -> Result<(i32, String, String), String> {
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("Could not start PowerShell: {e}"))?;
    Ok((
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
        String::from_utf8_lossy(&output.stderr).trim().to_string(),
    ))
}

pub fn ensure_shortcut() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("Could not locate beanxsound.exe: {e}"))?;
    let exe_text = exe.display().to_string().replace('\'', "''");
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$exe = '{exe_text}'
$dir = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\beanxsound'
New-Item -ItemType Directory -Force $dir | Out-Null
$lnk = Join-Path $dir 'beanxsound.lnk'
$ws = New-Object -ComObject WScript.Shell
$sc = $ws.CreateShortcut($lnk)
$sc.TargetPath = $exe
$sc.WorkingDirectory = Split-Path $exe
$sc.IconLocation = "$exe,0"
$sc.Description = 'beanxsound - Windows audio control center'
$sc.Save()
Write-Output $lnk
"#
    );
    let (code, out, err) = powershell(&script)?;
    if code == 0 && !out.is_empty() {
        Ok(PathBuf::from(out.lines().last().unwrap_or_default()))
    } else {
        Err(if err.is_empty() {
            "Windows would not create the shortcut.".to_string()
        } else {
            err
        })
    }
}

/// Best-effort taskbar pinning.
///
/// Windows does not expose a supported API for this. We try the shell verb first
/// (works on Windows 10 and some Windows 11 builds), then fall back to placing a
/// shortcut in the pinned-taskbar folder and restarting Explorer.
pub fn pin_to_taskbar() -> Result<(), String> {
    let lnk = ensure_shortcut()?;
    let lnk_text = lnk.display().to_string().replace('\'', "''");
    let verb_script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$lnk = '{lnk_text}'
$shell = New-Object -ComObject Shell.Application
$folder = $shell.Namespace((Split-Path $lnk))
$item = $folder.ParseName((Split-Path $lnk -Leaf))
$done = $false
foreach ($v in $item.Verbs()) {{
    $name = $v.Name.Replace('&','')
    if ($name -match 'Pin to taskbar') {{
        $v.DoIt()
        $done = $true
        break
    }}
}}
if (-not $done) {{ exit 2 }}
"#
    );
    let (code, _, _) = powershell(&verb_script)?;
    if code == 0 {
        return Ok(());
    }

    let fallback = format!(
        r#"
$ErrorActionPreference = 'Stop'
$lnk = '{lnk_text}'
$bar = Join-Path $env:APPDATA 'Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar'
New-Item -ItemType Directory -Force $bar | Out-Null
Copy-Item -LiteralPath $lnk -Destination (Join-Path $bar 'beanxsound.lnk') -Force
Stop-Process -Name explorer -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 900
if (-not (Get-Process explorer -ErrorAction SilentlyContinue)) {{
    Start-Process explorer.exe
}}
"#
    );
    let (code, _, err) = powershell(&fallback)?;
    if code == 0 {
        Ok(())
    } else {
        Err(if err.is_empty() {
            "Windows blocked taskbar pinning. Right-click the beanxsound Start Menu entry and choose “Pin to taskbar”.".to_string()
        } else {
            format!("Windows blocked taskbar pinning ({err}). Pin it from the Start Menu instead.")
        })
    }
}

/// Remove the pinned taskbar shortcut if it exists, without touching anything else.
pub fn unpin_from_taskbar() -> Result<(), String> {
    let script = r#"
$ErrorActionPreference = 'SilentlyContinue'
$bar = Join-Path $env:APPDATA 'Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar'
$lnk = Join-Path $bar 'beanxsound.lnk'
if (Test-Path -LiteralPath $lnk) {
    Remove-Item -LiteralPath $lnk -Force
    Stop-Process -Name explorer -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 900
    if (-not (Get-Process explorer -ErrorAction SilentlyContinue)) { Start-Process explorer.exe }
}
"#;
    let (code, _, err) = powershell(script)?;
    if code == 0 {
        Ok(())
    } else {
        Err(if err.is_empty() {
            "Could not unpin beanxsound.".to_string()
        } else {
            err
        })
    }
}

pub fn relaunch_elevated() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("Could not locate beanxsound.exe: {e}"))?;
    let exe_w: Vec<u16> = exe
        .display()
        .to_string()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let verb: Vec<u16> = "runas".encode_utf16().chain(std::iter::once(0)).collect();
    let args: Vec<u16> = "--elevated".encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let result = ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(exe_w.as_ptr()),
            PCWSTR(args.as_ptr()),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
        if result.0 as isize <= 32 {
            return Err("Elevation was cancelled or blocked by Windows.".to_string());
        }
    }
    Ok(())
}
