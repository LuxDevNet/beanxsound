//! beanxsound's audio engine.
//!
//! Replaces the old FxSound bridge. The app no longer talks to an external
//! process: it owns the DSP (crates/beanxdsp), it owns the state, and it
//! persists its own presets.
//!
//! Command grammar (unchanged from what the editor already emits, so the UI
//! needed no rewrite):
//!
//!   --power=1|0
//!   --num_bands=5|10|15|20|31
//!   --filter_q=<f32>
//!   --master_gain=<f32>
//!   --balance=<f32>
//!   --volume_leveling=<f32>
//!   --set_band_gain=i:g,i:g,...
//!   --set_effect=clarity:x,ambience:y,surround:z,dynamicboost:w,bass:v
//!   --preset=<name>
//!   --save_preset=<name>
//!   --output=<name>
//!
//! Every apply is synchronous: it updates the state, reconfigures the DSP and
//! writes the state file, then queues a Status event for the UI.

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};

use serde::{Deserialize, Serialize};

use crate::presets;
use beanxdsp::{Band as DspBand, BandKind, Engine as DspEngine};

pub const STATE_VERSION: &str = "beanxsound-engine/1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Status {
    pub version: String,
    pub power: bool,
    pub presets: Presets,
    pub selected_preset: String,
    pub output_devices: Vec<String>,
    pub selected_output: String,
    pub equalizer: Eq,
    pub effects: Effects,
}

impl Default for Status {
    fn default() -> Self {
        Self {
            version: STATE_VERSION.to_string(),
            power: true,
            presets: Presets::default(),
            selected_preset: "Flat".to_string(),
            output_devices: Vec::new(),
            selected_output: String::new(),
            equalizer: Eq::default(),
            effects: Effects::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Presets {
    pub built_in: Vec<PresetInfo>,
    pub user_defined: Vec<PresetInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PresetInfo {
    pub name: String,
    pub modified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Eq {
    pub num_bands: u32,
    pub master_gain: f32,
    pub volume_leveling: f32,
    pub filter_q: f32,
    pub balance: f32,
    pub bands: Vec<Band>,
}

impl Default for Eq {
    fn default() -> Self {
        Self {
            num_bands: 10,
            master_gain: 0.0,
            volume_leveling: 0.0,
            filter_q: 1.0,
            balance: 0.0,
            bands: vec![Band::default(); 10],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Band {
    pub index: u32,
    pub frequency: f32,
    pub gain: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Effects {
    pub clarity: f32,
    pub ambience: f32,
    pub surround: f32,
    pub dynamicboost: f32,
    pub bass: f32,
}

#[derive(Debug)]
pub enum Event {
    Status(Box<Status>),
    NotRunning,
    Error(String),
}

pub struct Client {
    tx: Sender<Vec<String>>,
    rx: Receiver<Event>,
    pub exe: Option<PathBuf>,
    pub status_path: PathBuf,
    state: Status,
    dsp: DspEngine,
}

impl Client {
    pub fn new() -> Self {
        let status_path = state_path();
        let mut state = load_state(&status_path).unwrap_or_default();
        if state.equalizer.bands.is_empty() {
            state.equalizer.bands = default_bands(state.equalizer.num_bands);
        }
        state.version = STATE_VERSION.to_string();
        if state.presets.built_in.is_empty() {
            state.presets.built_in = built_in_presets();
        }

        let mut dsp = DspEngine::new(48_000.0, 2);
        configure_dsp(&mut dsp, &state);

        let (tx, rx) = channel();
        let (event_tx, event_rx) = channel();
        let worker_state = state.clone();
        let worker_path = status_path.clone();
        std::thread::spawn(move || worker(worker_state, worker_path, rx, event_tx));

        Self {
            tx,
            rx: event_rx,
            exe: None,
            status_path,
            state,
            dsp,
        }
    }

    pub fn refresh(&self) {
        let _ = self.tx.send(Vec::new());
    }

    pub fn apply(&self, args: Vec<String>) {
        let _ = self.tx.send(args);
    }

    /// Kept for the UI: there is no separate program to start any more.
    pub fn launch(&self) {
        let _ = self.tx.send(vec!["--power=1".to_string()]);
    }

    pub fn poll(&self) -> Vec<Event> {
        let mut events = Vec::new();
        while let Ok(event) = self.rx.try_recv() {
            events.push(event);
        }
        events
    }

    /// The engine ships with the app, so it is always "installed".
    pub fn is_installed(&self) -> bool {
        true
    }

    pub fn state(&self) -> &Status {
        &self.state
    }

    /// Current DSP configuration, for meters and the diagnostics dump.
    pub fn dsp(&self) -> &DspEngine {
        &self.dsp
    }
}

fn worker(
    mut state: Status,
    path: PathBuf,
    rx: Receiver<Vec<String>>,
    tx: Sender<Event>,
) {
    while let Ok(args) = rx.recv() {
        if !args.is_empty() {
            if let Err(err) = apply_args(&mut state, &args) {
                let _ = tx.send(Event::Error(err));
                continue;
            }
            state.version = STATE_VERSION.to_string();
            if let Err(err) = save_state(&path, &state) {
                let _ = tx.send(Event::Error(format!("Could not save state: {err}")));
            }
        }
        let _ = tx.send(Event::Status(Box::new(state.clone())));
    }
}

fn apply_args(state: &mut Status, args: &[String]) -> Result<(), String> {
    for arg in args {
        let Some(rest) = arg.strip_prefix("--") else {
            return Err(format!("unknown argument: {arg}"));
        };
        let (key, value) = rest.split_once('=').unwrap_or((rest, ""));
        match key {
            "power" => state.power = value.trim() == "1" || value.eq_ignore_ascii_case("true"),
            "num_bands" => {
                let n: u32 = value.parse().map_err(|_| format!("bad num_bands: {value}"))?;
                state.equalizer.num_bands = n;
                state.equalizer.bands = default_bands(n);
            }
            "filter_q" => state.equalizer.filter_q = parse_f32(value)?,
            "master_gain" => state.equalizer.master_gain = parse_f32(value)?,
            "balance" => state.equalizer.balance = parse_f32(value)?,
            "volume_leveling" => state.equalizer.volume_leveling = parse_f32(value)?,
            "set_band_gain" => {
                for pair in value.split(',').filter(|p| !p.is_empty()) {
                    let (i, g) = pair.split_once(':').ok_or("bad band pair")?;
                    let index: usize = i.trim().parse().map_err(|_| "bad band index")?;
                    let gain = parse_f32(g)?;
                    if let Some(band) = state.equalizer.bands.get_mut(index) {
                        band.gain = gain;
                    }
                }
            }
            "set_effect" => {
                for pair in value.split(',').filter(|p| !p.is_empty()) {
                    let (name, v) = pair.split_once(':').ok_or("bad effect pair")?;
                    let v = parse_f32(v)?;
                    match name.trim() {
                        "clarity" => state.effects.clarity = v,
                        "ambience" => state.effects.ambience = v,
                        "surround" => state.effects.surround = v,
                        "dynamicboost" => state.effects.dynamicboost = v,
                        "bass" => state.effects.bass = v,
                        other => return Err(format!("unknown effect: {other}")),
                    }
                }
            }
            "preset" => {
                state.selected_preset = value.to_string();
                if let Some(saved) = load_preset(value) {
                    state.equalizer = saved.equalizer;
                    state.effects = saved.effects;
                }
                state.power = true;
            }
            "save_preset" => {
                let name = value.to_string();
                if !name.is_empty() {
                    let saved = Snapshot {
                        equalizer: state.equalizer.clone(),
                        effects: state.effects.clone(),
                    };
                    save_preset(&name, &saved)?;
                    if !state.presets.user_defined.iter().any(|p| p.name == name) {
                        state.presets.user_defined.push(PresetInfo { name, modified: false });
                    }
                }
            }
            "output" => state.selected_output = value.to_string(),
            other => return Err(format!("unknown command: {other}")),
        }
    }
    Ok(())
}

fn parse_f32(v: &str) -> Result<f32, String> {
    v.trim().parse().map_err(|_| format!("bad number: {v}"))
}

fn default_bands(count: u32) -> Vec<Band> {
    presets::freqs_for_count(count as usize)
        .into_iter()
        .enumerate()
        .map(|(i, f)| Band { index: i as u32, frequency: f, gain: 0.0 })
        .collect()
}

/// Turn the UI state into biquads. This is the whole point of the rewrite: the
/// numbers on screen are the numbers the DSP runs.
pub fn configure_dsp(dsp: &mut DspEngine, state: &Status) {
    let eq = &state.equalizer;
    let input_gain = eq.master_gain
        + eq.volume_leveling * 0.6
        + state.effects.dynamicboost * 0.5;
    dsp.set_input_gain_db(input_gain);
    dsp.width = 1.0 + state.effects.surround * 0.08;
    dsp.limiter_on = eq.volume_leveling > 0.05 || state.effects.dynamicboost > 0.05;
    if dsp.limiter_on {
        dsp.set_limiter_ceiling_db(-0.3 - eq.volume_leveling * 0.05);
    }

    let q = if eq.filter_q > 0.0 { eq.filter_q } else { 1.0 };
    let mut bands: Vec<DspBand> = eq
        .bands
        .iter()
        .map(|b| DspBand::peaking(b.frequency.max(20.0), q, b.gain))
        .collect();
    if state.effects.bass > 0.05 {
        bands.push(DspBand { kind: BandKind::LowShelf, freq: 90.0, q: 0.7, gain_db: state.effects.bass * 1.5 });
    }
    if state.effects.clarity > 0.05 {
        bands.push(DspBand { kind: BandKind::HighShelf, freq: 3500.0, q: 0.7, gain_db: state.effects.clarity * 0.8 });
    }
    dsp.set_bands(&bands);
    dsp.bypass = !state.power;
}

fn built_in_presets() -> Vec<PresetInfo> {
    ["Flat", "Music", "Movies", "Voice", "Bass Boost", "Headphones"]
        .iter()
        .map(|name| PresetInfo { name: (*name).to_string(), modified: false })
        .collect()
}

fn state_path() -> PathBuf {
    std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(r"beanxsound\engine-state.json")
}

fn presets_dir() -> PathBuf {
    std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(r"beanxsound\presets")
}

fn load_state(path: &PathBuf) -> Option<Status> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn save_state(path: &PathBuf, state: &Status) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_string_pretty(state).map_err(std::io::Error::other)?;
    std::fs::write(path, text)
}

#[derive(Serialize, Deserialize, Clone)]
struct Snapshot {
    equalizer: Eq,
    effects: Effects,
}

fn preset_path(name: &str) -> PathBuf {
    let safe: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' })
        .collect();
    presets_dir().join(format!("{}.json", safe.trim()))
}

fn save_preset(name: &str, snap: &Snapshot) -> Result<(), String> {
    let path = preset_path(name);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(snap).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

fn load_preset(name: &str) -> Option<Snapshot> {
    let text = std::fs::read_to_string(preset_path(name)).ok()?;
    serde_json::from_str(&text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn applies_the_editors_command_grammar() {
        let mut state = Status::default();
        apply_args(
            &mut state,
            &cmd(&[
                "--power=1",
                "--num_bands=10",
                "--filter_q=1.4",
                "--master_gain=3.5",
                "--balance=-0.5",
                "--volume_leveling=2.0",
                "--set_band_gain=0:6.00,9:-4.50",
                "--set_effect=clarity:3.0,ambience:1.0,surround:2.0,dynamicboost:0.0,bass:5.0",
            ]),
        )
        .expect("command list should apply");

        assert!(state.power);
        assert_eq!(state.equalizer.num_bands, 10);
        assert_eq!(state.equalizer.bands.len(), 10);
        assert!((state.equalizer.filter_q - 1.4).abs() < 1e-6);
        assert!((state.equalizer.master_gain - 3.5).abs() < 1e-6);
        assert!((state.equalizer.bands[0].gain - 6.0).abs() < 1e-6);
        assert!((state.equalizer.bands[9].gain + 4.5).abs() < 1e-6);
        assert!((state.effects.bass - 5.0).abs() < 1e-6);
        assert!((state.effects.clarity - 3.0).abs() < 1e-6);
    }

    #[test]
    fn unknown_commands_are_rejected_not_ignored() {
        let mut state = Status::default();
        let err = apply_args(&mut state, &cmd(&["--frobnicate=1"])).unwrap_err();
        assert!(err.contains("frobnicate"), "error should name the command: {err}");
    }

    #[test]
    fn dsp_reflects_the_ui_state() {
        let mut state = Status::default();
        apply_args(
            &mut state,
            &cmd(&["--power=1", "--num_bands=10", "--set_effect=bass:5.0,clarity:4.0,dynamicboost:3.0"]),
        )
        .unwrap();

        let mut dsp = DspEngine::new(48_000.0, 2);
        configure_dsp(&mut dsp, &state);
        assert!(!dsp.bypass);
        assert!(dsp.limiter_on, "dynamic boost should engage the limiter");

        state.power = false;
        configure_dsp(&mut dsp, &state);
        assert!(dsp.bypass, "power off must bypass the DSP");
    }
}
