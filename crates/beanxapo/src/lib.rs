//! beanxapo: beanxsound's Audio Processing Object.
//!
//! Wave 0 spike: a pass-through SFX APO whose only job is to prove that
//! `audiodg` loads this DLL on this Windows build, and that the engine actually
//! calls `APOProcess`.
//!
//! Realtime contract, already in force here:
//!   * `APOProcess` touches atomics and nothing else. No allocation, no locks,
//!     no file IO, no logging.
//!   * All evidence is written from `Initialize`/`LockForProcess` (non-realtime)
//!     and from a monitor thread that reads the atomics, never from the callback.

#![allow(non_snake_case, clippy::missing_safety_doc)]

use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Once;

use windows::core::{implement, ComObject, GUID, HRESULT, Interface, IUnknown, Ref, Result};
use windows::Win32::Foundation::{CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_POINTER};
use windows::Win32::Media::Audio::Apo::{
    IAudioMediaType, IAudioProcessingObject, IAudioProcessingObjectConfiguration,
    IAudioProcessingObjectConfiguration_Impl, IAudioProcessingObjectRT,
    IAudioProcessingObjectRT_Impl, IAudioProcessingObject_Impl, IAudioSystemEffects,
    IAudioSystemEffects_Impl, APO_CONNECTION_DESCRIPTOR,
    APO_CONNECTION_PROPERTY, APO_FLAG, APO_FLAG_BITSPERSAMPLE_MUST_MATCH,
    APO_FLAG_FRAMESPERSECOND_MUST_MATCH, APO_FLAG_INPLACE, APO_REG_PROPERTIES,
};
use windows::Win32::System::Com::{CoTaskMemAlloc, IClassFactory, IClassFactory_Impl};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;

/// {6F2F0B1C-3C1E-4A5F-9C1D-2B7A5C0E9D41}
pub const APO_CLSID: GUID = GUID::from_u128(0x6f2f0b1c_3c1e_4a5f_9c1d_2b7a5c0e9d41);

const LOG_DIR: &str = r"C:\ProgramData\beanxsound";
const LOG_FILE: &str = r"C:\ProgramData\beanxsound\apo-load.log";

static FRAMES: AtomicU64 = AtomicU64::new(0);
static BUFFERS: AtomicU64 = AtomicU64::new(0);
static LOCKS: AtomicU64 = AtomicU64::new(0);
static RESETS: AtomicU64 = AtomicU64::new(0);
static MONITOR: Once = Once::new();
static MONITOR_STOP: AtomicBool = AtomicBool::new(false);

fn append(line: &str) {
    let _ = std::fs::create_dir_all(LOG_DIR);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(LOG_FILE)
    {
        let _ = writeln!(f, "{line}");
    }
}

fn monitor() {
    MONITOR.call_once(|| {
        std::thread::spawn(|| {
            append(&format!("monitor: started pid={}", std::process::id()));
            while !MONITOR_STOP.load(Ordering::Relaxed) {
                append(&format!(
                    "frames={} buffers={} locks={} resets={}",
                    FRAMES.load(Ordering::Relaxed),
                    BUFFERS.load(Ordering::Relaxed),
                    LOCKS.load(Ordering::Relaxed),
                    RESETS.load(Ordering::Relaxed),
                ));
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        });
    });
}

unsafe fn format_summary(format: &IAudioMediaType) -> String {
    let wf = unsafe { format.GetAudioFormat() };
    if wf.is_null() {
        return "format=<null>".to_string();
    }
    unsafe {
        // WAVEFORMATEX is packed, so read each field through addr_of to avoid
        // forming a reference to a packed field.
        let rate = std::ptr::addr_of!((*wf).nSamplesPerSec).read_unaligned();
        let ch = std::ptr::addr_of!((*wf).nChannels).read_unaligned();
        let bits = std::ptr::addr_of!((*wf).wBitsPerSample).read_unaligned();
        format!("rate={rate} ch={ch} bits={bits}")
    }
}

/// The APO itself. One instance per endpoint graph.
#[implement(
    IAudioProcessingObject,
    IAudioProcessingObjectConfiguration,
    IAudioProcessingObjectRT,
    IAudioSystemEffects
)]
#[derive(Default)]
struct PassThroughApo;

impl IAudioSystemEffects_Impl for PassThroughApo_Impl {}

impl IAudioProcessingObject_Impl for PassThroughApo_Impl {
    fn Reset(&self) -> Result<()> {
        RESETS.fetch_add(1, Ordering::Relaxed);
        append("Reset");
        Ok(())
    }

    fn GetLatency(&self) -> Result<i64> {
        Ok(0)
    }

    fn GetRegistrationProperties(&self) -> Result<*mut APO_REG_PROPERTIES> {
        // Caller frees with CoTaskMemFree.
        let mem = unsafe { CoTaskMemAlloc(std::mem::size_of::<APO_REG_PROPERTIES>()) }
            as *mut APO_REG_PROPERTIES;
        if mem.is_null() {
            return Err(windows::core::Error::from_hresult(HRESULT(0x8007000Eu32 as i32)));
        }
        unsafe {
            let mut props = APO_REG_PROPERTIES {
                clsid: APO_CLSID,
                // INPLACE | FRAMESPERSECOND_MUST_MATCH | BITSPERSAMPLE_MUST_MATCH
                // = 0xD, which is exactly what the in-box Microsoft APO
                // ({C9453E73-...}, "WM audio LFX APO") declares on this machine.
                // The spike declared 0xF by adding SAMPLESPERFRAME_MUST_MATCH and
                // no APO installed here does that.
                Flags: APO_FLAG(
                    APO_FLAG_INPLACE.0
                        | APO_FLAG_FRAMESPERSECOND_MUST_MATCH.0
                        | APO_FLAG_BITSPERSAMPLE_MUST_MATCH.0,
                ),
                u32MajorVersion: 0,
                u32MinorVersion: 1,
                u32MinInputConnections: 1,
                u32MaxInputConnections: 1,
                u32MinOutputConnections: 1,
                u32MaxOutputConnections: 1,
                u32MaxInstances: u32::MAX,
                u32NumAPOInterfaces: 1,
                ..Default::default()
            };
            // The base interface, not RT: the in-box Microsoft APO lists
            // {FD7F2B29-...} (IAudioProcessingObject) here. The spike listed the
            // RT IID ({9E1D6A6D-...}), which nothing that loads uses.
            props.iidAPOInterfaceList[0] = <IAudioProcessingObject as windows::core::Interface>::IID;
            let name: Vec<u16> = "beanxsound Engine".encode_utf16().collect();
            for (dst, src) in props.szFriendlyName.iter_mut().zip(name.iter()) {
                *dst = *src;
            }
            let copyright: Vec<u16> = "MIT".encode_utf16().collect();
            for (dst, src) in props.szCopyrightInfo.iter_mut().zip(copyright.iter()) {
                *dst = *src;
            }
            mem.write(props);
        }
        Ok(mem)
    }

    fn Initialize(&self, cbdatasize: u32, _pbydata: *const u8) -> Result<()> {
        append(&format!(
            "Initialize: pid={} data_bytes={}",
            std::process::id(),
            cbdatasize
        ));
        monitor();
        Ok(())
    }

    fn IsInputFormatSupported(
        &self,
        _opposite: Ref<IAudioMediaType>,
        requested: Ref<IAudioMediaType>,
    ) -> Result<IAudioMediaType> {
        // Accept whatever the engine offers: hand the requested format back.
        match requested.ok() {
            Ok(f) => Ok(f.clone()),
            Err(_) => Err(windows::core::Error::from_hresult(HRESULT(1))),
        }
    }

    fn IsOutputFormatSupported(
        &self,
        _opposite: Ref<IAudioMediaType>,
        requested: Ref<IAudioMediaType>,
    ) -> Result<IAudioMediaType> {
        match requested.ok() {
            Ok(f) => Ok(f.clone()),
            Err(_) => Err(windows::core::Error::from_hresult(HRESULT(1))),
        }
    }

    fn GetInputChannelCount(&self) -> Result<u32> {
        Ok(2)
    }
}

impl IAudioProcessingObjectConfiguration_Impl for PassThroughApo_Impl {
    fn LockForProcess(
        &self,
        u32numinputconnections: u32,
        ppinputconnections: *const *const APO_CONNECTION_DESCRIPTOR,
        u32numoutputconnections: u32,
        ppoutputconnections: *const *const APO_CONNECTION_DESCRIPTOR,
    ) -> Result<()> {
        LOCKS.fetch_add(1, Ordering::Relaxed);
        let mut detail = String::new();
        unsafe {
            if !ppinputconnections.is_null() && u32numinputconnections > 0 {
                let desc = *ppinputconnections;
                if !desc.is_null() {
                    let d = &*desc;
                    detail.push_str(&format!("in_max_frames={} ", d.u32MaxFrameCount));
                    if let Some(f) = d.pFormat.as_ref() {
                        detail.push_str(&format_summary(f));
                    }
                }
            }
        }
        let _ = ppoutputconnections;
        append(&format!(
            "LockForProcess: pins={u32numinputconnections} pouts={u32numoutputconnections} {detail}"
        ));
        Ok(())
    }

    fn UnlockForProcess(&self) -> Result<()> {
        append("UnlockForProcess");
        Ok(())
    }
}

impl IAudioProcessingObjectRT_Impl for PassThroughApo_Impl {
    fn APOProcess(
        &self,
        u32numinputconnections: u32,
        ppinputconnections: *const *const APO_CONNECTION_PROPERTY,
        _u32numoutputconnections: u32,
        _ppoutputconnections: *mut *mut APO_CONNECTION_PROPERTY,
    ) {
        // Realtime: atomics only.
        BUFFERS.fetch_add(1, Ordering::Relaxed);
        if ppinputconnections.is_null() || u32numinputconnections == 0 {
            return;
        }
        unsafe {
            let prop = *ppinputconnections;
            if prop.is_null() {
                return;
            }
            FRAMES.fetch_add((*prop).u32ValidFrameCount as u64, Ordering::Relaxed);
        }
    }

    fn CalcInputFrames(&self, u32outputframecount: u32) -> u32 {
        u32outputframecount
    }

    fn CalcOutputFrames(&self, u32inputframecount: u32) -> u32 {
        u32inputframecount
    }
}

/// Class factory, so `CoCreateInstance` finds an APO for our CLSID.
#[implement(IClassFactory)]
struct ApoFactory;

impl IClassFactory_Impl for ApoFactory_Impl {
    fn CreateInstance(
        &self,
        punkouter: Ref<IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut core::ffi::c_void,
    ) -> Result<()> {
        if !punkouter.is_null() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let apo: IAudioProcessingObject = PassThroughApo.into();
        let unknown: IUnknown = apo.cast()?;
        unsafe { unknown.query(riid, ppvobject).ok() }
    }

    fn LockServer(&self, _flock: windows::core::BOOL) -> Result<()> {
        Ok(())
    }
}

#[no_mangle]
pub unsafe extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut core::ffi::c_void,
) -> HRESULT {
    if rclsid.is_null() || riid.is_null() || ppv.is_null() {
        return E_POINTER;
    }
    unsafe {
        if *rclsid != APO_CLSID {
            return CLASS_E_CLASSNOTAVAILABLE;
        }
        let factory: IClassFactory = ComObject::new(ApoFactory).into_interface();
        let unknown: IUnknown = match factory.cast() {
            Ok(u) => u,
            Err(e) => return e.code(),
        };
        unknown.query(riid, ppv)
    }
}

#[no_mangle]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    // Stay loaded for the life of the audio engine process.
    HRESULT(1)
}

fn our_dll_path() -> Option<std::path::PathBuf> {
    let mut buf = vec![0u16; 1024];
    let len = unsafe { GetModuleFileNameW(None, &mut buf) };
    if len == 0 {
        return None;
    }
    Some(std::path::PathBuf::from(String::from_utf16_lossy(
        &buf[..len as usize],
    )))
}

fn clsid_key() -> String {
    r"SOFTWARE\Classes\CLSID\{6F2F0B1C-3C1E-4A5F-9C1D-2B7A5C0E9D41}".to_string()
}

/// `regsvr32 beanxapo.dll` support, so the DLL is self-registering in the
/// conventional way. beanxctl does the same through a journal.
#[no_mangle]
pub unsafe extern "system" fn DllRegisterServer() -> HRESULT {
    match register() {
        Ok(()) => HRESULT(0),
        Err(e) => {
            append(&format!("DllRegisterServer failed: {e}"));
            HRESULT(0x80004005u32 as i32)
        }
    }
}

#[no_mangle]
pub unsafe extern "system" fn DllUnregisterServer() -> HRESULT {
    match unregister() {
        Ok(()) => HRESULT(0),
        Err(e) => {
            append(&format!("DllUnregisterServer failed: {e}"));
            HRESULT(0x80004005u32 as i32)
        }
    }
}

fn register() -> std::io::Result<()> {
    use std::process::Command;
    let dll = our_dll_path()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "cannot resolve own path"))?;
    let dll = dll.display().to_string();
    let key = clsid_key();
    let inproc = format!(r"{key}\InprocServer32");
    let steps: Vec<Vec<String>> = vec![
        vec!["add".into(), key.clone(), "/ve".into(), "/d".into(), "beanxsound Engine".into(), "/f".into()],
        vec!["add".into(), inproc.clone(), "/ve".into(), "/d".into(), dll.clone(), "/f".into()],
        vec!["add".into(), inproc, "/v".into(), "ThreadingModel".into(), "/d".into(), "Both".into(), "/f".into()],
    ];
    for args in steps {
        let status = Command::new("reg").args(&args).status()?;
        if !status.success() {
            return Err(std::io::Error::other(format!("reg {args:?} failed")));
        }
    }
    append(&format!("registered CLSID -> {dll}"));
    Ok(())
}

fn unregister() -> std::io::Result<()> {
    use std::process::Command;
    let status = Command::new("reg")
        .args(["delete", &clsid_key(), "/f"])
        .status()?;
    if !status.success() {
        return Err(std::io::Error::other("reg delete failed"));
    }
    append("unregistered CLSID");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Media::Audio::Apo::IAudioSystemEffects;

    #[test]
    fn class_factory_exposes_required_apo_interfaces_without_registration() {
        let mut raw = std::ptr::null_mut();
        unsafe {
            DllGetClassObject(&APO_CLSID, &IClassFactory::IID, &mut raw)
                .ok()
                .expect("class factory must be available");
            let factory = IClassFactory::from_raw(raw);
            let apo: IAudioProcessingObject = factory
                .CreateInstance(None)
                .expect("class factory must create an APO");
            assert!(apo.cast::<IAudioSystemEffects>().is_ok());
            assert!(apo.cast::<IAudioProcessingObjectConfiguration>().is_ok());
            assert!(apo.cast::<IAudioProcessingObjectRT>().is_ok());
        }
    }

    #[test]
    fn apo_exposes_the_system_effects_interface() {
        let apo: IAudioProcessingObject = PassThroughApo.into();
        assert!(
            apo.cast::<IAudioSystemEffects>().is_ok(),
            "Windows system-effects APOs must expose IAudioSystemEffects"
        );
    }
}

