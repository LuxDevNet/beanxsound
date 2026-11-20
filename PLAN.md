# Standalone plan

beanxsound should install from one setup exe and process audio without `FxSound.exe`. This is the plan. It is not built yet.

The published app is still `v0.1.0` (`1907c88`). Devices, mixer, meters, and the loopback visualiser already use WASAPI and do not need FxSound. EQ, effects, power, and processed output still shell `FxSound.exe`.

## Install

`installer/beanxsound.iss` already builds one setup exe. It installs a single per-user `beanxsound.exe` and can show an admin prompt (`PrivilegesRequired=lowest` with an elevation override).

Before that exe is actually standalone:

- Embed the UI fonts and icon in the binary. The dev build still reads `assets/fonts` from disk, and the installer does not ship those files.
- Keep presets and UI settings in `%APPDATA%`.
- Do not ship `FxSound.exe`.

## Processing

Add an in-process engine behind the existing engine seam. It owns bypass, EQ, gain, and balance. Do not copy FxSound's DSP. The five named FxSound effects are either our own approximations or removed from the UI.

Until that engine exists, the effects page must not claim a processed output.

## System-wide audio

An admin prompt is required to register a processing stage in the Windows audio engine. The prompt is not enough.

The stage is a signed per-endpoint Audio Processing Object or a signed virtual playback device. On current Windows 10 and 11, Microsoft has to sign a driver we write or it will not load.

Signing is a paid Partner Center submission. That cost is part of this plan if we ship our own driver. It is not optional for that path.

Two ways to avoid that fee, neither of which is system-wide for free:

- Stay in user mode. Process sound and play it to a device that already exists. No driver signature. Not a replacement for FxSound.
- Bundle a driver someone else already signed. That is a license, and those files cannot be modified.

Do not turn on test signing to load an unsigned driver.

## Do not break the machine

- Register the insert only on the endpoint the user picks.
- Do not replace their audio driver.
- Do not edit or remove an existing FxSound install.
- If the insert fails to load, leave the default device alone.
- Processing off restores the previous default. It does not uninstall Windows audio.
- Uninstall removes only beanxsound's registration and restores that default.

## Order

1. Embed fonts and icon so the installed exe runs without the dev tree.
2. In-process EQ, gain, balance, and bypass. No `FxSound.exe`.
3. Decide user-mode only, or a signed system-wide insert.
4. If system-wide: pay for Microsoft signing, then add the one-time admin install and the reversible uninstall.
