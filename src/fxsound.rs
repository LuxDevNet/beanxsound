use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::os::windows::process::CommandExt;
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct FxStatus {
    pub version: String,
    pub power: bool,
    pub presets: FxPresets,
    pub selected_preset: String,
    pub output_devices: Vec<String>,
    pub selected_output: String,
    pub equalizer: FxEq,
    pub effects: FxEffects,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct FxPresets {
    pub built_in: Vec<FxPresetInfo>,
    pub user_defined: Vec<FxPresetInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct FxPresetInfo {
    pub name: String,
    pub modified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct FxEq {
    pub num_bands: u32,
    pub master_gain: f32,
    pub volume_leveling: f32,
    pub filter_q: f32,
    pub balance: f32,
    pub bands: Vec<FxBand>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct FxBand {
    pub index: u32,
    pub frequency: f32,
    pub gain: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct FxEffects {
    pub clarity: f32,
    pub ambience: f32,
    pub surround: f32,
    pub dynamicboost: f32,
    pub bass: f32,
}

#[derive(Debug)]
pub enum FxEvent {
    Status(Box<FxStatus>),
    NotRunning,
    Error(String),
}

enum FxRequest {
    Refresh,
    Apply(Vec<String>),
    Launch,
}

pub struct FxClient {
    tx: Sender<FxRequest>,
    rx: Receiver<FxEvent>,
    pub exe: Option<PathBuf>,
    pub status_path: PathBuf,
}

impl FxClient {
    pub fn new() -> Self {
        let exe = locate_fxsound();
        let status_path = status_path();
        let (tx, rx) = channel();
        let (event_tx, event_rx) = channel();
        let worker_exe = exe.clone();
        let worker_status = status_path.clone();
        std::thread::spawn(move || worker(worker_exe, worker_status, rx, event_tx));
        Self {
            tx,
            rx: event_rx,
            exe,
            status_path,
        }
    }

    pub fn refresh(&self) {
        let _ = self.tx.send(FxRequest::Refresh);
    }

    pub fn apply(&self, args: Vec<String>) {
        let _ = self.tx.send(FxRequest::Apply(args));
    }

    pub fn launch(&self) {
        let _ = self.tx.send(FxRequest::Launch);
    }

    pub fn poll(&self) -> Vec<FxEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.rx.try_recv() {
            events.push(event);
        }
        events
    }

    pub fn is_installed(&self) -> bool {
        self.exe.is_some()
    }
}

fn worker(
    exe: Option<PathBuf>,
    status_path: PathBuf,
    rx: Receiver<FxRequest>,
    tx: Sender<FxEvent>,
) {
    while let Ok(request) = rx.recv() {
        let Some(exe) = &exe else {
            let _ = tx.send(FxEvent::Error(
                "FxSound was not found. Install it or add FxSound.exe to a standard Program Files location."
                    .to_string(),
            ));
            continue;
        };

        match request {
            FxRequest::Refresh => {
                if !is_running() {
                    let _ = tx.send(FxEvent::NotRunning);
                    continue;
                }
                match query_status(exe, &status_path, false) {
                    Ok(status) => {
                        let _ = tx.send(FxEvent::Status(Box::new(status)));
                    }
                    Err(err) => {
                        let _ = tx.send(FxEvent::Error(err));
                    }
                }
            }
            FxRequest::Launch => {
                let _ = Command::new(exe)
                    .arg("--run_minimized")
                    .spawn()
                    .map_err(|e| format!("Could not start FxSound: {e}"));
                std::thread::sleep(Duration::from_millis(1800));
                if is_running() {
                    match query_status(exe, &status_path, true) {
                        Ok(status) => {
                            let _ = tx.send(FxEvent::Status(Box::new(status)));
                        }
                        Err(err) => {
                            let _ = tx.send(FxEvent::Error(err));
                        }
                    }
                } else {
                    let _ = tx.send(FxEvent::NotRunning);
                }
            }
            FxRequest::Apply(args) => {
                if !is_running() {
                    let _ = Command::new(exe).arg("--run_minimized").spawn();
                    std::thread::sleep(Duration::from_millis(1800));
                }
                match Command::new(exe)
                    .args(&args)
                    .creation_flags(0x08000000)
                    .spawn()
                {
                    Ok(mut child) => {
                        let _ = child.wait();
                        std::thread::sleep(Duration::from_millis(260));
                        match query_status(exe, &status_path, true) {
                            Ok(status) => {
                                let _ = tx.send(FxEvent::Status(Box::new(status)));
                            }
                            Err(err) => {
                                let _ = tx.send(FxEvent::Error(err));
                            }
                        }
                    }
                    Err(err) => {
                        let _ = tx.send(FxEvent::Error(format!(
                            "Could not talk to FxSound: {err}"
                        )));
                    }
                }
            }
        }
    }
}

fn query_status(exe: &PathBuf, path: &PathBuf, wait_for_write: bool) -> Result<FxStatus, String> {
    let before = if wait_for_write {
        std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
    } else {
        None
    };
    let mut child = Command::new(exe)
        .arg("--status")
        .creation_flags(0x08000000)
        .spawn()
        .map_err(|e| format!("Could not run FxSound: {e}"))?;
    let _ = child.wait();

    for _ in 0..16 {
        if wait_for_write {
            let now = std::fs::metadata(path).ok().and_then(|m| m.modified().ok());
            if before.is_some() && now == before {
                std::thread::sleep(Duration::from_millis(90));
                continue;
            }
        }
        match std::fs::read_to_string(path) {
            Ok(text) => match serde_json::from_str::<FxStatus>(&text) {
                Ok(status) => return Ok(status),
                Err(err) => return Err(format!("Could not read FxSound status: {err}")),
            },
            Err(_) => std::thread::sleep(Duration::from_millis(90)),
        }
    }
    Err("Timed out waiting for FxSound to report its status.".to_string())
}

pub fn locate_fxsound() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    for var in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Ok(base) = std::env::var(var) {
            candidates.push(PathBuf::from(&base).join(r"FxSound LLC\FxSound\FxSound.exe"));
            candidates.push(PathBuf::from(&base).join(r"FxSound\FxSound.exe"));
        }
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        candidates.push(
            PathBuf::from(local).join(r"Programs\FxSound LLC\FxSound\FxSound.exe"),
        );
    }
    candidates.push(PathBuf::from(r"C:\Program Files\FxSound LLC\FxSound\FxSound.exe"));
    candidates.into_iter().find(|p| p.is_file())
}

pub fn status_path() -> PathBuf {
    std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(r"FxSound\status.json")
}

pub fn is_running() -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    unsafe {
        let snapshot = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            Ok(s) => s,
            Err(_) => return false,
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = false;
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let name: String = entry
                    .szExeFile
                    .iter()
                    .take_while(|c| **c != 0)
                    .map(|c| *c as u8 as char)
                    .collect();
                if name.eq_ignore_ascii_case("fxsound.exe") {
                    found = true;
                    break;
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
        found
    }
}
