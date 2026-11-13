# beanxsound

A tidy, dark control room for Windows audio — devices, per-app volume, the real
FxSound DSP, and your own presets in one native Rust app.

![platform](https://img.shields.io/badge/platform-Windows%2010%2F11%20x64%20%7C%20ARM64-ff4fa3)
![language](https://img.shields.io/badge/built%20with-Rust%20%2B%20egui-f46623)

## What it does

- **Devices** — every playback and recording endpoint, live peak meters,
  per-channel volume, mute, default-device switching for console / multimedia /
  communications, and enable/disable (admin required for the last one).
- **App mixer** — Windows audio sessions with per-app volume and mute.
- **FxSound engine** — power switch, output selection, a draggable 5/10/15/20/31
  band equalizer, master gain, balance, volume leveling, filter Q, and the five
  FxSound effects (clarity, ambience, surround, dynamic boost, bass). It drives
  the installed FxSound 1.2+ engine through its official command-line bridge, so
  the DSP you already own does the processing.
- **Presets** — all built-in FxSound presets plus your own snapshots, with
  rename, delete, import, and export.
- **Advanced** — direct links to Windows sound tools, diagnostics, elevation,
  Start Menu shortcut, taskbar pinning, and uninstall info.

Everything stays reversible from Windows itself. beanxsound never bundles or
patches FxSound code.

## Themes

Nine finishes, all black glass, liquid chrome and pink light. Switch from the
dots in the top bar or **Advanced → Appearance**; the choice is saved between
launches.

`Y2K Chrome` (default) · `Liquid Metal` · `Cyber Pink` · `Bubblegum` ·
`Shanghai Neon` · `Gunmetal` · `Chrome Rouge` · `Frosted Pink` · `Hologram`

The app icon and every shortcut use the same chrome-pink mark.
`tools\make-icon.ps1` regenerates `assets\beanxsound.ico` and `icon.png`.

## Install

Run `beanxsound-setup-0.1.0.exe`. It is a single file containing both the x64
and ARM64 builds; the correct one is selected automatically.

The installer:

- installs per-user to `%LOCALAPPDATA%\Programs\beanxsound` (no UAC needed),
- creates a Start Menu shortcut,
- optionally creates a desktop shortcut,
- optionally pins beanxsound to the taskbar.

FxSound is optional. Without it, the FxSound page explains how to get it; every
other page still works.

## Build from source

Requirements: Rust stable (MSVC toolchain), Visual Studio Build Tools with the
Desktop C++ workload, and Inno Setup 6 for the installer.

```powershell
cargo build --release --target x86_64-pc-windows-msvc
cargo build --release --target aarch64-pc-windows-msvc   # needs VC Tools ARM64
.\build.ps1
```

`build.ps1` stages both executables into `dist\` and calls `ISCC.exe` to produce
the single-file installer in `dist\`.

## Notes and caveats

- Enabling or disabling audio endpoints uses the undocumented `IPolicyConfig`
  interface and needs an elevated process. Use **Advanced → Restart as
  administrator**.
- Taskbar pinning has no supported Windows API. beanxsound tries the shell verb
  first and falls back to the pinned-taskbar folder (restarting Explorer). If
  Windows still refuses, right-click the Start Menu entry and choose **Pin to
  taskbar**.
- FxSound must be running for live band and effect changes. Switching presets
  works whenever FxSound is installed.
- FxSound is a trademark of FxSound LLC. This project is independent and not
  affiliated with or endorsed by FxSound LLC.

## License

MIT
