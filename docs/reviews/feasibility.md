# Implementation feasibility review: beanxapo / native DSP plan

Reviewer lens: implementation feasibility. Would this work, and would it survive a year?
Host: Windows 11 build 10.0.26300, x64, non-elevated shell. All registry and process facts
below were measured read-only on 2026-11-13 with `reg query` and PowerShell `Get-Acl` /
`Get-ItemProperty` / `Get-Process`. Commands are reproduced in the Mechanics reference.

Inputs: `docs/reviews/FACTS-2026-11-13.md`, `docs/NATIVE-DSP-PLAN.md`, plus a fresh dump of
every render endpoint's `FxProperties`, its ACL, and the APO COM registrations.

## Verdict

The mechanism is real and the plan is directionally correct, but as written it would not
work on this machine and two of its load-bearing facts are measurably false. The settings
bus cannot function at all: `audiodg.exe` runs in session 0 and the app runs in session 1,
so a `Local\` named section is not shared and the APO would read nothing. The plan's core
placement claim, that every render endpoint carries the SFX CLSID at
`{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},5`, is false for exactly the four endpoints it
targets: all four carry their effects in the composite slots `,13/14/15`, and the endpoint
chosen for the blast-radius spike already has a THX APO in `,13`. The template model is
unsound as designed, because a template switch implemented as a CLSID rebind cannot take
effect without an audio-engine rebuild, so the plan's own T2.4 acceptance check cannot pass.
On top of that, the existing `panic = "abort"` release profile means one panic in the APO
takes down the audio engine for every application on the box, and the restore journal as
specified cannot prove it restored anything. None of these are reasons to abandon the APO
approach. They are reasons to spend the 90 minute spike on the right four questions instead
of the wrong one, and to fix the section namespace, the slot, the template binding and the
panic strategy before any registry write happens.

## Findings

Severity: P0 would fail outright or corrupt audio, P1 incomplete, P2 polish.

| id | sev | finding | measured evidence | required fix |
|---|---|---|---|---|
| F-01 | P0 | Settings bus is designed on a per-session namespace that the audio engine cannot see. | `audiodg.exe` PID 19512, `SessionId: 0` (Services). Review shell `SessionId: 1`. `Local\` sections are scoped per session, so a section created by the UI is invisible to audiodg. | Use `Global\beanxsound.dsp.v1` with an explicit DACL (the app's user SID plus `NT AUTHORITY\LOCAL SERVICE`), a magic/version/ABI-size header, and a bootstrap path when the section is absent. Add a cross-session assertion to T0.3. |
| F-02 | P0 | Wrong FX slot. The plan writes `{d04e05a6-...},5` (legacy SFX) on every endpoint. No active endpoint has that value. | Measured: `{0171121c...}` (Razer) has `,13={C792E395-BD30-472D-9A2F-2783D7FAF812}` and `,14={68650828-...}`. `{269fb022...}` and `{d53502c6...}` have `,13={DA2C9ECE-...}`. `{07a2e72e...}` (FxSound Speakers) has an empty `FxProperties`. No active endpoint has `,5` or `,6`. `C:\Windows\INF\oem62.inf` maps `PKEY_CompositeFX_StreamEffectClsid` to `{D04E05A6-...},13`; `oem0.inf` maps the legacy `PKEY_FX_StreamEffectClsid` to `,5`. | Determine by measurement which slot the engine honours per endpoint class, then write the slot the endpoint actually uses. Do not hardcode `,5`. Do not use the Razer endpoint as the spike device; it has a competing THX APO in the same position. |
| F-03 | P0 | Template switch implemented as a CLSID rebind is not a live operation. | `FxProperties` is consumed when the endpoint's APO graph is built. A changed CLSID requires a graph rebuild, which costs an audible interruption. T2.4 asserts two endpoints "diverge without a restart". That check cannot pass. | One CLSID for the APO. Template identity travels in the shared parameter block, keyed by endpoint GUID. Registry is written once at install. A template switch becomes a data write. Measure the rebind path anyway and document its real cost. |
| F-04 | P0 | `panic = "abort"` in an `audiodg`-hosted DLL kills audio for the whole machine. | `Cargo.toml` release profile: `opt-level 2`, `thin LTO`, `panic = "abort"`. A panic inside `APOProcess` or `Initialize` aborts the host process, which is `audiodg.exe`, not just the endpoint. | Build the cdylib with `panic = "unwind"` (separate profile or separate workspace), `catch_unwind` at every COM entry point, and a pass-through path on any error. The plan's "no unwrap in the callback" is a review rule, not a mechanism. |
| F-05 | P0 | The restore journal cannot prove restoration, and a partial restore leaves a dangling CLSID that fails silently. | `Get-Acl` on `...\Render\{ep}\FxProperties`: `BUILTIN\Administrators Allow SetValue, ReadKey` only. `NT SERVICE\Audiosrv` and `NT SERVICE\AudioEndpointBuilder` get `SetValue, CreateSubKey, CreateLink, Delete, ReadKey`; `NT SERVICE\TrustedInstaller` gets FullControl. Per-APO subkeys exist and are unmentioned: `FxProperties\{08e54ab7-...}\{Default,User,Volatile}` and `FxProperties\{45da5c30-...}\{Default,User,Volatile}`. Both `REG_SZ` and `REG_MULTI_SZ` appear in the same slot number across endpoints. | Journal with `reg export` per endpoint (byte fidelity, preserves types), plus the ACL and the full subkey tree, stored under `%ProgramData%`, not `docs/evidence/`. Restore verifies by re-exporting and comparing hashes. Administrators cannot `CreateSubKey` here, so an endpoint with no `FxProperties` key needs an ownership path, not a silent failure. |
| F-06 | P1 | Processing-mode support lists are never captured or written. A CLSID slot with no matching mode entry may not be instantiated. | Every endpoint with a CLSID slot also carries `{d3993a3f-99c2-4402-b5ec-a92a0367664b}` values whose PIDs pair with the CLSID PIDs (`5`,`6`,`7`,`11`,`12` observed), values `{C18E2F7E-933D-4965-B7D1-1EEF228D2AF3}`. Razer: `,5` and `,6`. moniteerrrrsss and Realtek Digital Output: `,5`,`6`,`7`,`11`,`12`. | Treat the mode list as part of the registration footprint, capture it in the journal, and settle whether a write is required with a falsifiable test. |
| F-07 | P1 | "One instance per endpoint" is wrong for SFX. | Stream effects are instantiated per stream, so N concurrent streams on one endpoint means N instances, not one. | Design for N instances per endpoint. All shared state lives in the parameter block keyed by template. No per-endpoint mutable state in the instance. State that in the architecture section. |
| F-08 | P1 | Realtime contract is under-specified in the ways that actually cause glitches. | Plan says "no locks, no allocation, no syscalls" but says nothing about page faults on the section view, the RT/non-RT method split, or unwinding. | Pre-fault and `VirtualLock` the whole section view in `Initialize`/`LockForProcess` (touching a cold section page in `APOProcess` can block on paging). Only `APOProcess` runs on the RT thread; `Initialize`, `LockForProcess`, `GetLatency`, `IsInputFormatSupported` are non-RT. No COM, no registry, no logging, no allocation, no `println`, no mutex, no file or event waits inside `APOProcess`. |
| F-09 | P1 | `GetLatency` is never mentioned, but the chain has real internal delay. | Section 4 includes a Haas delay, an allpass/comb ambience network, and a true-peak limiter with lookahead. Each adds delay that must be reported or A/V sync drifts. | Implement `GetLatency` in 100 ns units, accumulate the exact internal delay of every block, and assert reported latency equals measured latency. |
| F-10 | P1 | Nonlinear stages alias, and the limiter test as written certifies a wrong implementation. | T1.2 asserts "a 0 dBFS sine stays below -0.1 dBFS", which a sample-peak limiter satisfies while still exceeding 0 dBTP on intersample peaks. Harmonic synthesis (clarity, bass) generates new spectrum with no oversampling plan. | Add oversampling or band-limited generation for every nonlinear stage. Replace the peak test with a true-peak (4x oversampled) test and add an alias test. |
| F-11 | P1 | 31 cascaded f32 biquads at low frequency and high Q lose precision and can go unstable. | T1.1's flatness test is at 0 dB and a single band; neither exercises 30 Hz at Q=8 with 31 bands loaded. | Compute coefficients in f64, keep state in f64 or transposed direct form II, and add a low-frequency/high-Q stability test over minutes of silence. |
| F-12 | P1 | "All 30 endpoints, always" is far more blast radius than the goal needs. | Measured: 30 render endpoints, 4 at state 1. The rest are 4 (`NOTPRESENT`), `0x20000004`, `0x10000008` (`UNPLUGGED`), `0x10000001`. 24 of 30 cannot currently play. | Scope to ACTIVE plus UNPLUGGED, plus the `IMMNotificationClient` watcher for arrivals. Writing to `NOTPRESENT` endpoints is churn that the next driver INF application silently wipes. |
| F-13 | P1 | The CLSID pool has no lifecycle. | Nothing prevents deleting a template whose slot CLSID is still referenced by an endpoint, which leaves that endpoint pointing at an unregistered class. CoCreateInstance then fails and the endpoint silently loses processing. | Refcount slots, run a GC pass that cross-checks every endpoint's `FxProperties` before unregistering a CLSID, and expose `beanxctl doctor` with a zero-dangling assertion. |
| F-14 | P1 | Uninstall while audio plays cannot delete the DLL. | `audiodg` holds the image mapped for the lifetime of the endpoint graph. | Schedule the delete with `MoveFileEx(..., DELAY_UNTIL_REBOOT)`, remove the CLSID values last, and verify on the next boot. Document the reboot requirement in the uninstaller UI. |
| F-15 | P1 | Fail-open direction is never specified. | The plan has no statement of behaviour when the section is absent, the version mismatches, or the endpoint has no template. An APO that writes silence on error is worse than no APO. | Contract: any error, missing section, or version mismatch means bit-exact pass-through. Test it by killing the UI during playback. |
| F-16 | P1 | Nothing proves the APO is actually live at runtime. | T0.1's marker file is a spike artifact only. There is no shipping liveness signal, and no test for `Audiosrv` restart or driver reinstall. | Per-endpoint heartbeat counter in the meter block, surfaced as a status in the engine tab. Add explicit restart and reinstall tests. |
| F-17 | P1 | DLL placement under `Program Files` is asserted, not verified, and the failure is silent. | `C:\Program Files` ACL grants read to `BUILTIN\Users` and `ALL APPLICATION PACKAGES`; there is no explicit `LOCAL SERVICE` ACE. Precedent that a non-System32 path can work: the Realtek APO loads from `C:\WINDOWS\System32\DriverStore\FileRepository\realtekapo.inf_amd64_17098f132c929189\RltkAPOU64.dll`, which grants `Everyone ReadAndExecute`. | Either place the DLL where `LOCAL SERVICE` provably can read it, or add an explicit read ACE. Prove it by load, not by assumption. On failure the engine plays without effects and reports nothing. |
| F-18 | P1 | Two APO registration models coexist on this host and the plan assumes only the flat one. | `{C9453E73}` and `{13AB3EBD}` resolve to `C:\WINDOWS\System32\WMALFXGFXDSP.dll` with `ThreadingModel: Both`. `{DA2C9ECE}` resolves to the DriverStore Realtek DLL, `ThreadingModel: Both`. The THX CLSIDs `{C792E395}` and `{68650828}` are absent from `HKLM\SOFTWARE\Classes\CLSID` entirely: they are device instances of class `AudioProcessingObject` `{5989fce8-9cd0-467d-8a6a-5419e31529d4}`, installed from `oem12.inf` under `SWD\DRIVERENUM\THXAPOCOMPONENT_...`. | State which model beanxsound uses (flat `InprocServer32` is fine and is what Equalizer APO does), and verify it works on an endpoint that already has composite components, since that is every endpoint in scope. |
| F-19 | P2 | No versioning for the parameter block or the template JSON. | The header is named `.v1` but there is no stated compatibility rule when the ABI changes. | Fixed-size header with magic, version, and total byte length checked by both sides; the APO passes through on mismatch. |
| F-20 | P2 | "Users identify endpoints by name" is unsafe here. | Measured duplicates: 5 endpoints named `Speakers`, 5 named `Digital Audio (HDMI)`, 4 named `NVIDIA Output`, 3 named `U32J59x`, 2 named `LC32G5xT`. | Bind by endpoint GUID or container ID, display the friendly name plus the device instance path, and never key a profile on the name alone. |
| F-21 | P2 | The backup journal path is a repo path. | T0.2 puts the backup in `docs/evidence/`. That is fine for the spike and wrong for the product, where the journal must survive a reinstall of the app. | Product journal under `%ProgramData%\beanxsound\journal\`; spike stays in `docs/evidence/`. |
| F-22 | P2 | Latency and quality claims are unbacked. | "No resample, no device switch, which the FxSound Speakers endpoint currently costs on every sample" was never measured. "Adds no IPC latency" ignores the delay the chain itself introduces. | Measure the FxSound path before asserting it, and publish a before/after round-trip latency number. Delete the claim if it does not reproduce. |
| F-23 | P2 | FxSound teardown is not scoped. | `FxSound Speakers` `{07a2e72e...}` exists as an endpoint from driver `Root\FXVAD`, `oem40.inf`, section `DFX_Device.NT`. Removing FxSound leaves an orphan endpoint unless the device is removed too. | Add an explicit teardown task with a check that the endpoint is gone from `MMDevices\Audio\Render`. |

## Missing tasks

Each task is a gap in waves 0 to 4, with the check that proves it done. Checks run on this host.

| id | goal | falsifiable check |
|---|---|---|
| M-01 | Cross-session settings bus | Create the section from a session 1 process, read it from the APO running in `audiodg` session 0, write a gain of -12 dB, and confirm the APO's own log records the new value within 100 ms, 20 of 20 trials. Fails today with `Local\`. |
| M-02 | Slot discovery per endpoint class | Register a diagnostic APO at each of `,5`, `,7`, `,13`, `,15` on one endpoint at a time on all 4 active endpoints. Produce a table of which slot actually gets instantiated per endpoint. The spike passes only when the table has 4 rows and each row names a slot that fired. |
| M-03 | Mode-support capture and restore | Dump every `{d3993a3f-99c2-4402-b5ec-a92a0367664b}` value per endpoint before and after a write/restore cycle; the two `reg export` files are byte-identical (`sha256sum` equal). |
| M-04 | Journal v2 with fidelity proof | `beanxctl backup` then `beanxctl restore`; `reg export` of all managed endpoints plus their `FxProperties` subkeys hashes identically before and after, and the ACL string is unchanged. |
| M-05 | Fail-open contract | With music playing, kill the UI process. Output remains within 0.1 dB of the bypass capture, no dropout, no `audiodg` restart in the event log. Repeat with the section deleted and with a deliberately corrupted version header. |
| M-06 | Realtime no-allocation proof | Build the APO with a global allocator that aborts on any allocation from the RT thread. Play 10 minutes across 3 endpoints with parameter changes. Zero aborts. |
| M-07 | Latency accounting | Measure loopback impulse round trip with the APO bypassed and active. The delta equals `GetLatency`'s reported value within 1 ms, for each endpoint. |
| M-08 | Format matrix | `LockForProcess` accepts 2, 6 and 8 channel float32 at 44.1, 48 and 96 kHz and rejects anything else with a clean error. Table of 9 formats, no crash, and a canary-pattern buffer shows no out-of-bounds write. |
| M-09 | Alias and true-peak measurement | 1 kHz and 10 kHz sine through 12 dB harmonic drive, captured at 96 kHz: all images below -80 dBFS. True-peak test at 4x oversampling on a 0 dBFS sine stays below -0.1 dBTP. |
| M-10 | Numerical stability of 31 bands | 30 Hz peak, +12 dB, Q=8, 31 bands loaded: measured gain within 0.1 dB, and 10 minutes of digital silence produces no NaN, no drift, no rising noise floor. |
| M-11 | Engine restart and driver reinstall | Restart `Audiosrv` during playback: meters resume within 3 s and `beanxctl status` still reports the endpoint as live. Reinstall the Realtek driver: `beanxctl status` flags the endpoint as reset and `--repair` restores it. |
| M-12 | Slot pool lifecycle | Create 20 templates, delete 10, run `beanxctl doctor`: zero CLSIDs referenced by any endpoint are unregistered, and the freed slots are reusable. |
| M-13 | Uninstall while playing | Uninstall with music playing: audio continues, the CLSID values are gone after reboot, the DLL is gone, and `beanxctl status` reports zero managed endpoints. |
| M-14 | Template switch semantics, measured | Bind one endpoint to a second template through shared memory only: 20 of 20 switches land within 100 ms with no dropout. Then separately measure a real registry rebind and record the audio interruption length. |
| M-15 | Bitness guard | Installer refuses to register an APO whose architecture does not match `audiodg.exe`. Verified by attempting an x64 install against an ARM64 `audiodg` and getting a refusal with a clear message. |

## Mechanics reference

All values below were read on this host, read-only, non-elevated, 2026-11-13.

### Registry footprint to attach an APO to one endpoint

Hive and key: `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render\{endpoint-guid}\FxProperties`.
One value per APO slot in the property set `{d04e05a6-594b-4fb6-a80d-01af5eed7d1d}`. The slot numbers
were read out of the INFs on this machine, not from memory.

| pid | name (from `oem0.inf` / `oem11.inf` / `oem62.inf`) | type seen |
|---|---|---|
| 0 | `PKEY_FX_Association` | `REG_SZ`, e.g. `{DFF21CE1-F70F-11D0-B917-00A0C9223196}` on Razer, `{00000000-0000-0000-0000-000000000000}` on the Realtek outputs |
| 1 | `PKEY_FX_PreMixEffectClsid` | not present on this host |
| 2 | `PKEY_FX_PostMixEffectClsid` | not present on this host |
| 3 | `PKEY_FX_UserInterfaceClsid` | `REG_SZ` `{5860E1C5-F95C-4a7a-8EC8-8AEF24F379A1}` on non-active endpoints |
| 5 | `PKEY_FX_StreamEffectClsid` (legacy SFX) | `REG_SZ` `{C9453E73-8C5C-4463-9984-AF8BAB2F5447}` on non-active endpoints only |
| 6 | `PKEY_FX_ModeEffectClsid` (legacy MFX) | `REG_SZ` `{13AB3EBD-137E-4903-9D89-60BE8277FD17}` on non-active endpoints only |
| 7 | `PKEY_FX_EndpointEffectClsid` (legacy EFX) | not present on this host |
| 13 | `PKEY_CompositeFX_StreamEffectClsid` | `REG_MULTI_SZ` `{C792E395-...}` (Razer), `{DA2C9ECE-...}` (moniteerrrrsss, Realtek Digital Output) |
| 14 | `PKEY_CompositeFX_ModeEffectClsid` | `{68650828-...}` (Razer), `{A296D363-...}` (Realtek outputs) |
| 15 | `PKEY_CompositeFX_EndpointEffectClsid` | `{A29EB043-...}` (Realtek outputs) |
| 19 | `PKEY_CompositeFX_Offload_StreamEffectClsid` | `{71111103-...}` (Realtek outputs) |
| 20 | `PKEY_CompositeFX_Offload_ModeEffectClsid` | `{7121B512-...}` (Realtek outputs) |

Rules that follow from the measurements:

1. Slot numbers 5/6/7 are the legacy path and 13/14/15/19/20 are the composite path. This host
   uses the composite path on all four active endpoints and the legacy path on the inactive ones.
   A plan that writes only `,5` is unverified for every endpoint it actually cares about.
2. The same slot number carries different types on different endpoints (`REG_SZ` on the Microsoft
   SFX endpoints, `REG_MULTI_SZ` on the Razer and Realtek composite slots). A journal that stores
   values without types cannot restore them faithfully. Use `reg export`.
3. Do not touch `{d04e05a6-...},0` (`PKEY_FX_Association`) or `{d04e05a6-...},3`
   (`PKEY_FX_UserInterfaceClsid`). The first is the engine association, the second is the
   Enhancements tab property page (`mmres.dll` / `{5860E1C5-...}`).
4. `{d3993a3f-99c2-4402-b5ec-a92a0367664b}` carries the mode support lists, PIDs aligned with
   the CLSID PIDs, value `{C18E2F7E-933D-4965-B7D1-1EEF228D2AF3}` (default processing mode).
   Present on all four active endpoints. Not written by any INF on this host, so the audio
   service maintains it.
5. `FxProperties` also holds per-APO subkeys. Observed on Razer:
   `FxProperties\{08e54ab7-ff00-4b50-92bb-63d6dc1ef8ea}\{Default,User,Volatile}` and
   `FxProperties\{45da5c30-2837-4ac4-b1e2-50acc3865974}\{Default,User,Volatile}`. A backup that
   skips these loses OEM settings and leaves orphan keys on restore.
6. ACL on `FxProperties`, identical on all endpoints sampled: `TrustedInstaller` FullControl;
   `Audiosrv` and `AudioEndpointBuilder` SetValue, CreateSubKey, CreateLink, Delete, ReadKey;
   `Administrators` SetValue, ReadKey only; `Users` CreateLink, ReadKey plus QueryValues, SetValue;
   `ALL APPLICATION PACKAGES` ReadKey. Two consequences: an elevated admin can write and delete
   values but cannot create the `FxProperties` subkey on a fresh endpoint, and a standard user can
   already set values here, so the plan's "every write needs elevation" assumption in section 9 is
   unverified. The CLSID registration and the service restart still need elevation.

### COM registration of the APO class

Flat model, which is what beanxsound should use: `HKLM\SOFTWARE\Classes\CLSID\{your-guid}\InprocServer32`,
default value = full path to the DLL, plus `ThreadingModel`. Verified reference implementations on
this host both use `ThreadingModel: Both`:

- `{C9453E73-8C5C-4463-9984-AF8BAB2F5447}` to `C:\WINDOWS\System32\WMALFXGFXDSP.dll`, Both
- `{DA2C9ECE-7418-4906-B4FA-0A00B3EB88AA}` to
  `C:\WINDOWS\System32\DriverStore\FileRepository\realtekapo.inf_amd64_17098f132c929189\RltkAPOU64.dll`, Both
- `{BCDE0395-E52F-467C-8E3D-C4579291692E}` to `C:\WINDOWS\System32\MMDevApi.dll`, Both

A DLL outside `System32` is therefore proven viable, because the Realtek APO loads from the
DriverStore, but that directory grants `Everyone ReadAndExecute` while `C:\Program Files` grants
read to `BUILTIN\Users` and `ALL APPLICATION PACKAGES` only, with no explicit `LOCAL SERVICE` ACE.
Prove the load on the target path rather than assuming it, because a failed load is silent.

Driver model, also present here: APO components can be device instances of class
`AudioProcessingObject` `{5989fce8-9cd0-467d-8a6a-5419e31529d4}`, enumerated under
`HKLM\SYSTEM\CurrentControlSet\Enum\SWD\DRIVERENUM\THXAPOCOMPONENT_...`, installed from
`oem12.inf` section `THXAPO_Component_Install_Windows11`. In that model the CLSIDs
`{C792E395-...}` and `{68650828-...}` are absent from `HKLM\SOFTWARE\Classes\CLSID` and still
resolve inside the engine. beanxsound does not need this model, but must not assume the flat
model is the only one when reasoning about what an endpoint's existing graph contains.

### Realtime rules for `APOProcess`

- `APOProcess` runs on a realtime thread inside `audiodg.exe` (session 0, `LOCAL SERVICE`).
  Only `IAudioProcessingObjectRT` methods may be called from it.
- Forbidden inside it: allocation, mutexes or any blocking synchronisation, COM calls,
  registry access, file or console I/O, `println` and any formatting, event or thread waits,
  `sleep`, network, and any call that can raise or unwind.
- Buffers are non-interleaved `float32`, one plane per channel. The caller supplies a frame
  count that changes between calls. Write exactly `frames * channels`, never assume 2 channels
  or a fixed buffer size, and never assume the input and output pointers alias.
- All structure-building (coefficient computation, section mapping, pre-faulting,
  `VirtualLock`) belongs in `Initialize` or `LockForProcess` on a non-realtime thread.
  Touching a cold page of the section view inside `APOProcess` can block on paging.
- `LockForProcess` must validate channels, sample rate and format and return a failure rather
  than proceeding, because the endpoint may be 8-channel HDMI at 96 kHz.
- `GetLatency` reports the APO's own delay in 100 ns units. Any Haas, comb, allpass or limiter
  lookahead in the chain must be included.
- `audiodg` maps the DLL for the lifetime of the endpoint graph. The file cannot be replaced or
  deleted while audio is playing.
- A crash, abort or unhandled exception in the APO terminates `audiodg`, which drops audio for
  every application on the machine, not just the affected endpoint.

### Settings bus

The section must be `Global\` because `audiodg` is in session 0 and the app is in session 1.
Creating a `Global\` section requires `SeCreateGlobalPrivilege`, which a standard user token does
not hold, so the section is created by the elevated installer or by the elevated portion of the
app, with a DACL granting the interactive user and `LOCAL SERVICE`. The reader side needs a
bounded retry loop on the seqlock (an unbounded `loop` under a preempted writer is a livelock in
the audio thread) and `Acquire`/`Release` ordering on the counter. Both sides must validate magic,
version and length before using any field.

### Measurements used above

```
reg query "HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render\{0171121c-2804-4482-8f27-c0681a7a45e4}\FxProperties"
powershell -NoProfile -Command "Get-ChildItem 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render' | ForEach-Object { ... DeviceState, name, FX slots ... }"
powershell -NoProfile -Command "Get-Acl 'HKLM:\...\Render\{ep}\FxProperties'"
powershell -NoProfile -Command "Get-ItemProperty 'HKLM:\SOFTWARE\Classes\CLSID\{guid}\InprocServer32'"
powershell -NoProfile -Command "Get-Process audiodg | Select-Object Id,SessionId"
tasklist /fi "imagename eq audiodg.exe"
grep -iE "PKEY_(Composite)?FX_[A-Za-z]+ *=" C:/Windows/INF/oem0.inf C:/Windows/INF/oem11.inf C:/Windows/INF/oem62.inf
```

Endpoint inventory at review time, matching the FACTS document: 30 render endpoints, 4 at
`DeviceState 1` (`HYPERSPEED WIRELESS`, `FxSound Speakers`, `moniteerrrrsss`,
`Realtek Digital Output`). None of the four carries a legacy `,5` SFX value.
