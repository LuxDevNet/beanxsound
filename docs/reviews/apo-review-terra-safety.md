# APO safety review — 2026-11-13

This is a daily-use machine with a live THX/Razer chain. This review judges present risk and failure residue, not whether someone could later reverse it. Live registry, certificate-store, Driver Store, and filesystem reads were performed after reading `APO-FACTS-2026-11-13.md` and the two test logs.

## Current exposure

| Change made today | Live risk now | Evidence it is still there |
|---|---|---|
| `HKLM\...\MMDevices\Audio\Render\{0171121c...}\FxProperties\{d04e05a6...},13` changed from the THX CLSID to `{6F2F0B1C...}` | **High:** the active Razer/THX headphone endpoint no longer names its original THX APO chain entry. The owner can lose THX spatial processing or get a silently altered chain. | Live value is beanxsound; pre-write baseline names THX’s CLSID. |
| `HKLM\...\Render\{269fb022...}\FxProperties\{d04e05a6...},5` set to `{6F2F0B1C...}` | **Medium:** an ordinary render endpoint is pointed at experimental audio processing. If Windows begins honoring it later, sound can change without a visible application. | Live value is beanxsound. |
| `HKCR\CLSID\{6F2F...}` and `HKCR\AudioEngine\AudioProcessingObjects\{6F2F...}` registered; DLL at `C:\ProgramData\beanxsound\beanxapo.dll` | **Medium:** machine-wide COM/APO registration points audio infrastructure at a mutable, non-system DLL. It is not proven loaded, but it is callable (`CoCreateInstance` was S_OK). | Both live registry keys and the 273,408-byte DLL exist. |
| `C:\ProgramData\beanxsound` ACL granted `Authenticated Users:(M)` and `LOCAL SERVICE:(RX)` | **High:** any authenticated local user/process can modify the directory containing the registered DLL; if the audio engine later loads it, that widens the code-execution/trust boundary. | `load-test.log` records those ACLs; directory and DLL remain. |
| `LocalMachine\Root` and `LocalMachine\TrustedPublisher` received self-signed **LuxDev beanxsound Test** certificate, SHA-1 `719C101B139A704C1C5330DCFE1A345D9E1E0E92` | **High:** this is a machine trust anchor for a private signing identity, valid until 2029. It is not limited to beanxsound. | Live `certutil -store Root` and `TrustedPublisher` both show that certificate. |
| `oem44.inf` / `beanxapo.inf` added to Driver Store at `C:\Windows\System32\DriverStore\FileRepository\beanxapo.inf_amd64_a6965c71e184f879` | **Standing hazard:** an experimental, self-signed AudioProcessingObject package remains in the driver store even though it produced no device node or deployed DLL. | `pnputil` lists oem44 as Legacy, signer LuxDev beanxsound Test; INF and CAT exist in Driver Store. |
| `Audiosrv` and `AudioEndpointBuilder` restarted for tests | **No continuing service-state change found:** both reported Running afterwards. The immediate risk was interrupted audio/endpoint re-enumeration during the restart. | Both logs report the services Running after their test. |
| `HKLM\SOFTWARE\Microsoft\Windows\Audio\DisableProtectedAudioDG=1` was set during the earlier work | **Not live now:** current query finds neither the value nor its parent key. The fact file records that it was set today, so the earlier change occurred, but it is not current exposure. | Current `reg query` returns key/value absent; facts file records the earlier setting. |

## Untrusting the certificate

No. Trusting this self-signed certificate in **LocalMachine\Root** and **LocalMachine\TrustedPublisher** is not acceptable on a personal daily machine.

Root trust means Windows accepts chains rooted in that private key. TrustedPublisher means packages signed by that publisher can be installed without the normal publisher decision prompt. The same certificate is in both stores; it is not scoped to this INF, this DLL, or this repo. Whoever controls that signing key can sign another driver package or signed code package that this machine will accept under that identity until expiry. A package that appears to do nothing is still a trust-and-driver-store change.

A careful owner should require all of the following before any such trust remains: an independently controlled signing key, an inventory proving exactly which packages carry that signature, a narrowly scoped test machine rather than the daily machine, and independent verification that the package binds only to the intended test endpoint and has no OEM-chain regression. None is present here. Microsoft’s own driver guidance describes these stores for internal test systems, not a permanent personal-machine exception.

## Installed package

**Standing hazard, not harmless or cosmetic.** `oem44.inf` is retained in Driver Store and its signer remains trusted in two LocalMachine stores. The package did not create a `SWD\DRIVERENUM\*BEANX*` node, did not place `beanxapo.dll` in System32, and did not produce `apo-load.log` after the service restart and tone. Those facts show **no demonstrated audio function**, not no change. The package can be selected or inspected by later PnP/driver activity, while its signing identity remains trusted. Meanwhile the active Razer endpoint is already pointed away from THX.

## Proposed work

- **Link a component to every endpoint — unsafe.** Halfway failure can replace, reorder, or orphan OEM properties across all outputs; the owner loses known-good THX/Realtek tuning and may only notice altered spatial sound. Evidence: per-endpoint FxProperties comparison, actual component nodes, and listening tests on every endpoint. There is no safe blanket-link evidence.
- **Install a component package that Windows actually binds — unsafe.** Half-success leaves a bound third-party component, driver-store package, and trust anchor while the DLL/registration mismatch can produce silence, crashes, or a hidden no-op. Evidence: a real SWD node, installed deployed DLL, audiodg module proof, and controlled audio output—not `pnputil` success.
- **Restart audio services — conditionally unsafe.** Halfway failure can interrupt all sound, strand streams, or re-enumerate endpoints; the owner loses working playback until recovery. Evidence: both services Running, all endpoints present, and THX/Realtek playback verified afterward. “Running” alone does not prove sound fidelity.
- **Wire DSP into the APO — unsafe on this machine.** A real-time DSP bug can cause silence, distortion, latency, audiodg instability, or degrade THX processing. The current APO is pass-through and has no settings bus, so no safe integration evidence exists. Evidence must include module load, deterministic DSP bypass/enable tests, glitch/latency measurements, and THX A/B listening.
- **Later uninstall FxSound and remove FxSound Speakers — unsafe until proven unused.** Half-success removes a functioning fallback/endpoint while leaving routing, app defaults, or package remnants inconsistent. The owner loses that playback device and possibly application routing. Evidence: enumerate defaults and per-app routing before/after, confirm the endpoint is gone, and prove all wanted playback still works.

## Verdict

**STOP NOW — the single condition that matters most is: do not continue while a self-signed LocalMachine root/publisher trust anchor authorizes experimental audio-driver packages on this daily machine.**
