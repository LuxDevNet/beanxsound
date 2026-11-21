# APO diagnosis: adversarial technical review (sonnet)

**Verdict up front: the diagnosis is mostly inference, not measurement.** The only hard negative is "no `apo-load.log`". That marker is written only from `Initialize`/`LockForProcess` (`crates/beanxapo/src/lib.rs:10,35,194`). There is no log in `DllGetClassObject` or at DLL attach. So the absence cannot separate "audiodg never mapped the DLL" from "mapped it, then rejected the registration or format negotiation before `Initialize`". The audiodg module list was never read. The registry was never read back from an elevated or LOCAL SERVICE context. Nothing proves where in the chain it fails, and "cannot be instantiated on 26300" has not been tested. Only `AddComponent` is newly confirmed from the files: a grep of the DriverStore found it in thxusbapo, HDX_GigabyteExt_RTK and heci INFs.

## Claim table

| # | Claim | Verdict | Deciding measurement |
|---|---|---|---|
| 1 | A bare HKCR APO cannot be instantiated on build 26300 | **UNPROVEN** (not shown; the facts file itself says no control experiment exists) | An unsigned, classically registered APO known to work (Equalizer APO) loads on this machine. Or, with a load-trace in `DllGetClassObject`/`DllMain`, audiodg is shown to map or not map our DLL. Today's evidence is one APO of ours failing for unknown reasons. Equalizer APO is also not Microsoft-signed and is widely reported to work on Win11, which argues against the claim. I have not measured that here. |
| 2 | The difference is (a) signed APO-class packaging and (b) the `{c792e395},100` reference | **UNPROVEN, and probably not the first difference** | We have not diffed against a working endpoint. Known unequal items: our `,13` (MULTI_SZ) and `,5` placement, the missing supported-modes and processing-mode keys on the endpoint, and our single CLSID where THX has stream and mode CLSIDs. Also, the Razer `,13` value was overwritten, so the endpoint had a working chain and now has ours. Also untested: audiodg DLL load policy, since audiodg is the process that loads it. The `{c792e395},100` value is evidence of association, not of necessity. THX has it because THX installs a component; a registry-only APO may never need it. |
| 3 | A self-signed, locally trusted catalog suffices, so no cert purchase | **UNPROVEN for installation, FALSE as written for this evidence** | `pnputil` printing "added successfully" shows only that the package is staged in the DriverStore. It created no devnode, and the DLL never reached System32. That is a staged package that never installed. "Sufficient" would need a devnode with `CM_PROB_NONE` plus a loaded APO. Separately, the facts file says THX and Realtek DLLs carry `SignatureAttributes.PETrust` with WHCP signatures. If audiodg enforces a signing policy on APO DLLs, a self-signed DLL fails it regardless of the catalog. The test-signing and `DisableProtectedAudioDG` setting is a separate path from catalog trust. Also, our DLL itself may be unsigned. The facts file says only that the catalog was signed. |
| 4 | Equalizer APO is the right control experiment for "is the classic path closed" | **PARTLY TRUE, but badly framed** | It is a good positive control: if it loads, the classic path is open. If it fails, you still do not know whether it is a policy block or an environment problem such as the protected-audiodg setting. It is not the best first control. The cheaper one is instrumenting our own DLL (see below). It also installs through its Configurator, which writes endpoint FxProperties and CLSIDs and may differ from our values. Capture those with a pre/post registry export of the endpoint, then diff against ours. |
| 5 | APO is the right route; a virtual device is the fallback | **UNPROVEN, and the order is unjustified** | `beanxapo` is a pass-through. It has no `beanxdsp` and no bus. An APO also must satisfy real-time, no-allocation, no-lock rules, and DSP in a hostile audiodg environment costs per-endpoint integration on three machines. A virtual device or loopback capture plus render (the FxSound approach, `Root\FXVAD` still present) already works. APO would be better for latency and per-endpoint scope. It has had zero audio, three days in. The ordering is a preference, not a result. |

## Cheapest decisive measurements

All read-only or non-elevated, and none touch registry or driver state beyond what is already installed.

1. **Did audiodg ever see our CLSID? (the missing measurement.)** Needs a rebuild with a log line in `DllGetClassObject`, plus a `DllMain` `PROCESS_ATTACH` log (including `GetCurrentProcessId`), and an unprotected audiodg. After a tone:
   `powershell -c "Get-Content C:\ProgramData\beanxsound\apo-load.log"` and `tasklist /m beanxapo.dll`.
   The `tasklist /m` check works only if you can read the module list, which failed unelevated. If so, rely on the logs. Outcomes: DLL attach but no `Initialize` means format negotiation or flags. Nothing at all means audiodg never tries our CLSID, and the endpoint registration is wrong.
2. **Diff a working endpoint against ours, in the registry.** Export before/after:
   `reg export "HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render" C:\Users\Gabe\AppData\Local\hermes\profiles\doug\cache\scratch\now.reg /y`
   Then compare the Realtek-Digital endpoint `{d53502c6}`, which has in-box `{DA2C9ECE}` in `,13`, against the `moniteerrrrsss` endpoint with ours in `,5`. Look at the sibling `{d3993a3f-...},5` mode value and the `,6`/`,7` SFX/MFX/EFX slots. You are checking whether we wrote to the right property index for a Win10+ mode-based chain.
3. **Positive control: Equalizer APO** (or any known unsigned classic APO). Download only; do not install in this step. Read its installer scripts and the registry values it writes. A passive diff avoids the install risk:
   `reg query "HKLM\SOFTWARE\Classes\AudioEngine\AudioProcessingObjects" /s`
   against a clean machine or the pre-install export. Compare the flags and interface list it registers with ours. If it uses `0xd` and the base IID like us and loads, claim 1 is dead.

## Most likely failure

The next attempt probably fails because the endpoint reference is wrong or incomplete, not because of signing. Evidence:

- We wrote our CLSID to different slots on different endpoints (`,13` on Razer, `,5` on moniteerrrrsss). Neither slot was shown to be the one audiodg reads on 26300. The facts file lists `{DA2C9ECE}` in `,13` on an in-box endpoint, so `,13` is plausibly the composite or in-box chain. Putting a pass-through, base-IID-only APO there with `NumAPOInterfaces=1` may fail the host's interface expectations. THX's CLSIDs register interfaces `{8C6A82DA...}` etc., which are not the base IID.
- Windows 10+ endpoints list supported processing modes. Our registration declares none, and `Flags 0xd` appears to be taken from the old WM LFX. That is a Win7-era shape.
- The package route will probably also fail on its own. `pnputil` staged `oem44.inf` but created no component, and our INF has no `AddComponent`. All three INFs found in the DriverStore with that directive are device-extension INFs. A standalone `SWC\` hardware ID with nothing instantiating it is inert. Under claim 3 that is the stronger reading: without `AddComponent` or a parent device, the "APO-class package" is just files in the store.

## What the diagnosis gets wrong

- It treats an absent log as a mechanism fact. The log site cannot discriminate between failure stages, and the audiodg module list was never read.
- It calls "S_OK from CoCreateInstance in an ordinary process" a good sign. That proves only that the registry and DLL are consistent for a normal client. audiodg is a different process with a different token, a different COM context and its own load policy.
- It reads `pnputil` success as installation. Staging and installing are different. No devnode and no System32 copy means nothing was installed.
- It builds the signing story on the THX/Realtek WHCP signatures. That is correlation, since those are also the only packaged APOs here.
- It overwrote the working Razer THX chain entry and has not restored it. Any further tests on that endpoint are confounded until the baseline `.reg` is reimported, which is an owner-run action.
- It plans wiring `beanxdsp` and a bus into an APO that has not once been loaded. Proving load comes first. Then comes a design for audiodg (no allocation or locks in `APOProcess`, bounded shared-memory reads), and only then the DSP.
- The "three machines" requirement is untouched. A path that depends on `DisableProtectedAudioDG` or a locally trusted cert needs that setup repeated on each machine, and it may be reset by Windows updates. That is operationally a worse fallback story than a virtual device.
