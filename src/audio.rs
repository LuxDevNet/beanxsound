use std::ffi::c_void;
use std::ptr::null_mut;

use windows::core::{BOOL, GUID, HRESULT, IUnknown, Interface, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, PROPERTYKEY};
use windows::Win32::Devices::FunctionDiscovery::{PKEY_Device_DeviceDesc, PKEY_Device_FriendlyName};
use windows::Win32::Media::Audio::Endpoints::{IAudioEndpointVolume, IAudioMeterInformation};
use windows::Win32::Media::Audio::{
    eCapture, eCommunications, eConsole, eMultimedia, eRender, AudioSessionState,
    AudioSessionStateActive, AudioSessionStateExpired, AudioSessionStateInactive, IAudioClient,
    IAudioSessionControl2, IAudioSessionManager2, IMMDevice, IMMDeviceCollection,
    IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator, DEVICE_STATE,
    DEVICE_STATE_ACTIVE, DEVICE_STATE_DISABLED, DEVICE_STATE_NOTPRESENT, DEVICE_STATE_UNPLUGGED,
};
use windows::Win32::System::Com::StructuredStorage::{
    PropVariantClear, PropVariantToStringAlloc, PROPVARIANT,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_APARTMENTTHREADED, STGM_READ,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

const CLSID_POLICY_CONFIG: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);
const IID_POLICY_CONFIG: GUID = GUID::from_u128(0xf8679f50_850a_41cf_9c72_430f290290c8);
const CLSID_POLICY_CONFIG_WIN10: GUID = GUID::from_u128(0x7f419449_07f6_4654_9b05_c0a2db1abf12);
const IID_POLICY_CONFIG_WIN10: GUID = GUID::from_u128(0xca286fc3_91fd_42c3_8e9b_caafa66242e3);

pub fn init_com() -> Option<String> {
    unsafe {
        // The window toolkits (winit/OLE drag-and-drop) set up a single-threaded
        // apartment on the UI thread before the app starts. WASAPI is happy in
        // either apartment, so join the existing one rather than forcing MTA
        // (which would make OleInitialize fail with RPC_E_CHANGED_MODE).
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if hr.0 < 0 && hr != HRESULT(0x80010106u32 as i32) {
            return Some(format!("COM initialization failed: 0x{:08X}", hr.0 as u32));
        }
    }
    None
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn pkey_string(store: &IPropertyStore, key: &PROPERTYKEY) -> String {
    unsafe {
        let mut pv: PROPVARIANT = match store.GetValue(key) {
            Ok(v) => v,
            Err(_) => return String::new(),
        };
        let mut result = String::new();
        if let Ok(text) = PropVariantToStringAlloc(&pv) {
            result = text.to_string().unwrap_or_default();
            CoTaskMemFree(Some(text.0 as *const c_void));
        }
        let _ = PropVariantClear(&mut pv);
        result
    }
}

fn device_id(device: &IMMDevice) -> String {
    unsafe {
        match device.GetId() {
            Ok(pw) => {
                let s = pw.to_string().unwrap_or_default();
                CoTaskMemFree(Some(pw.0 as *const c_void));
                s
            }
            Err(_) => String::new(),
        }
    }
}

pub fn default_endpoint_id(en: &IMMDeviceEnumerator, flow: windows::Win32::Media::Audio::EDataFlow, role: windows::Win32::Media::Audio::ERole) -> String {
    unsafe {
        en.GetDefaultAudioEndpoint(flow, role)
            .map(|d| device_id(&d))
            .unwrap_or_default()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Console,
    Multimedia,
    Communications,
}

impl Role {
    fn erole(self) -> windows::Win32::Media::Audio::ERole {
        match self {
            Role::Console => eConsole,
            Role::Multimedia => eMultimedia,
            Role::Communications => eCommunications,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Role::Console => "Default",
            Role::Multimedia => "Multimedia",
            Role::Communications => "Communications",
        }
    }
}

pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub description: String,
    pub is_capture: bool,
    pub state: DEVICE_STATE,
    pub default_console: bool,
    pub default_multimedia: bool,
    pub default_communications: bool,
    pub volume: f32,
    pub muted: bool,
    pub channels: u32,
    pub channel_volumes: Vec<f32>,
    pub peak: f32,
    pub sample_rate: u32,
    pub bits: u16,
    pub mix_channels: u16,
    pub volume_ctl: Option<IAudioEndpointVolume>,
    pub meter: Option<IAudioMeterInformation>,
    pub raw: IMMDevice,
}

impl AudioDevice {
    pub fn is_active(&self) -> bool {
        self.state.0 & DEVICE_STATE_ACTIVE.0 != 0
    }
    pub fn state_label(&self) -> &'static str {
        if self.state.0 & DEVICE_STATE_ACTIVE.0 != 0 {
            "Active"
        } else if self.state.0 & DEVICE_STATE_DISABLED.0 != 0 {
            "Disabled"
        } else if self.state.0 & DEVICE_STATE_UNPLUGGED.0 != 0 {
            "Unplugged"
        } else if self.state.0 & DEVICE_STATE_NOTPRESENT.0 != 0 {
            "Not present"
        } else {
            "Unknown"
        }
    }
    pub fn format_label(&self) -> String {
        if self.sample_rate == 0 {
            String::new()
        } else {
            format!(
                "{:.1} kHz · {}-bit · {} ch",
                self.sample_rate as f32 / 1000.0,
                self.bits,
                self.mix_channels
            )
        }
    }
    pub fn set_volume(&mut self, value: f32) {
        self.volume = value.clamp(0.0, 1.0);
        if let Some(ctl) = &self.volume_ctl {
            unsafe {
                let _ = ctl.SetMasterVolumeLevelScalar(self.volume, std::ptr::null());
            }
        }
    }
    pub fn set_mute(&mut self, muted: bool) {
        self.muted = muted;
        if let Some(ctl) = &self.volume_ctl {
            unsafe {
                let _ = ctl.SetMute(muted, std::ptr::null());
            }
        }
    }
    pub fn set_channel(&mut self, channel: usize, value: f32) {
        let value = value.clamp(0.0, 1.0);
        if let Some(slot) = self.channel_volumes.get_mut(channel) {
            *slot = value;
        }
        if let Some(ctl) = &self.volume_ctl {
            unsafe {
                let _ = ctl.SetChannelVolumeLevelScalar(channel as u32, value, std::ptr::null());
            }
        }
    }
    #[allow(dead_code)]
    pub fn set_all_channels(&mut self, value: f32) {
        let value = value.clamp(0.0, 1.0);
        for i in 0..self.channel_volumes.len() {
            self.channel_volumes[i] = value;
            if let Some(ctl) = &self.volume_ctl {
                unsafe {
                    let _ = ctl.SetChannelVolumeLevelScalar(i as u32, value, std::ptr::null());
                }
            }
        }
    }
}

pub struct SessionInfo {
    #[allow(dead_code)]
    pub device_id: String,
    pub device_name: String,
    pub is_capture: bool,
    pub pid: u32,
    pub process: String,
    pub display_name: String,
    pub state: AudioSessionState,
    pub is_system: bool,
    pub volume: f32,
    pub muted: bool,
    #[allow(dead_code)]
    pub instance_id: String,
    pub control: ISimpleAudioVolume,
}

impl SessionInfo {
    pub fn app_name(&self) -> String {
        if self.is_system {
            return "System sounds".to_string();
        }
        if !self.process.is_empty() {
            return self.process.clone();
        }
        if !self.display_name.is_empty() {
            return self.display_name.clone();
        }
        if self.pid == 0 {
            "System".to_string()
        } else {
            format!("PID {}", self.pid)
        }
    }
    #[allow(dead_code)]
    pub fn state_label(&self) -> &'static str {
        if self.state == AudioSessionStateActive {
            "Active"
        } else if self.state == AudioSessionStateInactive {
            "Inactive"
        } else if self.state == AudioSessionStateExpired {
            "Expired"
        } else {
            "Unknown"
        }
    }
    pub fn set_volume(&mut self, value: f32) {
        self.volume = value.clamp(0.0, 1.0);
        unsafe {
            let _ = self.control.SetMasterVolume(self.volume, std::ptr::null());
        }
    }
    pub fn set_mute(&mut self, muted: bool) {
        self.muted = muted;
        unsafe {
            let _ = self.control.SetMute(muted, std::ptr::null());
        }
    }
}

#[derive(Default)]
pub struct Snapshot {
    pub devices: Vec<AudioDevice>,
    pub sessions: Vec<SessionInfo>,
}

pub struct AudioEngine {
    enumerator: IMMDeviceEnumerator,
}

impl AudioEngine {
    pub fn new() -> Result<Self, String> {
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None::<&IUnknown>, CLSCTX_ALL)
                    .map_err(|e| format!("Could not open the Windows audio device enumerator: {e}"))?;
            Ok(Self { enumerator })
        }
    }

    pub fn refresh(&self, include_inactive: bool) -> Snapshot {
        let render_defaults = [
            default_endpoint_id(&self.enumerator, eRender, eConsole),
            default_endpoint_id(&self.enumerator, eRender, eMultimedia),
            default_endpoint_id(&self.enumerator, eRender, eCommunications),
        ];
        let capture_defaults = [
            default_endpoint_id(&self.enumerator, eCapture, eConsole),
            default_endpoint_id(&self.enumerator, eCapture, eMultimedia),
            default_endpoint_id(&self.enumerator, eCapture, eCommunications),
        ];

        let mut devices = Vec::new();
        let mut sessions = Vec::new();
        for (flow, is_capture, defaults) in [
            (eRender, false, &render_defaults),
            (eCapture, true, &capture_defaults),
        ] {
            let mask = if include_inactive {
                DEVICE_STATE(0xF)
            } else {
                DEVICE_STATE_ACTIVE
            };
            let collection: IMMDeviceCollection = match unsafe {
                self.enumerator.EnumAudioEndpoints(flow, mask)
            } {
                Ok(c) => c,
                Err(_) => continue,
            };
            let count = unsafe { collection.GetCount().unwrap_or(0) };
            for i in 0..count {
                let device = match unsafe { collection.Item(i) } {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                let info = self.read_device(device, is_capture, defaults);
                if info.is_active() {
                    sessions.extend(self.read_sessions(&info));
                }
                devices.push(info);
            }
        }

        devices.sort_by(|a, b| {
            b.is_active()
                .cmp(&a.is_active())
                .then_with(|| {
                    (b.default_console as u8 + b.default_multimedia as u8)
                        .cmp(&(a.default_console as u8 + a.default_multimedia as u8))
                })
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        sessions.sort_by(|a, b| {
            a.is_capture
                .cmp(&b.is_capture)
                .then_with(|| a.device_name.cmp(&b.device_name))
                .then_with(|| a.is_system.cmp(&b.is_system))
                .then_with(|| a.app_name().to_lowercase().cmp(&b.app_name().to_lowercase()))
        });

        Snapshot { devices, sessions }
    }

    fn read_device(
        &self,
        device: IMMDevice,
        is_capture: bool,
        defaults: &[String; 3],
    ) -> AudioDevice {
        unsafe {
            let id = device_id(&device);
            let (name, description) = match device.OpenPropertyStore(STGM_READ) {
                Ok(store) => (
                    pkey_string(&store, &PKEY_Device_FriendlyName),
                    pkey_string(&store, &PKEY_Device_DeviceDesc),
                ),
                Err(_) => (String::new(), String::new()),
            };
            let state = device.GetState().unwrap_or(DEVICE_STATE(0));
            let is_active = state.0 & DEVICE_STATE_ACTIVE.0 != 0;

            let mut info = AudioDevice {
                id: id.clone(),
                name: if name.is_empty() {
                    if description.is_empty() {
                        "Unknown audio device".to_string()
                    } else {
                        description.clone()
                    }
                } else {
                    name
                },
                description,
                is_capture,
                state,
                default_console: defaults[0] == id && !id.is_empty(),
                default_multimedia: defaults[1] == id && !id.is_empty(),
                default_communications: defaults[2] == id && !id.is_empty(),
                volume: 0.0,
                muted: false,
                channels: 0,
                channel_volumes: Vec::new(),
                peak: 0.0,
                sample_rate: 0,
                bits: 0,
                mix_channels: 0,
                volume_ctl: None,
                meter: None,
                raw: device.clone(),
            };

            if is_active {
                if let Ok(ctl) = device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) {
                    info.volume = ctl.GetMasterVolumeLevelScalar().unwrap_or(0.0);
                    info.muted = ctl.GetMute().map(|b| b.as_bool()).unwrap_or(false);
                    info.channels = ctl.GetChannelCount().unwrap_or(0);
                    for ch in 0..info.channels {
                        info.channel_volumes
                            .push(ctl.GetChannelVolumeLevelScalar(ch).unwrap_or(info.volume));
                    }
                    info.volume_ctl = Some(ctl);
                }
                if let Ok(meter) = device.Activate::<IAudioMeterInformation>(CLSCTX_ALL, None) {
                    info.peak = meter.GetPeakValue().unwrap_or(0.0);
                    info.meter = Some(meter);
                }
                if let Ok(client) = device.Activate::<IAudioClient>(CLSCTX_ALL, None) {
                    if let Ok(fmt) = client.GetMixFormat() {
                        let f = *fmt;
                        info.sample_rate = f.nSamplesPerSec;
                        info.bits = f.wBitsPerSample;
                        info.mix_channels = f.nChannels;
                        CoTaskMemFree(Some(fmt as *const c_void));
                    }
                }
            }

            info
        }
    }

    fn read_sessions(&self, device: &AudioDevice) -> Vec<SessionInfo> {
        let mut out = Vec::new();
        unsafe {
            let manager: IAudioSessionManager2 =
                match device.raw.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) {
                    Ok(m) => m,
                    Err(_) => return out,
                };
            let enumerator = match manager.GetSessionEnumerator() {
                Ok(e) => e,
                Err(_) => return out,
            };
            let count = enumerator.GetCount().unwrap_or(0);
            for i in 0..count {
                let control = match enumerator.GetSession(i) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let control2: IAudioSessionControl2 = match control.cast() {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let volume: ISimpleAudioVolume = match control.cast() {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let pid = control2.GetProcessId().unwrap_or(0);
                let is_system = control2.IsSystemSoundsSession().0 == 0;
                let display_name = match control.GetDisplayName() {
                    Ok(pw) => {
                        let s = pw.to_string().unwrap_or_default();
                        CoTaskMemFree(Some(pw.0 as *const c_void));
                        s
                    }
                    Err(_) => String::new(),
                };
                let instance_id = match control2.GetSessionInstanceIdentifier() {
                    Ok(pw) => {
                        let s = pw.to_string().unwrap_or_default();
                        CoTaskMemFree(Some(pw.0 as *const c_void));
                        s
                    }
                    Err(_) => String::new(),
                };
                if instance_id.is_empty() {
                    continue;
                }
                out.push(SessionInfo {
                    device_id: device.id.clone(),
                    device_name: device.name.clone(),
                    is_capture: device.is_capture,
                    pid,
                    process: if is_system {
                        String::new()
                    } else {
                        process_name(pid).unwrap_or_default()
                    },
                    display_name,
                    state: control.GetState().unwrap_or(AudioSessionStateInactive),
                    is_system,
                    volume: volume.GetMasterVolume().unwrap_or(0.0),
                    muted: volume.GetMute().map(|b| b.as_bool()).unwrap_or(false),
                    instance_id,
                    control: volume,
                });
            }
        }
        out
    }

    pub fn tick_meters(&self, devices: &mut [AudioDevice]) {
        for d in devices {
            if let Some(m) = &d.meter {
                if let Ok(p) = unsafe { m.GetPeakValue() } {
                    d.peak = p;
                }
            }
        }
    }

    pub fn set_default(&self, id: &str, role: Role) -> Result<(), String> {
        let idw = wide(id);
        with_policy(|vtbl, this| unsafe {
            (vtbl.SetDefaultEndpoint)(this, PCWSTR(idw.as_ptr()), role.erole())
        })
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
        let idw = wide(id);
        with_policy(|vtbl, this| unsafe {
            (vtbl.SetEndpointVisibility)(
                this,
                PCWSTR(idw.as_ptr()),
                if enabled { BOOL(1) } else { BOOL(0) },
            )
        })
    }

    #[allow(dead_code)]
    pub fn virtual_fxsound_device(&self) -> Option<String> {
        let snapshot = self.refresh(true);
        snapshot
            .devices
            .into_iter()
            .find(|d| {
                d.name.to_lowercase().contains("fxsound")
                    || d.description.to_lowercase().contains("fxsound")
            })
            .map(|d| d.name)
    }
}

fn process_name(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = vec![0u16; 1024];
        let mut len = buffer.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(handle);
        if ok.is_err() {
            return None;
        }
        let path = String::from_utf16_lossy(&buffer[..len as usize]);
        std::path::Path::new(&path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
    }
}

#[repr(C)]
#[allow(non_snake_case)]
struct PolicyConfigVtbl {
    QueryInterface: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT,
    AddRef: unsafe extern "system" fn(*mut c_void) -> u32,
    Release: unsafe extern "system" fn(*mut c_void) -> u32,
    GetMixFormat: unsafe extern "system" fn(*mut c_void, PCWSTR, *mut *mut c_void) -> HRESULT,
    GetDeviceFormat:
        unsafe extern "system" fn(*mut c_void, PCWSTR, i32, *mut *mut c_void) -> HRESULT,
    ResetDeviceFormat: unsafe extern "system" fn(*mut c_void, PCWSTR) -> HRESULT,
    SetDeviceFormat:
        unsafe extern "system" fn(*mut c_void, PCWSTR, *const c_void, *const c_void) -> HRESULT,
    GetProcessingPeriod:
        unsafe extern "system" fn(*mut c_void, PCWSTR, i32, *mut i64, *mut i64) -> HRESULT,
    SetProcessingPeriod: unsafe extern "system" fn(*mut c_void, PCWSTR, *const i64) -> HRESULT,
    GetShareMode: unsafe extern "system" fn(*mut c_void, PCWSTR, *mut i32) -> HRESULT,
    SetShareMode: unsafe extern "system" fn(*mut c_void, PCWSTR, *const i32) -> HRESULT,
    GetPropertyValue:
        unsafe extern "system" fn(*mut c_void, PCWSTR, *const PROPERTYKEY, *mut PROPVARIANT) -> HRESULT,
    SetPropertyValue:
        unsafe extern "system" fn(*mut c_void, PCWSTR, *const PROPERTYKEY, *const PROPVARIANT) -> HRESULT,
    SetDefaultEndpoint:
        unsafe extern "system" fn(*mut c_void, PCWSTR, windows::Win32::Media::Audio::ERole) -> HRESULT,
    SetEndpointVisibility: unsafe extern "system" fn(*mut c_void, PCWSTR, BOOL) -> HRESULT,
}

fn with_policy<F>(mut call: F) -> Result<(), String>
where
    F: FnMut(&PolicyConfigVtbl, *mut c_void) -> HRESULT,
{
    unsafe {
        let attempts = [
            (CLSID_POLICY_CONFIG, IID_POLICY_CONFIG),
            (CLSID_POLICY_CONFIG_WIN10, IID_POLICY_CONFIG_WIN10),
        ];
        let mut last_error = String::from("No policy configuration interface available");
        for (clsid, iid) in attempts {
            let unknown: IUnknown = match CoCreateInstance(&clsid, None::<&IUnknown>, CLSCTX_ALL) {
                Ok(u) => u,
                Err(e) => {
                    last_error = format!("{e}");
                    continue;
                }
            };
            let mut raw: *mut c_void = null_mut();
            if unknown.query(&iid, &mut raw).is_err() || raw.is_null() {
                continue;
            }
            let vtbl_ptr = *(raw as *const *const PolicyConfigVtbl);
            if vtbl_ptr.is_null() {
                continue;
            }
            let vtbl = &*vtbl_ptr;
            let hr = call(vtbl, raw);
            (vtbl.Release)(raw);
            if hr.0 >= 0 {
                return Ok(());
            }
            last_error = format!("Windows returned 0x{:08X}", hr.0 as u32);
        }
        Err(last_error)
    }
}

pub fn open_target(target: &str) {
    let operation = wide("open");
    let file = wide(target);
    unsafe {
        ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

pub fn format_percent(v: f32) -> String {
    format!("{:.0}%", (v.clamp(0.0, 1.0) * 100.0).round())
}
