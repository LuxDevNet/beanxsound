# beanxsound: own DSP engine, no FxSound

Goal: beanxsound becomes the audio engine itself. No FxSound process, no FxSound
virtual device, no third-party audio driver. Same feature set, plus templated
processing across every output device, parametric EQ, and meters measured in the
live audio path.

Status: REVISED after a three-lens review (adversarial, feasibility, outcomes) on
2026-11-13. Not approved for implementation. Wave 0 is the proving spike.

Owner: Gabe (approves each wave gate). Executor: agent. Host for every check: this
box, Windows 11 build 10.0.26300 x64, tailnet `no-5`. Evidence root:
`docs/evidence/`. Review artifacts and measured facts: `docs/reviews/`.

---

## 1. What the dependency actually is today

Measured on this machine, 2026-11-13. The coupling is **four files**, not one:

| Fact | Evidence |
|---|---|
| Process bridge: locates `FxSound.exe`, runs `--status` and `--run_minimized`, applies CLI args | `src/fxsound.rs:107`, `:158`, `:181`, `:241` |
| All DSP state is read out of FxSound, none computed here | `src/fxsound.rs:209` reads `%APPDATA%\FxSound\status.json` |
| 16 UI call sites drive that bridge | `src/app.rs` (`self.fx.apply`, `.launch`, `.refresh`, `.is_installed`, `.exe`, `.status_path`) |
| Editor state is translated into FxSound CLI arguments | `src/presets.rs:179` (`EqEditor::args`), `:155`, `:218` |
| `--diagnostics` dumps FxSound state | `src/main.rs:207` to `:261`, `mod fxsound` at `:5` |
| Dead helper fingerprints the FxSound endpoint | `src/audio.rs:525` (`virtual_fxsound_device`) |
| FxSound is a **virtual playback endpoint** on this box, not only a process | `{07a2e72e-a871-4b18-a503-1aa488870627}` "FxSound Speakers" in `MMDevices\Audio\Render` |
| FxSound presets are two binary `.fac` files | `%APPDATA%\FxSound\Presets\BB!.fac`, `BB! surround.fac` |

Devices and Mixer pages are already pure WASAPI (`src/audio.rs`), so the audio
plumbing stays. Only the DSP is being replaced.

## 2. The mechanism that replaces it: an APO in the endpoint graph

Windows loads DSP modules (APOs) into `audiodg.exe` once per endpoint, after the
per-app mixer and before the driver. That is where every OEM enhancement already
sits, and this box proves it: each endpoint's `FxProperties` names the CLSIDs in
its chain.

### Registry footprint, measured on the four active endpoints

Read with `reg query` and `GetValueKind` on 2026-11-13. Correction to the first
draft of this plan: **no active endpoint uses the legacy `,5` slot.**

| Endpoint | Values found |
|---|---|
| HYPERSPEED WIRELESS (Razer) | `{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},13` = `{C792E395-BD30-472D-9A2F-2783D7FAF812}` (stream effect), `,14` = `{68650828-CA0F-4196-9096-5F50931AF10A}` (mode effect), plus `{c792e395...},100` and `{68650828...},100` = `SWD\DRIVERENUM\THXAPOCOMPONENT_...`. No `,5` anywhere. |
| moniteerrrrsss | `{d04e05a6...},13/14/15/19/20`, `{d3993a3f...},5/6/7/11/12` (processing modes) |
| Realtek Digital Output | same family as above |
| FxSound Speakers | `FxProperties` key exists with **zero values**, so FxSound runs no APO on its own device |
| inactive endpoints, e.g. U32J59x | `{d04e05a6...},5` = `{C9453E73-8C5C-4463-9984-AF8BAB2F5447}`, `,6`, `,3` (legacy slots) |

Requirements that follow, each one now a plan item:

- The writer targets the **composite stream-effect slot `{d04e05a6-...},13`** when
  present, and falls back to the legacy `,5` only when `,13` is absent. Writing
  `,5` on the Razer would have been a silent no-op, which is why the first draft
  picked the single worst endpoint on the machine as its test case.
- Every other value is preserved byte for byte, including the `{d3993a3f-...}`
  processing modes, the `,100` SWD component references, and the `FxProperties`
  **subkeys** (`{08e54ab7-...}`, `{12325c6d-...}`, `{45da5c30-...}`, each with
  `Default` / `User` / `Volatile` children).
- Measured DACL on `FxProperties`: `NT SERVICE\Audiosrv` and
  `NT SERVICE\AudioEndpointBuilder` hold `SetValue, CreateSubKey, Delete`;
  `BUILTIN\Administrators` holds only `SetValue, ReadKey`; `BUILTIN\Users` holds
  `QueryValues, SetValue`. Two consequences: an elevated process **cannot create
  a subkey** under `FxProperties` (so the APO cannot keep settings in one), and
  writing a value does not need elevation at all, while restarting `Audiosrv`
  does. The journal must record value types, not just data.

### COM registration, which the first draft omitted

A CLSID in `FxProperties` is only a pointer. The engine `CoCreateInstance`s it,
so `HKLM\SOFTWARE\Classes\CLSID\{ours}\InprocServer32` must exist with the DLL
path and a threading model. Verified on the class this machine already uses:

```
{default}     : C:\WINDOWS\System32\WMALFXGFXDSP.dll
ThreadingModel: Both
```

So the DLL needs a stable path readable by the audio service, COM registration,
`DllGetClassObject` / `DllCanUnloadNow`, `DllRegisterServer` /
`DllUnregisterServer`. Install location follows the proven precedent
(`%SystemRoot%\System32\`); wave 0 tests that against
`Program Files\beanxsound\` and records which one `audiodg` actually loads.

### Signing, verified rather than assumed

Equalizer APO 1.4.2's `EqualizerAPO.dll`, extracted today with 7-Zip:

```
Get-AuthenticodeSignature .\EqualizerAPO.dll
Status: NotSigned
```

That proves a **signature is not required**. It does not prove the DLL loads in
`audiodg` on this build. Only wave 0 T0.1 proves that, and it is a hard gate.

## 3. Architecture

```
app audio (any app)
        |
        v
Windows audio engine (audiodg.exe, session 0, NT AUTHORITY\LocalService)
        |
        +-- beanxapo.dll   one instance per endpoint, in the graph
        |      reads settings from a Global shared section, writes meters back
        |      calls beanxdsp (pure Rust, no Windows types)
        v
audio driver / hardware
```

| Unit | Type | Responsibility |
|---|---|---|
| `crates/beanxdsp` | rlib | DSP: biquads, dynamics, limiter, loudness, stereo tools. No Windows types, allocation-free, unit tested offline against measured dB targets. |
| `crates/beanxapo` | cdylib `beanxapo.dll` | COM APO: `IAudioProcessingObject`, `IAudioProcessingObjectRT`, `IAudioProcessingObjectConfiguration`, via `windows` feature `Win32_Media_Audio_Apo` (verified present in `windows` 0.62.0). |
| `crates/beanxctl` | rlib + bin | Enumerate endpoints, export the journal, write and restore `FxProperties`, register COM, restart services, report status. Shared by installer and app. |
| `src/` (existing) | bin | UI. Engine page reads meters from the shared section. `src/fxsound.rs` is removed only after the soak (T4.4). |

Realtime contract inside `APOProcess`: no allocation, no locks, no registry or
file access, no logging, no panic. Panics are contained by building `beanxapo`
with `panic = "unwind"` (the workspace release profile says `panic = "abort"`,
which would terminate `audiodg` and silence every application on the machine)
plus `catch_unwind` at every COM entry point, falling back to bit-exact
pass-through.

### Settings bus

`audiodg` runs in **session 0** as `NT AUTHORITY\LocalService`; the app runs in
session 1. Measured:

```
audiodg.exe  PID 19512  Session Name: Services  Session#: 0
Audiosrv     SERVICE_START_NAME: NT AUTHORITY\LocalService
```

A `Local\` mapping is therefore invisible to the APO. Requirements:

- The section is `Global\beanxsound.dsp.v1`, created with an explicit DACL that
  grants `NT AUTHORITY\LocalService` read/write and the interactive user
  read/write. `SeCreateGlobalPrivilege` is held by admins and by LocalService.
- Fixed-size parameter block with a seqlock (writer bumps a counter before and
  after; reader retries on mismatch). Meter block plus a stereo sample ring the
  UI reads for a real spectrum.
- If the section is missing or unreadable, the APO is **bit-exact pass-through**,
  never silence, and exposes that state so the UI can say "engine loaded, no
  settings bus" rather than hang.

### Scope: every endpoint

- `beanxctl` enumerates `EnumAudioEndpoints(eRender, ACTIVE | DISABLED | UNPLUGGED
  | NOTPRESENT)` and manages every endpoint it is allowed to touch. This box has
  30 render endpoints, 4 active.
- Registration persists per device GUID, so replugging the same headset reuses it.
- New hardware is caught by an `IMMNotificationClient` watcher and registered on
  arrival.
- Any endpoint can be excluded, and an excluded endpoint must measure bit
  identical to bypass.

### Templates, not per-device copies

- A **template** is one named parameter block, stored once in
  `%ProgramData%\beanxsound\templates\<id>.json`. It is what the UI edits.
- An **endpoint binding** says which template an endpoint uses. Nothing is copied
  onto the endpoint.
- Default install: one template bound to every endpoint. Drag a band once and the
  headset, the monitors and the Realtek output all follow, live, in one write to
  the shared section.
- **Live template switching works. Live per-device assignment does not.** The APO
  never learns which endpoint it belongs to (the engine hands it buffers only),
  so per-device assignment can only be expressed as a distinct CLSID, and
  changing a CLSID rebuilds that endpoint's graph, which is a short audible gap.
  The UI says that instead of promising it away; it is a deliberate settings
  action, not something that happens under the user.
- Wave 0 evaluates `IAudioProcessingObjectNotifications` (Windows 11) as a way to
  identify the endpoint inside the APO, which would remove the rebind cost in a
  later version. Not on the critical path.

## 4. DSP chain

1. Input gain, DC blocker.
2. Parametric EQ: up to 31 bands, per-band frequency / gain / Q, peaking, low
   shelf, high shelf, high pass, low pass, plus a graphic mode for FxSound parity
   at 5 / 10 / 15 / 20 / 31 bands.
3. Clarity: dynamic presence shelf plus transient harmonic synthesis.
4. Bass: low shelf plus optional harmonic bass synthesis.
5. Ambience: short allpass/comb network, wet/dry.
6. Surround: mid/side width, Haas delay, crossfeed for headphones.
7. Dynamic boost: 3-band compressor with makeup gain.
8. Volume leveling: momentary loudness tracking with true-peak limiter.
9. Balance, per-channel trims, mono downmix.

FxSound parity list, each with its own acceptance test in T3.3: power switch,
graphic EQ at 5/10/15/20/31 bands, master gain, balance, volume leveling, filter
Q, clarity, ambience, surround, dynamic boost, bass, built-in presets. Beyond
that: parametric Q control, gain reduction and LUFS measured in the path, a real
spectrum, per-endpoint opt-out, and sample-accurate A/B.

## 5. Waves

Owner: agent executes, Gabe approves each gate. The Rust toolchain is at
`%USERPROFILE%\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin` and is not on
the bash PATH.

### Wave 0, baseline and proof (13 Nov, 2 hour hard stop)

| id | goal | validation |
|---|---|---|
| T0.0 | First build of this project ever on this machine, so later failures are not mistaken for new ones | `cargo build --release` exits 0; `docs/evidence/build-baseline.txt` records elapsed time and missing prerequisites (Inno Setup and ARM64 MSVC tools are absent today) |
| T0.1 | Prove an unsigned APO loads in `audiodg` on this build | pass-through APO writes a marker and a loop counter in `Initialize` / `LockForProcess` (never in `APOProcess`) and counts frames into the shared section. Check: marker exists and frame counter is above 0 after 10 s of playback. Evidence: `docs/evidence/apo-load.txt` |
| T0.2 | Prove registration and restore are complete and reversible | `reg export` the whole `Render` subtree, apply, restore, `reg export` again, `diff` byte identical. Negative test: mutate one value before restoring and confirm the diff fails. Evidence: `docs/evidence/fxprops-roundtrip.txt` |
| T0.3 | Prove the settings bus crosses the session boundary and is audible | write our CLSID at the slot the Razer actually uses (`{d04e05a6...},13`), set master gain through the section, measure a 1 kHz tone by WASAPI loopback: -12 dB measures 12 dB +/- 1 dB below the 0 dB capture. Evidence: `docs/evidence/gain-sweep.txt` |
| T0.4 | Decide install path, slot strategy and endpoint identification from evidence | `docs/evidence/wave0-decisions.md`: DLL path that loads, chosen slot per endpoint, whether `IAudioProcessingObjectNotifications` is usable, measured rebind gap |

Stop condition: T0.1 or T0.3 failing means the native path is unproven; stop and
re-plan rather than build waves 1 to 4 on it.

### Wave 1, engine (14 to 15 Nov, hard stop 15 Nov 18:00)

| id | goal | validation |
|---|---|---|
| T1.1 | `beanxdsp`: biquad parametric EQ, shelves, filters | `cargo test -p beanxdsp`: 0 dB gain is flat within 0.05 dB; +6 dB at 1 kHz measures 6 dB +/- 0.2 dB at 1 kHz and within 0.2 dB of 0 at 100 Hz |
| T1.2 | Compressor, limiter, loudness | 0 dBFS sine stays under -0.1 dBFS with the limiter on; -30 dBFS sine reaches the loudness target within 1 LU |
| T1.3 | Stereo tools: width, Haas, crossfeed, bass harmony, ambience | width 0 gives L == R within 1e-6; width 2 keeps L+R energy within 0.5 dB |
| T1.4 | Offline harness: WAV in, WAV out, measured response table | `cargo run -p beanxdsp --example sweep -- in.wav out.wav` writes 31 measured rows to `docs/evidence/eq-response.csv` |
| T1.5 | Realtime audit of the crate | no `Vec`, `Mutex`, `unwrap`, `expect` or `println` in the processing path: the grep in `docs/evidence/rt-audit.txt` returns nothing |

### Wave 2, APO and control (16 to 18 Nov, hard stop 18 Nov 18:00)

| id | goal | validation |
|---|---|---|
| T2.1 | COM APO: registration, InprocServer32, unwind safety, pass-through on error | +12 dB at 60 Hz measures a 12 dB rise at 60 Hz in a loopback FFT; with the settings bus killed, output is bit identical to bypass. Evidence: `docs/evidence/eq-live.txt`, `passthrough.txt` |
| T2.2 | `Global\` section with DACL, seqlock, meters, sample ring | a gain change lands in loopback within 100 ms, 20 of 20 trials; with the app closed the APO still passes audio |
| T2.3 | `beanxctl`: enumerate, typed journal with subkeys, write composite slot, restore, status | `beanxctl status` lists every managed endpoint with slot, original CLSID and value type; `beanxctl restore` reproduces a byte-identical `reg export` (same check as T0.2) |
| T2.4 | Templates: one block per template, live content edits reach every bound endpoint | one template on two endpoints, change one band, both captures show the new dB within 100 ms; rebinding one endpoint to a second template produces the documented gap, then divergent responses |
| T2.5 | Register-all plus device watcher (`IMMNotificationClient`) | plug a USB audio device while the app runs: `beanxctl status` lists it as managed within 5 s; unplug leaves the others untouched |
| T2.6 | Multi-channel, communications-role and opt-out policy | an 8-channel endpoint processes every channel with no channel dropped; an excluded endpoint measures bit identical to bypass |
| T2.7 | Emergency paths: `--safe-mode` (no APO dependency), `--dsp-off`, and a Desktop `.reg` restore written before the first ever write | kill the app mid-playback and run `beanxsound --safe-mode`: audio is unchanged and the journal still restores byte identically |

### Wave 3, engine UI and parity (19 to 20 Nov, hard stop 20 Nov 18:00)

| id | goal | validation |
|---|---|---|
| T3.1 | Engine page: EQ graph with draggable nodes, effects, A/B bypass, template selector | one screenshot per control with the measured response matching the setting, `docs/evidence/ui-*.png` |
| T3.2 | Meters from the real stream: peak, LUFS, gain reduction, spectrum | UI LUFS within 1 LU of `ffmpeg -af ebur128` on the same loopback capture |
| T3.5 | Engine events through the existing toast system (`src/app.rs:151` `Toast`, `:255` `fn toast`, `:1541` `toast_ui`), no new notification path | registration failed, engine loaded with no settings bus, endpoint reset by an update, safe mode active, template rebound: each fires a toast and the same string lands in `docs/evidence/toasts.md` with the code path that raises it |
| T3.3 | FxSound parity, one test per feature | `docs/evidence/parity.md`, one row per feature, each with the number that proves it |
| T3.4 | Preset store: own JSON, import/export, built-ins, rename, delete | save, restart, preset intact; exported file re-imports to an identical state; `.fac` import explicitly not supported, and the UI says so |

### Wave 4, installer, repair, soak (21 to 25 Nov)

| id | goal | validation |
|---|---|---|
| T4.1 | Installer: per-machine DLL and COM registration, per-user app, explicit consent screen for the audio graph change | install from a clean state; `reg query` shows our CLSID at the composite slot on every chosen endpoint; uninstall restores the journal byte identically |
| T4.2 | Repair after Windows updates or driver reinstall | `beanxsound --repair` reports which endpoints were reset and re-registers only those |
| T4.3 | Soak: three days of normal use with dropout and engine-restart counters | `docs/evidence/soak.md`: zero dropouts attributable to the APO, zero `audiodg` restarts, latency measured against the current FxSound path |
| T4.4 | Remove the FxSound bridge, last step of the project | delete `src/fxsound.rs`, the `main.rs` diagnostics section, `presets.rs:179`, `audio.rs:525`; `grep -ri fxsound src/` returns nothing; FxSound uninstalled, `FxSound Speakers` removed, real device set default |
| T4.5 | README, version 0.2.0, ARM64 build | `cargo build --release --target aarch64-pc-windows-msvc` succeeds (ARM64 MSVC tools are absent today) and no README claim of FxSound dependence remains |

Schedule honesty: 26 tasks across 13 to 25 Nov including the first build of a
project that has never been built here. A failed gate stops the wave and reports;
it does not slide into the next.

## 6. Budget and stop conditions

- Waves 0 and 2 run serial on a strong model: COM, the audio graph and the
  registry, all against one binary.
- Wave 1 may run two workers in parallel (pure DSP, no shared files).
- One rebuild per failing check, then report. No loops.
- Hard stops as dated. Wave 0's 2 hours are not extended the same day.

## 7. Risks

| Risk | If it lands | Control |
|---|---|---|
| `audiodg` refuses the DLL on this build | Native path dies | T0.1 gate before any other work |
| A panic in the APO kills `audiodg` | Every app on the machine loses audio | `panic=unwind` plus `catch_unwind` at COM entries plus bit-exact pass-through, checked by killing the bus and confirming audio survives |
| Settings section invisible across the session boundary | Engine loads and does nothing | T0.3 measures the gain change through the real path, not in a unit test |
| Wrong slot or partial restore | Silent no-op, or a dangling CLSID after uninstall | composite slot detection, typed journal, byte-identical export round-trip, negative test |
| Machine left silent with no rollback | Worst outcome on a daily-use box | `--safe-mode`, `--dsp-off`, a Desktop `.reg` restore written before the first write, pass-through when the section is missing |
| Windows update resets endpoint properties | DSP vanishes silently | T4.2 repair that reports what it reset |

Known limits, stated in the README rather than hidden: exclusive-mode and ASIO
apps bypass the engine; DRM-protected streams may route around an unsigned APO;
a driver reinstall resets endpoint properties.

## 8. Alternatives rejected

| Option | Why not |
|---|---|
| Process-loopback capture plus re-render | Cannot remove the original audio: the per-session mute that stops the double feed also silences that app's own loopback capture. Adds drift correction and 20 to 60 ms. |
| VB-Cable / VoiceMeeter | Replaces one vendor dependency with another, plus a second virtual device and a resample hop. |
| Own signed audio driver | Needs a certificate, an INF, WHQL or attestation and weeks. Worse outcome than an APO. |
| Keep FxSound, reskin only | Does not meet the ask. |
| Only toggle Windows' own enhancements | No third party, but a subset of FxSound. Kept as the zero-admin degraded mode if every other path fails. |

## 9. Before wave 0 runs

The spike changes the audio graph, so it needs a go. What happens:

- One value on one endpoint first (the Razer, at `{d04e05a6...},13`, the slot it
  actually uses), then `Audiosrv` and `AudioEndpointBuilder` restart once, which
  drops audio for about 2 s.
- A full `reg export` of every render endpoint is written to
  `docs/evidence/fxprops-baseline-2026-11-13.txt`, and a double-clickable `.reg`
  restore lands on the Desktop, before the first write.
- Restore is `beanxctl restore` or that `.reg` file, and neither needs elevation.

## 10. What the review changed

Three reviewers ran against measured facts on 2026-11-13. Three findings changed
the design rather than the wording:

1. **The slot was wrong.** The first draft wrote `{d04e05a6-...},5` and claimed
   every endpoint had it. Measured: none of the active four do. The writer now
   targets `,13` with a `,5` fallback, and the spike uses the Razer's `,13`.
2. **The section name was wrong.** `Local\` cannot cross into session 0, so the
   APO would have read nothing. Now `Global\` with a DACL, plus a pass-through
   rule when it is absent.
3. **Live per-device template switching was impossible.** A CLSID change rebuilds
   the endpoint graph, so per-device assignment is a settings action with a short
   audible gap; live switching applies to template content, which is the case that
   matters.

Also folded in: COM `InprocServer32` as its own task, `panic=unwind` for the DLL,
the measured fact that admins cannot create `FxProperties` subkeys, the export
round-trip with a negative test, the emergency `--safe-mode` and `.reg` paths,
soak before the FxSound bridge is deleted, a first-build task, and dates that
survive the task count.
