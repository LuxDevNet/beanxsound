use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct EqBand {
    pub freq: f32,
    pub gain: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Effects {
    pub clarity: f32,
    pub ambience: f32,
    pub surround: f32,
    pub dynamic_boost: f32,
    pub bass: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UserPreset {
    pub name: String,
    pub bands: Vec<EqBand>,
    pub master_gain: f32,
    pub balance: f32,
    pub volume_leveling: f32,
    pub filter_q: f32,
    pub effects: Effects,
}

impl Default for UserPreset {
    fn default() -> Self {
        Self {
            name: "New preset".to_string(),
            bands: Vec::new(),
            master_gain: 0.0,
            balance: 0.0,
            volume_leveling: 0.0,
            filter_q: 1.0,
            effects: Effects::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PresetStore {
    pub presets: Vec<UserPreset>,
}

impl PresetStore {
    pub fn load() -> Self {
        let path = store_path();
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = store_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Could not create preset folder: {e}"))?;
        }
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Could not serialize presets: {e}"))?;
        std::fs::write(&path, text).map_err(|e| format!("Could not save presets: {e}"))
    }

    pub fn add(&mut self, preset: UserPreset) -> Result<(), String> {
        let name = preset.name.trim();
        if name.is_empty() {
            return Err("Preset name cannot be empty.".to_string());
        }
        if let Some(existing) = self
            .presets
            .iter_mut()
            .find(|p| p.name.eq_ignore_ascii_case(name))
        {
            *existing = preset;
        } else {
            self.presets.push(preset);
            self.presets
                .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        }
        self.save()
    }

    pub fn remove(&mut self, name: &str) -> Result<(), String> {
        self.presets.retain(|p| p.name != name);
        self.save()
    }

    pub fn rename(&mut self, old: &str, new: &str) -> Result<(), String> {
        let new = new.trim();
        if new.is_empty() {
            return Err("Preset name cannot be empty.".to_string());
        }
        if self
            .presets
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case(new) && p.name != old)
        {
            return Err(format!("A preset named \"{new}\" already exists."));
        }
        if let Some(p) = self.presets.iter_mut().find(|p| p.name == old) {
            p.name = new.to_string();
        }
        self.presets
            .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        self.save()
    }

    pub fn export_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    pub fn import_json(&mut self, text: &str) -> Result<usize, String> {
        let incoming: PresetStore = serde_json::from_str(text)
            .map_err(|e| format!("That is not a valid BeanXSound preset file: {e}"))?;
        let mut count = 0;
        for preset in incoming.presets {
            if preset.name.trim().is_empty() {
                continue;
            }
            if let Some(existing) = self
                .presets
                .iter_mut()
                .find(|p| p.name.eq_ignore_ascii_case(&preset.name))
            {
                *existing = preset;
            } else {
                self.presets.push(preset);
            }
            count += 1;
        }
        self.presets
            .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        self.save()?;
        Ok(count)
    }
}

pub fn store_path() -> PathBuf {
    std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(r"beanxsound\presets.json")
}

/// 31-band centre frequencies used by FxSound's full equalizer.
pub const FREQS_31: [f32; 31] = [
    20.0, 25.0, 31.5, 40.0, 50.0, 63.0, 80.0, 100.0, 125.0, 160.0, 200.0, 250.0, 315.0, 400.0,
    500.0, 630.0, 800.0, 1000.0, 1250.0, 1600.0, 2000.0, 2500.0, 3150.0, 4000.0, 5000.0, 6300.0,
    8000.0, 10000.0, 12500.0, 16000.0, 20000.0,
];

pub fn freq_label(f: f32) -> String {
    if f >= 10000.0 {
        format!("{:.0}k", f / 1000.0)
    } else if f >= 1000.0 {
        let v = f / 1000.0;
        if (v - v.round()).abs() < 0.01 {
            format!("{:.0}k", v)
        } else {
            format!("{v:.2}k")
        }
    } else if f >= 100.0 {
        format!("{f:.0}")
    } else {
        format!("{f:.0}")
    }
}

/// Build one FxSound CLI invocation from the editor state.
pub fn apply_args(
    bands: &[EqBand],
    master_gain: f32,
    balance: f32,
    volume_leveling: f32,
    filter_q: f32,
    effects: &Effects,
) -> Vec<String> {
    let n = if matches!(bands.len(), 5 | 10 | 15 | 20 | 31) {
        bands.len()
    } else {
        31
    };
    let gain_list = bands
        .iter()
        .enumerate()
        .map(|(i, b)| format!("{}:{:.2}", i, b.gain))
        .collect::<Vec<_>>()
        .join(",");
    vec![
        "--power=1".to_string(),
        format!("--num_bands={n}"),
        format!("--filter_q={filter_q:.1}"),
        format!("--master_gain={master_gain:.1}"),
        format!("--balance={balance:.1}"),
        format!("--volume_leveling={volume_leveling:.1}"),
        format!("--set_band_gain={gain_list}"),
        format!(
            "--set_effect=clarity:{:.1},ambience:{:.1},surround:{:.1},dynamicboost:{:.1},bass:{:.1}",
            effects.clarity.clamp(0.0, 10.0),
            effects.ambience.clamp(0.0, 10.0),
            effects.surround.clamp(0.0, 10.0),
            effects.dynamic_boost.clamp(0.0, 10.0),
            effects.bass.clamp(0.0, 10.0)
        ),
    ]
}

/// Frequencies for the band count FxSound was told to use.
pub fn freqs_for_count(count: usize) -> Vec<f32> {
    match count {
        5 => vec![60.0, 250.0, 1000.0, 4000.0, 12000.0],
        10 => vec![
            31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
        ],
        15 => vec![
            25.0, 40.0, 63.0, 100.0, 160.0, 250.0, 400.0, 630.0, 1000.0, 1600.0, 2500.0,
            4000.0, 6300.0, 10000.0, 16000.0,
        ],
        20 => vec![
            20.0, 25.0, 31.5, 40.0, 50.0, 63.0, 80.0, 100.0, 125.0, 160.0, 200.0, 250.0, 315.0,
            400.0, 500.0, 630.0, 1000.0, 2000.0, 6300.0, 16000.0,
        ],
        _ => FREQS_31.to_vec(),
    }
}
