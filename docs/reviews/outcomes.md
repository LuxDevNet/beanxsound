# Outcomes and standards review: NATIVE-DSP-PLAN.md

Reviewer lens: outcomes and standards. Evidence read: `docs/reviews/FACTS-2026-11-13.md`,
`docs/NATIVE-DSP-PLAN.md`, `README.md`, `src/app.rs` (FxSound tab, EQ band selector,
effects and signal rows), `src/fxsound.rs`, `src/presets.rs`, `.gitignore`.
Repo is read-only; nothing was built, installed, registered or restarted.

## Verdict

The plan picks the right mechanism and is more honest than most first drafts: it names
its riskiest assumption, gates on a spike, keeps a command-line restore path, and states
known limits (exclusive mode, ASIO, DRM). It is not, however, approvable as written.
It proves the DSP with bench tones and never proves the thing the owner actually needs,
which is that a normal day of listening, gaming and calls is uninterrupted. It carries
four defects that can end in silence on the owner's own machine with no on-machine
recovery, a five day schedule covering twenty-two tasks on a codebase that has never
compiled here, and a task table where no single task names an owner or a reviewer. Of
FxSound's twelve features in the owner's list, four are genuinely covered with a test,
seven exist in the chain but have no acceptance test or lose behavior the current UI
already has, and one (built-in presets) has no source of truth at all. "More functional"
is asserted for six capabilities, of which three have a validation task and three do
not, and the qualities that decide daily usability (no dropouts, survives device
changes, survives reboot, same settings on three computers) appear in no task. Fix the
four P0 findings and the ten required additions below and this becomes approvable;
ship it as written and the realistic outcome is a machine that is silent after an
install, an owner who cannot tell whether the sound got worse, and a schedule that was
never true.

## Feature parity against the owner's FxSound list

Rule used for the verdict: covered means a chain stage exists AND a task validates its
measurable behavior; degraded means the stage exists but has no acceptance test, or the
plan silently gives up a behavior the current app already ships; missing means no stage,
no task, or no source of data.

| FxSound feature | Plan coverage | Verdict |
|---|---|---|
| Power switch (engine on/off) | Section 4 offers "sample accurate bypass A/B" as a momentary comparison; T3.2 repeats "bypass A/B". No task defines a persisted on/off state, no task requires bit-exact pass-through when off, and the existing toggle at `src/app.rs:643` is not carried into any task. | degraded |
| 5/10/15/20/31 band graphic EQ | Section 4 gives parametric EQ to 31 bands with per-band gain/frequency/Q, which is a superset of a graphic EQ. But the band-count selector that ships today (`src/app.rs:742`, `set_band_count`, `presets::freqs_for_count`) appears in no task and in no screenshot check; T3.2 only asks for "EQ graph with draggable nodes". | degraded |
| Master gain | Chain step 1 input gain; T0.3 proves a -12 dB setting measures -12 dB +/- 1 dB in a loopback capture. | covered |
| Balance | Chain step 9 lists balance and channel trims. No task tests it: T1.3 only asserts width 0 gives L == R, which exercises mid/side, not the pan law. No law specified (constant power vs linear) and no range parity with the current -20 to +20 dB. | degraded |
| Volume leveling | Chain step 8 momentary loudness tracking plus true-peak limiter; T1.2 requires a -30 dBFS sine to reach the loudness target within 1 LU. | covered |
| Filter Q | Per-band Q replaces FxSound's single global Q, which is a superset. T1.1's response test at a stated Q exercises it. Preset migration from the global `filter_q` (`src/presets.rs:30`) is not a task. | covered |
| Clarity | Chain step 3 "dynamic presence shelf plus transient-driven harmonic synthesis". No task measures it, and T1.3 covers only stereo metrics. An exciter's character cannot be validated by a dB assertion, so parity with FxSound is unproven by construction. | degraded |
| Ambience | Chain step 5 allpass/comb network with wet/dry. T1.3's goal sentence lists ambience but its validation text tests width and energy only, so nothing asserts the ambience path does anything measurable. | degraded |
| Surround | Chain step 6 mid/side width, Haas delay, crosstalk cancellation. T1.3 tests width 0 (L == R within 1e-6) and width 2 (L+R energy within 0.5 dB), which is a real check on the width stage; crosstalk cancellation is untested but the stage is proven live. | covered |
| Dynamic boost | Chain step 7 three-band compressor with makeup gain. T1.2 validates the limiter and loudness, not this compressor: no test for ratio, attack, release or makeup gain, and no test that quiet passages actually rise. | degraded |
| Bass | Chain step 4 low shelf plus optional harmonic bass synthesis. The low shelf is exercised indirectly by T1.1's 100 Hz bin; the harmonic synthesis named in T1.3's goal has no assertion anywhere, and there is no test that small-driver bass does not clip the limiter. | degraded |
| Built-in presets | T3.4 says "own JSON, import/export, built-ins". FxSound's built-ins exist on this machine only as binary `.fac` files (`BB!.fac`, `BB! surround.fac`, per FACTS section 3), and the plan neither sources the curves, defines a parity set, nor admits the loss. | missing |

Totals: 4 covered, 7 degraded, 1 missing.

The current app already shows what parity means in the UI: five effect rows
(`src/app.rs:813` to `:817`), master gain, balance, leveling and filter Q in one Signal
card (`:832` to `:835`), band buttons 5/10/15/20/31 (`:742`), a power toggle (`:643`),
and an output device picker (`:706`). The plan replaces all of it with two task rows
(T3.2, T3.3) and a preset row (T3.4). That ratio is why seven features land in degraded.

## Is "more functional" substantiated?

Section 4 claims five specific advantages. Scored against what is actually tasked:

| Claim | Tasked? | Assessment |
|---|---|---|
| Parametric bands with Q, draggable nodes | Yes, T1.1 and T3.2 | Substantiated, and a genuine superset of a graphic EQ |
| Real gain-reduction and LUFS meters | Yes, T2.2 and T3.3 | Substantiated, with one caveat: T3.3 compares momentary LUFS to ffmpeg `ebur128` integrated, which are different measures |
| Real spectrum from a sample ring | Partly, T2.2 builds the ring, T3.3 tests LUFS only | Asserted; no task validates the spectrum |
| Per-endpoint profiles | Yes, T2.4 | Real, but it cuts against the owner's stated preference for one template everywhere, and the plan does not say the default stays one template |
| Sample-accurate bypass A/B | No | Asserted, untested, and both interleaving and click-free switching are unspecified |
| No extra routing hop, no resample, no device switch | No | Architecturally true and the best argument in the document, but unquantified: no latency measurement against the current FxSound Speakers path, so the improvement is a belief, not a result |

What the plan does not claim, and does not test, is the set of properties that decide
whether this is usable every day: no dropouts under load, no `audiodg` restarts over a
long session, correct behavior on device plug and unplug during playback, correct
behavior on sample rate changes, survival of a reboot, and identical settings on the
other two computers. "More functional" is therefore partly substantiated on features and
entirely unsubstantiated on reliability, which is the axis the owner will judge it on
after week one.

## Findings

### P0

**P0-1. The install and first-run story can leave the machine silent with no on-machine
rollback.** Section 9 promises a two second audio drop and a restore command; T4.2
promises uninstall puts the original CLSIDs back. Neither covers the actual failure: a
registered CLSID whose DLL fails to load. When the audio engine cannot create the
processing object for an endpoint, the endpoint does not quietly fall back to
unprocessed audio on every build; the common outcome is that endpoint failing to render.
The plan's only recovery is `beanxctl restore`, a binary in a repo that has never
produced a build on this machine, with its registry backups written to
`docs/evidence/`, an untracked directory inside a working tree. If the DLL does not
load, the owner has silence, no app to click (the app needs the same audio graph to be
sane to be useful), and no documented manual recovery that does not require building
Rust first. That is exactly the state the owner's "undoable from a command line" rule
exists to prevent, and it is not met. Section 9 and T4.1/T4.2 need a safe mode that does
not depend on our own binary: a pre-registration snapshot copied to a fixed path outside
the repo plus installer and uninstaller switches (`/restore`, `/safe`) that run before
anything else, and a boot-time or login-time watchdog that reverts if the endpoint has
not produced audio within N seconds of the audio service starting.

**P0-2. The dates are not real and the time contract is not a contract.** Waves 0 to 4
are dated 13 to 17 November: five days, twenty-two tasks, for a Rust DSP crate, a COM
APO with realtime constraints, a lock-free settings bus, an endpoint watcher, a template
model, a UI cutover, and a signed-off installer. Wave 0 alone, "prove an unsigned APO
loads", is a ninety minute hard stop that includes authoring a working COM object with
`IAudioProcessingObject`, `IAudioProcessingObjectRT` and
`IAudioProcessingObjectConfiguration`, compiling a cdylib, registering it and hearing
audio. FACTS section 2 states the project has never been built on this host and the
cargo registry cache is empty, so the first `cargo build` pulls the whole eframe and wgpu
tree over the network; that cost is budgeted in no task and will consume most of the
first wave by itself. Two dependencies in the plan are absent from this machine and I
re-measured them: `ISCC.exe` is not present (so T4.1 cannot be validated as written),
and there is no ARM64 MSVC compiler at
`.../14.29.30133/bin/Hostx64/arm64/cl.exe` (so T4.4's `cargo build --release --target
aarch64-pc-windows-msvc` cannot succeed as written). "Hard stop end of day" is not a time
block: no start time, no hours, no per-task minutes, and no single task names an owner or
a reviewer, while the header assigns the whole plan to "Gabe" and "agent". A plan whose
schedule is arithmetically impossible and whose owners are collective cannot be held to.

**P0-3. The load-bearing assumption is stated as measured when it was only inferred.**
Section 2 says "Confirmed on this host" and concludes "So an unsigned APO DLL,
registered per endpoint through FxProperties, loads in the audio engine." FACTS section 4
records what was actually done: downloading Equalizer APO, extracting
`EqualizerAPO.dll`, and reading `Status: NotSigned` from `Get-AuthenticodeSignature`.
That establishes the DLL is unsigned. It establishes nothing about whether `audiodg`
loads it on build 10.0.26300, and it says nothing about Equalizer APO's other install
requirements, which include its own device selection and configuration path. The plan is
allowed to bet on this, and T0.1 is the right gate, but the document must not report an
untested assumption as a verified fact, because the cost of that error is the entire
project. Separately, T0.1's own validation is self-defeating: it requires the APO to
"write a marker file and a frame counter on every APOProcess", which is file I/O inside
the realtime callback, the exact thing section 7 bans. The counter also proves the
object was created, not that it is in the signal path with real audio flowing. Use a
shared-memory counter and assert the marker alongside a loopback capture that shows
sample modification.

**P0-4. The plan deletes the working path before proving the replacement daily.** T3.1
deletes `src/fxsound.rs` and its sixteen call sites on 16 November, one day after T2.1
first makes the APO audible, and two days before any installer exists. There is no
product-level fallback: once registration is live and the bridge is gone, going back
means reverting the repo, rebuilding, re-registering, and reinstalling FxSound. A DSP
engine's real acceptance test is a week of ordinary use, and that week is not in the
plan. T3.1 must move after a soak period with the old path behind a runtime switch, and
that switch must stay through at least one release.

### P1

**P1-1. Nothing tests the property that decides daily usability.** There is no soak
test, no dropout counter, no `audiodg` restart monitor, no CPU budget in the audio
callback, no latency measurement against the current path, and no test that audio
survives a sample rate change, a device unplug mid track, a resume from sleep, or a
reboot with the app never launched. Every check in section 5 is a bench tone measured
once. Two hours of mixed content with a glitch counter and a stated ceiling (zero
dropouts, `audiodg` CPU under a fixed percentage, added latency under a fixed
millisecond figure) is the minimum that makes "usable daily" a claim instead of a hope.

**P1-2. The five effects have no acceptance test, and sonics is the whole product.**
Clarity, ambience, bass harmony and dynamic boost have no measurable assertion (see the
table). The only test that touches them is T1.3, and its validation text measures stereo
width. For an exciter and a bass enhancer, parity with FxSound is a listening question,
so the plan needs a reference-materials protocol: fixed test tracks, a recorded
pre-change reference from the current FxSound chain, a null and spectral comparison, and
a blind A/B with the owner as judge, with the pass condition written down before the
test. Without that, T3.2's "one screenshot per control showing the measured response
matching the setting" passes while the sound is worse, which is precisely the checklist
that passes while broken the owner warned about.

**P1-3. Registering every endpoint by default is an unbounded blast radius.** Section 3
enumerates all thirty render endpoints including DISABLED, UNPLUGGED and NOTPRESENT, and
T2.5 has a watcher auto-register brand new devices. Nothing specifies what happens on a
communications endpoint during a call, on an HDMI endpoint with more than two channels
(the chain is written in stereo terms: mid/side, Haas, crosstalk cancellation), on a
notification stream, or on a device the owner never chose. Nothing gives him a per
endpoint opt-out in the UI, and a device plugged in for the first time silently inherits
the `Studio` curve. "All endpoints, always" is defensible as a default only with an
explicit excluded class list, defined multichannel behavior (bypass above two channels
or an explicit downmix policy), and a visible per-endpoint binding screen.

**P1-4. Restore is only correct if nothing changed since the backup.** Backups are
point-in-time. Windows updates, driver reinstalls and endpoint property resets are
explicitly expected by the plan (section 7 and T4.3), which means the stored "original"
CLSID can be stale, and a blind restore can write an old Microsoft CLSID over a newer
one, or restore an endpoint that no longer exists. T0.2's empty `diff` proves the happy
path on one endpoint. Restore needs a per-endpoint journal with timestamps, a
restore-only-if-current-CLSID-is-ours guard, and a report of endpoints skipped because
they had drifted.

**P1-5. Who runs the privileged parts, and with what lifetime, is never stated.** Writing
`FxProperties` and restarting `Audiosrv` needs elevation (`net session` fails in this
shell, per FACTS section 2). The plan puts registration in `beanxctl`, shared by the
installer and the app, and puts the device watcher in `beanxctl` too, but never says
whether that is a service, a scheduled task, or a UAC prompt at every launch. A per-user
app cannot re-register devices after an elevation prompt is dismissed, so T2.5 is
unachievable as written for a user session. The same gap applies to the settings bus:
section 3 says the APO "reads it once per buffer", but nothing states what the APO does
when the UI has never run and `Local\beanxsound.dsp.v1` does not exist. Unspecified
answers here are either silence, a stale curve, or uninitialized gain.

**P1-6. The three-computer requirement is not designed, only implied.** Templates live in
`%ProgramData%\beanxsound\templates\` on one machine. There is no export, no sync, no
versioned template document that can be carried to the other two computers, and no task
that installs and validates on a second machine. The owner's stated standard is one
template applied everywhere with no per-device drift; the plan's answer is per-endpoint
bindings on a single host. A template file format with a stable id, an
export/import command, and a second-machine acceptance run belong in the plan, not in a
later issue.

**P1-7. Existing user data is migrated by nothing.** User presets already live in
`%APPDATA%` as JSON with a global `filter_q` and a band count (`src/presets.rs:30`,
`:148`), and `EqEditor::args` translates them into FxSound CLI arguments
(`src/presets.rs:180`). T3.4 reworks the store but never migrates it, and the mapping is
lossy by design (one global Q becomes per-band Q). Without an explicit migration with a
stated lossy rule and a backup of the old file, his saved presets either vanish or change
shape silently.

**P1-8. The proof that we do not degrade audio at all is missing, and the measurement
tooling is partly absent.** The single cheapest correctness test is a null: record a
loopback capture with the APO registered and flat, record the same content without it,
and require the residual to be below a stated floor (for example -100 dBFS). Nothing in
the plan asks for it, which means a subtle gain, phase or channel-order bug can pass
every bench test. On tooling: ffmpeg exists only as a Hermes-side binary
(`.../hermes/tools/ffmpeg-9.0.1-win32-x64/bin/ffmpeg`), not on a path a cargo run or a
task script can assume; no WASAPI loopback capture harness is tasked to exist, yet T0.3,
T2.1, T2.2, T2.4 and T2.5 all depend on one measuring "the same loopback recording".

### P2

**P2-1. Diagnostics disappear with the dependency.** `src/main.rs:207` to `:261` produces
a diagnostic dump through the FxSound bridge. T3.1 deletes the bridge and no task replaces
the dump with APO load state, registration state per endpoint, `audiodg` version and
template hashes. That dump is the only support artifact when audio breaks, and P0-1 makes
breaking audio plausible.

**P2-2. T0.2's scope contradicts section 9's scope.** T0.2 validates "backup of every
managed endpoint's FxProperties" while section 9 says the spike touches one endpoint
(the Razer headset) for blast radius control. The task and the approval request describe
different blast radii; pick one and state it once.

**P2-3. Evidence durability is unspecified.** `docs/` is untracked today and
`.gitignore` covers `target`, `dist` and `*.exe`, so text evidence would be committable,
but no task says evidence is committed, and the registry backups the restore path depends
on currently live inside that untracked tree. Recovery data must not live in a directory
a `git clean` can remove.

**P2-4. README and installer claims will be false the moment the plan lands.** README
still advertises a single file x64 plus ARM64 installer at 0.1.0 built by Inno Setup,
which is not installed here and cannot produce the ARM64 binary for lack of the compiler.
T4.4 rewrites the README but only checks that "no FxSound dependency claims" remain;
it should also check that every install claim is executable on this host.

**P2-5. Uninstall does not address a locked DLL.** `beanxapo.dll` is loaded by
`audiodg`. T4.2 asserts that audio plays unchanged after uninstall but says nothing about
the file being in use, the need to stop the audio service, or a
`PendingFileRenameOperations` fallback. This is the ordinary case, not an edge case.

## Required additions before approval

1. **Safe mode and non-dependent recovery (sections 9, T4.1, T4.2).** A registration
   snapshot written to a fixed machine path outside the repo; installer and app switches
   `/restore` and `/safe` that revert without requiring a build; a login-time watchdog
   that reverts an endpoint that has produced no audio within a stated timeout; a
   documented manual registry restore with the exact `reg add` lines in the README.
2. **A real time contract (section 5).** Replace "hard stop end of day" with dated
   blocks: start, hours, per-task minutes, and an explicit first-build budget (network
   fetch plus full eframe tree) as its own task before T0.1. State the total hours and
   the calendar span in days, not dates, until the first build has actually run.
3. **An owner and a reviewer per task (section 5).** Every row gets a named owner
   (Gabe or agent), a reviewer, and an acceptance artifact. Collective ownership is not
   accountability.
4. **Correct the foundation claim and the T0.1 check (section 2, T0.1).** Reword section
   2 to "signature verified, load not yet verified, T0.1 decides"; replace file I/O in
   the callback with a shared-memory counter; require a loopback capture proving samples
   changed, not just that a counter moved.
5. **A daily-use acceptance test (new task in wave 3 or a new wave 3.5).** A multi-hour
   soak with mixed content: zero dropouts, `audiodg` restart count zero, callback CPU
   under a stated ceiling, added latency measured against the current FxSound path, plus
   unplug, replug, sleep and resume, sample rate change, and reboot-with-app-never-run.
6. **An effect sonics protocol (T3.2, new T1.5).** Fixed reference tracks, a recorded
   reference from the current FxSound chain, null and spectral comparison per effect, and
   a blind A/B judged by the owner, with pass conditions written before the test.
   Ambience, clarity, bass harmony and dynamic boost each get a row.
7. **Feature parity rows the plan lost (T3.2).** Band count 5/10/15/20/31, a persisted
   power state with bit-exact bypass, balance with a stated pan law and range, and the
   spectrum view. Each with its own check, since T3.2's single screenshot rule does not
   cover them.
8. **Scoped endpoint policy and multichannel rules (section 3, T2.5).** An excluded class
   list (communications, notifications, more than two channels), a defined bypass above
   two channels, a visible per-endpoint binding screen, and a first-seen device policy
   that does not silently apply the default curve.
9. **Drift-safe restore (T0.2, T2.3, T4.2).** Per-endpoint journal with timestamps, a
   restore-only-if-current-CLSID-is-ours guard, and a skip report for drifted endpoints.
10. **Privilege and lifetime model (section 3, T2.3, T2.5).** State whether registration
    and the watcher run as a service, a scheduled task or a UAC prompt; state what the
    APO does when the shared memory block is absent; state what happens when the app is
    closed and a new device arrives.
11. **Cross-machine template sync (new task in wave 4).** A versioned template file with a
    stable id, `beanxctl export` and `beanxctl import`, and a second-machine install and
    verification run, since the owner has three computers and one template.
12. **User data migration and rollback (T3.4).** Import existing `%APPDATA%` presets into
    the new model with a stated lossy rule for global Q to per-band Q, back up the old
    file, and keep a documented downgrade path.
13. **A null test and a measurement harness (new T0.4, T2.2).** A flat-APO null capture
    with a stated residual floor, and a committed WASAPI loopback harness every later
    check calls, so the five tasks that say "the same loopback recording" have one real
    tool with a resolved path.
14. **Keep the FxSound path until the soak passes (T3.1).** Move the deletion after the
    daily-use acceptance test, behind a runtime switch that survives one release, and
    state the revert procedure in the README.

## Bottom line

Approve nothing until P0-1 through P0-4 are closed. The mechanism is right, the spike
gate is right, and the document is unusually candid about its limits, which is why it is
worth fixing rather than replacing. The gap is that it optimizes for a demonstration:
bench tones, screenshots, a registry diff. The owner's standard is a week of ordinary
listening on his own machine with nothing left in a state he cannot undo, and on that
standard the plan currently has no task, no owner and no evidence path.
