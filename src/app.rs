use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontFamily, FontId, Id, Layout, Margin, Pos2, Rect,
    RichText, Sense, Shape, Stroke, Vec2,
};

use crate::audio::{self, AudioDevice, AudioEngine, Role, Snapshot};
use crate::fxsound::{FxClient, FxEvent, FxStatus};
use crate::presets::{self, EqBand, Effects, PresetStore, UserPreset};
use crate::theme as th;
use crate::winshell;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Devices,
    Mixer,
    FxSound,
    Presets,
    Advanced,
}

impl Tab {
    fn label(self) -> &'static str {
        match self {
            Tab::Devices => "Devices",
            Tab::Mixer => "App mixer",
            Tab::FxSound => "FxSound",
            Tab::Presets => "Presets",
            Tab::Advanced => "Advanced",
        }
    }
    fn icon(self) -> &'static str {
        match self {
            Tab::Devices => "◉",
            Tab::Mixer => "▤",
            Tab::FxSound => "≈",
            Tab::Presets => "✦",
            Tab::Advanced => "⚙",
        }
    }
    fn subtitle(self) -> &'static str {
        match self {
            Tab::Devices => "Outputs & inputs",
            Tab::Mixer => "Per-app volume",
            Tab::FxSound => "EQ & effects",
            Tab::Presets => "Your profiles",
            Tab::Advanced => "Tools & info",
        }
    }
}

#[derive(Clone)]
struct Editor {
    bands: Vec<EqBand>,
    master_gain: f32,
    balance: f32,
    volume_leveling: f32,
    filter_q: f32,
    effects: Effects,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            bands: presets::freqs_for_count(31)
                .into_iter()
                .map(|freq| EqBand { freq, gain: 0.0 })
                .collect(),
            master_gain: 0.0,
            balance: 0.0,
            volume_leveling: 0.0,
            filter_q: 1.0,
            effects: Effects::default(),
        }
    }
}

impl Editor {
    fn from_status(status: &FxStatus) -> Self {
        let mut editor = Self::default();
        if !status.equalizer.bands.is_empty() {
            editor.bands = status
                .equalizer
                .bands
                .iter()
                .map(|b| EqBand {
                    freq: b.frequency,
                    gain: b.gain,
                })
                .collect();
        }
        editor.master_gain = status.equalizer.master_gain;
        editor.balance = status.equalizer.balance;
        editor.volume_leveling = status.equalizer.volume_leveling;
        editor.filter_q = if status.equalizer.filter_q > 0.0 {
            status.equalizer.filter_q
        } else {
            1.0
        };
        editor.effects = Effects {
            clarity: status.effects.clarity,
            ambience: status.effects.ambience,
            surround: status.effects.surround,
            dynamic_boost: status.effects.dynamicboost,
            bass: status.effects.bass,
        };
        editor
    }

    fn band_count(&self) -> usize {
        self.bands.len()
    }

    fn set_band_count(&mut self, count: usize) {
        if count == self.bands.len() {
            return;
        }
        self.bands = presets::freqs_for_count(count)
            .into_iter()
            .map(|freq| EqBand { freq, gain: 0.0 })
            .collect();
    }

    fn flatten(&mut self) {
        for band in &mut self.bands {
            band.gain = 0.0;
        }
    }

    fn args(&self) -> Vec<String> {
        presets::apply_args(
            &self.bands,
            self.master_gain,
            self.balance,
            self.volume_leveling,
            self.filter_q,
            &self.effects,
        )
    }
}

enum Dialog {
    None,
    SaveNew,
    Rename(String),
    Delete(String),
    About,
}

struct Toast {
    text: String,
    kind: bool,
    at: Instant,
}

enum DeviceAction {
    SetDefault(String, Role),
    SetEnabled(String, bool),
}

pub struct BeanXSound {
    audio: Option<AudioEngine>,
    audio_error: Option<String>,
    snapshot: Snapshot,
    include_inactive: bool,
    default_role: Role,
    expanded_channel: Option<String>,
    tab: Tab,

    fx: FxClient,
    fx_status: Option<FxStatus>,
    fx_running: bool,
    fx_error: Option<String>,
    fx_pending: bool,
    ignore_sync_until: f64,
    editor: Editor,
    editor_dirty: bool,
    live: bool,
    last_edit: f64,
    last_apply: f64,

    presets: PresetStore,
    new_preset_name: String,
    rename_name: String,
    mixer_search: String,

    dialog: Dialog,
    toast: Option<Toast>,
    last_audio_refresh: Instant,
    last_fx_refresh: Instant,
    drag_band: Option<usize>,
    hover_band: Option<usize>,
    admin: bool,
}

impl BeanXSound {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        th::install_fonts(&cc.egui_ctx);
        let _ = cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
        th::set_theme_index(th::load_saved());
        th::apply(&cc.egui_ctx);

        // The window is alive now, so COM has already been initialized on this
        // thread by winit. Join that apartment before touching WASAPI.
        let _ = audio::init_com();

        let (audio, audio_error) = match AudioEngine::new() {
            Ok(engine) => (Some(engine), None),
            Err(err) => (None, Some(err)),
        };
        let snapshot = audio
            .as_ref()
            .map(|engine| engine.refresh(true))
            .unwrap_or_default();
        let fx = FxClient::new();
        fx.refresh();
        let presets = PresetStore::load();

        let mut app = Self {
            audio,
            audio_error,
            snapshot,
            include_inactive: true,
            default_role: Role::Console,
            expanded_channel: None,
            tab: Tab::Devices,
            fx,
            fx_status: None,
            fx_running: false,
            fx_error: None,
            fx_pending: false,
            ignore_sync_until: 0.0,
            editor: Editor::default(),
            editor_dirty: false,
            live: false,
            last_edit: 0.0,
            last_apply: 0.0,
            presets,
            new_preset_name: String::new(),
            rename_name: String::new(),
            mixer_search: String::new(),
            dialog: Dialog::None,
            toast: None,
            last_audio_refresh: Instant::now(),
            last_fx_refresh: Instant::now(),
            drag_band: None,
            hover_band: None,
            admin: winshell::is_elevated(),
        };
        app.refresh_audio();
        app
    }

    fn toast(&mut self, text: impl Into<String>, good: bool) {
        self.toast = Some(Toast {
            text: text.into(),
            kind: good,
            at: Instant::now(),
        });
    }

    fn refresh_audio(&mut self) {
        if let Some(engine) = &self.audio {
            self.snapshot = engine.refresh(self.include_inactive);
        }
        self.last_audio_refresh = Instant::now();
    }

    fn refresh_fx(&mut self) {
        self.fx.refresh();
        self.last_fx_refresh = Instant::now();
    }

    fn apply_editor(&mut self, now: f64) {
        if !self.fx.is_installed() {
            self.toast("FxSound is not installed on this PC.", false);
            return;
        }
        self.fx.apply(self.editor.args());
        self.fx_pending = true;
        self.editor_dirty = false;
        self.last_apply = now;
        self.ignore_sync_until = now + 1.8;
    }

    fn select_preset(&mut self, name: &str, now: f64) {
        self.fx.apply(vec![
            "--power=1".to_string(),
            format!("--preset={name}"),
        ]);
        self.fx_pending = true;
        self.ignore_sync_until = now + 1.8;
        self.toast(format!("Loading preset “{name}”"), true);
    }

    fn set_power(&mut self, on: bool, now: f64) {
        self.fx.apply(vec![format!("--power={}", if on { 1 } else { 0 })]);
        self.fx_pending = true;
        self.ignore_sync_until = now + 1.4;
        if let Some(status) = &mut self.fx_status {
            status.power = on;
        }
    }

    fn poll(&mut self, now: f64) {
        if self.last_audio_refresh.elapsed() > Duration::from_millis(1300) {
            self.refresh_audio();
        }
        if let Some(engine) = &self.audio {
            engine.tick_meters(&mut self.snapshot.devices);
        }

        for event in self.fx.poll() {
            match event {
                FxEvent::Status(status) => {
                    self.fx_running = true;
                    self.fx_error = None;
                    self.fx_pending = false;
                    let sync = !self.editor_dirty && now > self.ignore_sync_until;
                    if sync {
                        self.editor = Editor::from_status(&status);
                    }
                    self.fx_status = Some(*status);
                }
                FxEvent::NotRunning => {
                    self.fx_running = false;
                    self.fx_pending = false;
                }
                FxEvent::Error(err) => {
                    self.fx_error = Some(err);
                    self.fx_pending = false;
                }
            }
        }

        if self.last_fx_refresh.elapsed() > Duration::from_millis(3800) {
            if self.fx.is_installed() {
                self.fx.refresh();
            }
            self.last_fx_refresh = Instant::now();
        }

        if self.live && self.editor_dirty && now - self.last_edit > 0.55 && now - self.last_apply > 0.8 {
            self.apply_editor(now);
        }

        if let Some(toast) = &self.toast {
            if toast.at.elapsed() > Duration::from_secs(4) {
                self.toast = None;
            }
        }
    }

    fn mark_edit(&mut self, now: f64) {
        self.editor_dirty = true;
        self.last_edit = now;
    }

    fn page_header(&mut self, ui: &mut egui::Ui, title: &str, subtitle: &str, action: impl FnOnce(&mut Self, &mut egui::Ui)) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("B E A N X S O U N D")
                        .size(9.5)
                        .color(th::accent())
                        .strong(),
                );
                ui.add_space(2.0);
                ui.label(RichText::new(title).size(28.0).strong().color(th::text()));
                ui.label(RichText::new(subtitle).size(12.5).color(th::muted()));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                action(self, ui);
            });
        });
        ui.add_space(14.0);
    }

    fn tab_devices(&mut self, ui: &mut egui::Ui, now: f64) {
        self.page_header(ui, "Devices", "Everything Windows can play through, in one place.", |app, ui| {
            ui.checkbox(&mut app.include_inactive, "Show inactive");
            ui.add_space(6.0);
            egui::ComboBox::from_id_salt("default_role")
                .selected_text(format!("Default: {}", app.default_role.label()))
                .width(176.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut app.default_role, Role::Console, "Default (console)");
                    ui.selectable_value(&mut app.default_role, Role::Multimedia, "Multimedia");
                    ui.selectable_value(&mut app.default_role, Role::Communications, "Communications");
                });
        });

        let mut actions = Vec::new();
        let role = self.default_role;
        let mut expanded = self.expanded_channel.clone();

        let mut outputs = Vec::new();
        let mut inputs = Vec::new();
        for (idx, device) in self.snapshot.devices.iter().enumerate() {
            if device.is_capture {
                inputs.push(idx);
            } else {
                outputs.push(idx);
            }
        }

        if outputs.is_empty() {
            th::card().show(ui, |ui| {
                empty_state(ui, "No playback devices found", "Check that your audio hardware is connected and enabled in Windows.");
            });
        } else {
            ui.label(RichText::new("PLAYBACK").size(10.0).strong().color(th::faint()));
            ui.add_space(4.0);
            for idx in outputs {
                device_card(ui, &mut self.snapshot.devices[idx], role, &mut expanded, &mut actions);
                ui.add_space(4.0);
            }
        }

        ui.add_space(14.0);
        if inputs.is_empty() {
            ui.label(RichText::new("RECORDING").size(10.0).strong().color(th::faint()));
            ui.add_space(4.0);
            th::card().show(ui, |ui| {
                empty_state(ui, "No microphones found", "Connect a microphone or enable one in Windows sound settings.");
            });
        } else {
            ui.label(RichText::new("RECORDING").size(10.0).strong().color(th::faint()));
            ui.add_space(4.0);
            for idx in inputs {
                device_card(ui, &mut self.snapshot.devices[idx], role, &mut expanded, &mut actions);
                ui.add_space(4.0);
            }
        }

        self.expanded_channel = expanded;

        for action in actions {
            match action {
                DeviceAction::SetDefault(id, role) => {
                    if let Some(engine) = &self.audio {
                        match engine.set_default(&id, role) {
                            Ok(()) => {
                                self.toast(format!("Default {} device updated", role.label().to_lowercase()), true);
                                self.refresh_audio();
                            }
                            Err(err) => self.toast(format!("Windows refused the change: {err}"), false),
                        }
                    }
                }
                DeviceAction::SetEnabled(id, enabled) => {
                    if let Some(engine) = &self.audio {
                        match engine.set_enabled(&id, enabled) {
                            Ok(()) => {
                                self.toast(
                                    if enabled { "Device enabled" } else { "Device disabled" },
                                    true,
                                );
                                self.refresh_audio();
                            }
                            Err(err) => {
                                self.toast(
                                    format!("Could not change device: {err}. Try running as administrator."),
                                    false,
                                );
                            }
                        }
                    }
                }
            }
        }

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ghost_button(ui, "Open Windows sound settings").clicked() {
                audio::open_target("ms-settings:sound");
            }
            if ghost_button(ui, "Sound control panel").clicked() {
                audio::open_target("control.exe mmsys.cpl,,0");
            }
            ui.label(RichText::new(format!("Updated just now · {} endpoints", self.snapshot.devices.len())).size(10.5).color(th::faint()));
        });

        let _ = now;
    }

    fn tab_mixer(&mut self, ui: &mut egui::Ui, now: f64) {
        self.page_header(ui, "App mixer", "Per-application volume, straight from the Windows audio session bus.", |app, ui| {
            ui.add(
                egui::TextEdit::singleline(&mut app.mixer_search)
                    .hint_text("Filter apps…")
                    .desired_width(190.0),
            );
            if ghost_button(ui, "Refresh").clicked() {
                app.refresh_audio();
            }
        });

        let filter = self.mixer_search.to_lowercase();
        let mut sessions: Vec<usize> = (0..self.snapshot.sessions.len())
            .filter(|idx| {
                let s = &self.snapshot.sessions[*idx];
                filter.is_empty()
                    || s.app_name().to_lowercase().contains(&filter)
                    || s.device_name.to_lowercase().contains(&filter)
            })
            .collect();
        sessions.sort_by(|a, b| {
            let sa = &self.snapshot.sessions[*a];
            let sb = &self.snapshot.sessions[*b];
            sb.state
                .0
                .cmp(&sa.state.0)
                .then_with(|| sa.app_name().to_lowercase().cmp(&sb.app_name().to_lowercase()))
        });

        if sessions.is_empty() {
            th::card().show(ui, |ui| {
                empty_state(
                    ui,
                    "Nothing is playing right now",
                    "Start some audio in any app and its session will appear here automatically.",
                );
            });
            return;
        }

        ui.label(RichText::new(format!("{} ACTIVE SESSIONS", sessions.len())).size(10.0).strong().color(th::faint()));
        ui.add_space(4.0);
        for idx in sessions {
            let session = &mut self.snapshot.sessions[idx];
            let active = session.state == windows::Win32::Media::Audio::AudioSessionStateActive;
            th::card().show(ui, |ui| {
                ui.horizontal(|ui| {
                    let letter = session
                        .app_name()
                        .chars()
                        .next()
                        .unwrap_or('?')
                        .to_uppercase()
                        .to_string();
                    app_badge(ui, &letter, if session.is_capture { th::amber() } else { th::accent() });
                    ui.add_space(4.0);
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(session.app_name()).size(14.5).strong().color(th::text()));
                            if session.muted {
                                th::chip(ui, "MUTED", th::red());
                            }
                        });
                        ui.label(
                            RichText::new(format!(
                                "{} · {}",
                                session.device_name,
                                if active { "playing" } else { "idle" }
                            ))
                            .size(10.5)
                            .color(th::faint()),
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let mut muted = session.muted;
                        let mute_text = if muted { "Unmute" } else { "Mute" };
                        if ghost_button(ui, mute_text).clicked() {
                            muted = !muted;
                            session.set_mute(muted);
                        }
                        let mut volume = session.volume;
                        let resp = ui.add_sized(
                            [190.0, 22.0],
                            egui::Slider::new(&mut volume, 0.0..=1.0).show_value(false),
                        );
                        if resp.changed() {
                            session.set_volume(volume);
                        }
                        ui.label(
                            RichText::new(audio::format_percent(session.volume))
                                .size(11.0)
                                .color(th::muted())
                                .monospace(),
                        );
                    });
                });
            });
            ui.add_space(4.0);
        }
        let _ = now;
    }

    fn tab_fxsound(&mut self, ui: &mut egui::Ui, now: f64) {
        let installed = self.fx.is_installed();
        self.page_header(ui, "FxSound engine", "The real FxSound DSP, driven through its command-line bridge.", |app, ui| {
            if app.editor_dirty {
                th::chip(ui, "UNSAVED CHANGES", th::amber());
            }
            if primary_button(ui, "Apply to FxSound").clicked() {
                app.apply_editor(now);
            }
            if ghost_button(ui, "Revert").clicked() {
                if let Some(status) = app.fx_status.clone() {
                    app.editor = Editor::from_status(&status);
                    app.editor_dirty = false;
                    app.toast("Reverted to the live FxSound state", true);
                }
            }
        });

        if !installed {
            th::card().show(ui, |ui| {
                ui.label(RichText::new("FxSound was not found").size(17.0).strong().color(th::text()));
                ui.label(RichText::new("Install FxSound 1.2 or newer and this page will come alive. beanxsound controls the official engine rather than copying its DSP.").size(12.0).color(th::muted()));
                ui.add_space(8.0);
                if primary_button(ui, "Get FxSound").clicked() {
                    audio::open_target("https://www.fxsound.com/");
                }
            });
            return;
        }

        if let Some(err) = &self.fx_error {
            if !self.fx_running {
                th::card().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        th::icon_badge(ui, "!", th::amber());
                        ui.vertical(|ui| {
                            ui.label(RichText::new("FxSound is not responding").size(14.0).strong().color(th::text()));
                            ui.label(RichText::new(err).size(11.0).color(th::muted()));
                        });
                    });
                    ui.add_space(6.0);
                    if primary_button(ui, "Start FxSound").clicked() {
                        self.fx.launch();
                    }
                });
                ui.add_space(8.0);
            }
        }

        // Hero / live control
        th::card().show(ui, |ui| {
            ui.horizontal(|ui| {
                let power = self.fx_status.as_ref().map(|s| s.power).unwrap_or(false);
                let (rect, resp) = toggle_switch(ui, power);
                let _ = rect;
                if resp.clicked() {
                    self.set_power(!power, now);
                }
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(if power { "Processing on" } else { "Processing off" })
                            .size(20.0)
                            .strong()
                            .color(if power { th::text() } else { th::muted() }),
                    );
                    ui.label(
                        RichText::new(if power {
                            "FxSound is enhancing the selected output."
                        } else {
                            "The engine is bypassed. Flip the switch to enable it."
                        })
                        .size(11.5)
                        .color(th::faint()),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ghost_button(ui, "Open FxSound").clicked() {
                        if let Some(exe) = &self.fx.exe {
                            audio::open_target(&exe.display().to_string());
                        }
                    }
                    if !self.fx_running && ghost_button(ui, "Launch engine").clicked() {
                        self.fx.launch();
                    }
                    if self.fx_status.is_none() && ghost_button(ui, "Refresh").clicked() {
                        self.refresh_fx();
                    }
                });
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);

            ui.columns(3, |cols| {
                cols[0].label(RichText::new("CURRENT PRESET").size(9.5).strong().color(th::faint()));
                let current = self
                    .fx_status
                    .as_ref()
                    .map(|s| s.selected_preset.clone())
                    .unwrap_or_else(|| "—".to_string());
                cols[0].label(RichText::new(current).size(15.0).strong().color(th::text()));

                cols[1].label(RichText::new("OUTPUT DEVICE").size(9.5).strong().color(th::faint()));
                let outputs: Vec<String> = self
                    .fx_status
                    .as_ref()
                    .map(|s| s.output_devices.clone())
                    .unwrap_or_default();
                let selected = self
                    .fx_status
                    .as_ref()
                    .map(|s| s.selected_output.clone())
                    .unwrap_or_default();
                egui::ComboBox::from_id_salt("fx_output")
                    .selected_text(if selected.is_empty() { "Choose output".to_string() } else { selected.clone() })
                    .width(cols[1].available_width().min(300.0))
                    .show_ui(&mut cols[1], |ui| {
                        for name in &outputs {
                            if ui.selectable_label(&selected == name, name).clicked() {
                                self.fx.apply(vec![format!("--output={name}")]);
                                self.toast(format!("FxSound output → {name}"), true);
                            }
                        }
                    });

                cols[2].label(RichText::new("ENGINE").size(9.5).strong().color(th::faint()));
                let (label, color) = if self.fx_running {
                    (format!("FxSound {}", self.fx_status.as_ref().map(|s| s.version.clone()).unwrap_or_default()), th::green())
                } else {
                    ("Not running".to_string(), th::red())
                };
                cols[2].label(RichText::new(label).size(13.0).strong().color(color));
            });
        });

        ui.add_space(10.0);

        // Equalizer
        th::card().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Equalizer").size(17.0).strong().color(th::text()));
                    ui.label(RichText::new("Drag the nodes. Double-click to flatten a band.").size(10.5).color(th::faint()));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ghost_button(ui, "Flatten").clicked() {
                        self.editor.flatten();
                        self.mark_edit(now);
                    }
                    for count in [31usize, 20, 15, 10, 5] {
                        let selected = self.editor.band_count() == count;
                        if th::tab_button(ui, selected, &format!("{count}")) && !selected {
                            self.editor.set_band_count(count);
                            self.mark_edit(now);
                        }
                    }
                    ui.label(RichText::new("BANDS").size(9.5).strong().color(th::faint()));
                });
            });
            ui.add_space(8.0);
            let before = self.editor.bands.clone();
            self.eq_graph(ui, now);
            if self.editor.bands.len() != before.len() {
                self.mark_edit(now);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Precision").size(10.0).color(th::faint()));
                ui.label(RichText::new("·").size(10.0).color(th::faint()));
                ui.label(RichText::new("double-click resets · scroll for bands").size(10.0).color(th::faint()));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.checkbox(&mut self.live, "Live apply");
                });
            });
            ui.add_space(6.0);
            egui::ScrollArea::horizontal()
                .id_salt("band_values")
                .max_height(78.0)
                .show(ui, |ui| {
                    let mut changed = false;
                    ui.horizontal(|ui| {
                        for (i, band) in self.editor.bands.iter_mut().enumerate() {
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(presets::freq_label(band.freq))
                                        .size(9.0)
                                        .color(th::faint()),
                                );
                                let resp = ui.add(
                                    egui::DragValue::new(&mut band.gain)
                                        .speed(0.1)
                                        .range(-12.0..=12.0)
                                        .fixed_decimals(1),
                                );
                                if resp.changed() {
                                    changed = true;
                                }
                                let _ = i;
                            });
                        }
                    });
                    if changed {
                        self.mark_edit(now);
                    }
                });
        });

        ui.add_space(10.0);

        // Effects
        th::card().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Effects").size(17.0).strong().color(th::text()));
                    ui.label(RichText::new("FxSound's five signature stages. Small moves go a long way.").size(10.5).color(th::faint()));
                });
            });
            ui.add_space(10.0);
            let mut changed = false;
            let effects = &mut self.editor.effects;
            changed |= effect_row(ui, "Clarity", "Restores detail and texture", th::accent(), &mut effects.clarity);
            changed |= effect_row(ui, "Ambience", "Adds room and depth", Color32::from_rgb(120, 200, 255), &mut effects.ambience);
            changed |= effect_row(ui, "Surround", "Widens the stereo field", Color32::from_rgb(150, 140, 255), &mut effects.surround);
            changed |= effect_row(ui, "Dynamic boost", "Lifts quiet passages", th::amber(), &mut effects.dynamic_boost);
            changed |= effect_row(ui, "Bass", "Adds low-end weight", Color32::from_rgb(255, 130, 150), &mut effects.bass);
            if changed {
                self.mark_edit(now);
            }
        });

        ui.add_space(10.0);

        // Signal
        th::card().show(ui, |ui| {
            ui.label(RichText::new("Signal").size(17.0).strong().color(th::text()));
            ui.label(RichText::new("Gain staging and stereo placement.").size(10.5).color(th::faint()));
            ui.add_space(10.0);
            let mut changed = false;
            ui.columns(2, |cols| {
                changed |= signal_row(&mut cols[0], "Master gain", "dB", -20.0..=20.0, 0.5, &mut self.editor.master_gain);
                changed |= signal_row(&mut cols[1], "Balance", "dB", -20.0..=20.0, 0.5, &mut self.editor.balance);
                changed |= signal_row(&mut cols[0], "Volume leveling", "dB", 0.0..=4.0, 0.5, &mut self.editor.volume_leveling);
                changed |= signal_row(&mut cols[1], "Filter Q", "Q", 1.0..=3.0, 0.1, &mut self.editor.filter_q);
            });
            if changed {
                self.mark_edit(now);
            }
        });

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if self.fx_pending {
                th::chip(ui, "TALKING TO ENGINE…", th::amber());
            } else if let Some(status) = &self.fx_status {
                ui.label(RichText::new(format!("{} bands · master {:.1} dB · Q {:.1}", status.equalizer.num_bands, status.equalizer.master_gain, status.equalizer.filter_q)).size(10.5).color(th::faint()));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ghost_button(ui, "Reset all").clicked() {
                    self.editor = Editor::default();
                    self.mark_edit(now);
                    self.apply_editor(now);
                }
            });
        });
    }

    fn eq_graph(&mut self, ui: &mut egui::Ui, now: f64) {
        let width = ui.available_width().max(360.0);
        let height = 236.0;
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, CornerRadius::same(10), th::input());
        painter.rect_stroke(
            rect,
            CornerRadius::same(10),
            Stroke::new(1.0, th::border()),
            egui::StrokeKind::Inside,
        );

        let plot = Rect::from_min_max(
            Pos2::new(rect.left() + 12.0, rect.top() + 12.0),
            Pos2::new(rect.right() - 12.0, rect.bottom() - 24.0),
        );
        let y_for = |gain: f32| plot.center().y - gain / 12.0 * (plot.height() * 0.46);
        let x_for = |idx: usize, count: usize| {
            if count <= 1 {
                plot.center().x
            } else {
                plot.left() + plot.width() * idx as f32 / (count - 1) as f32
            }
        };

        // Grid
        for g in [-12.0f32, -6.0, 0.0, 6.0, 12.0] {
            let y = y_for(g);
            let color = if g == 0.0 { th::border() } else { th::border().gamma_multiply(0.45) };
            if g == 0.0 {
                painter.line_segment([Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)], Stroke::new(1.0, color));
            } else {
                let mut x = plot.left();
                while x < plot.right() {
                    painter.line_segment(
                        [Pos2::new(x, y), Pos2::new((x + 4.0).min(plot.right()), y)],
                        Stroke::new(1.0, color),
                    );
                    x += 9.0;
                }
            }
            painter.text(
                Pos2::new(plot.left() - 2.0, y),
                Align2::RIGHT_CENTER,
                format!("{g:+.0}"),
                FontId::new(8.5, FontFamily::Proportional),
                th::faint(),
            );
        }
        for f in [20.0f32, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 20000.0] {
            let t = ((f / 20.0).log10() / 1000.0f32.log10()).clamp(0.0, 1.0);
            let x = plot.left() + t * plot.width();
            painter.line_segment(
                [Pos2::new(x, plot.top()), Pos2::new(x, plot.bottom())],
                Stroke::new(1.0, th::border().gamma_multiply(0.25)),
            );
            painter.text(
                Pos2::new(x, plot.bottom() + 12.0),
                Align2::CENTER_CENTER,
                presets::freq_label(f),
                FontId::new(8.5, FontFamily::Proportional),
                th::faint(),
            );
        }

        let count = self.editor.bands.len();
        if count == 0 {
            return;
        }
        let points: Vec<Pos2> = self
            .editor
            .bands
            .iter()
            .enumerate()
            .map(|(i, b)| Pos2::new(x_for(i, count), y_for(b.gain)))
            .collect();

        // Filled area: vertical strips with a soft gradient.
        let columns = 110;
        for col in 0..columns {
            let t0 = col as f32 / columns as f32;
            let t1 = (col + 1) as f32 / columns as f32;
            let x0 = plot.left() + t0 * plot.width();
            let x1 = plot.left() + t1 * plot.width();
            let y = sample_curve(&points, x0);
            let baseline = y_for(0.0);
            if (baseline - y).abs() < 0.5 {
                continue;
            }
            let top = y.min(baseline);
            let bottom = y.max(baseline);
            let color = if y < baseline {
                th::accent().gamma_multiply(0.16 + 0.10 * (1.0 - (baseline - y) / plot.height()))
            } else {
                Color32::from_rgb(255, 120, 150).gamma_multiply(0.10)
            };
            painter.rect_filled(
                Rect::from_min_max(Pos2::new(x0, top), Pos2::new(x1 + 1.0, bottom)),
                CornerRadius::ZERO,
                color,
            );
        }

        // Glow + curve
        painter.add(Shape::line(points.clone(), Stroke::new(6.0, th::accent().gamma_multiply(0.10))));
        painter.add(Shape::line(points.clone(), Stroke::new(2.0, th::accent())));

        // Bars + handles
        for (i, b) in self.editor.bands.iter().enumerate() {
            let x = x_for(i, count);
            let y = y_for(b.gain);
            let base = y_for(0.0);
            let color = if b.gain >= 0.0 { th::accent() } else { Color32::from_rgb(255, 120, 150) };
            painter.rect_filled(
                Rect::from_min_max(Pos2::new(x - 2.0, y.min(base)), Pos2::new(x + 2.0, y.max(base))),
                CornerRadius::same(2),
                color.gamma_multiply(0.28),
            );
        }

        // Interaction
        if let Some(pos) = response.interact_pointer_pos() {
            if response.clicked() || response.drag_started() {
                self.drag_band = nearest_band(&self.editor.bands, pos.x, &plot);
            }
            if response.dragged() {
                if let Some(idx) = self.drag_band {
                    let gain = ((y_for(0.0) - pos.y) / (plot.height() * 0.46) * 12.0).clamp(-12.0, 12.0);
                    let snapped = (gain * 2.0).round() / 2.0;
                    if (self.editor.bands[idx].gain - snapped).abs() > 0.001 {
                        self.editor.bands[idx].gain = snapped;
                        self.mark_edit(now);
                    }
                }
            }
        }
        if response.drag_stopped() {
            self.drag_band = None;
        }

        if response.double_clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                if let Some(idx) = nearest_band(&self.editor.bands, pos.x, &plot) {
                    self.editor.bands[idx].gain = 0.0;
                    self.mark_edit(now);
                }
            }
        }

        self.hover_band = response
            .hover_pos()
            .and_then(|pos| nearest_band(&self.editor.bands, pos.x, &plot));

        for (i, b) in self.editor.bands.iter().enumerate() {
            let x = x_for(i, count);
            let y = y_for(b.gain);
            let active = self.drag_band == Some(i) || self.hover_band == Some(i);
            let radius = if active { 5.5 } else { 3.2 };
            painter.circle_filled(Pos2::new(x, y), radius + 3.0, th::accent().gamma_multiply(if active { 0.18 } else { 0.07 }));
            painter.circle_filled(Pos2::new(x, y), radius, if active { Color32::WHITE } else { th::accent() });
            if active {
                painter.circle_stroke(
                    Pos2::new(x, y),
                    radius + 3.0,
                    Stroke::new(1.0, th::accent().gamma_multiply(0.6)),
                );
            }
        }

        if let Some(idx) = self.hover_band.or(self.drag_band) {
            if let Some(b) = self.editor.bands.get(idx) {
                let text = format!("{} Hz  ·  {:+.1} dB", presets::freq_label(b.freq), b.gain);
                let galley = painter.layout_no_wrap(text, FontId::new(11.0, FontFamily::Proportional), th::text());
                let pos = Pos2::new(
                    (x_for(idx, count) + 12.0).min(plot.right() - galley.size().x - 12.0),
                    (y_for(b.gain) - 30.0).max(plot.top() + 4.0),
                );
                let bg = Rect::from_min_size(pos, galley.size() + Vec2::new(16.0, 8.0));
                painter.rect_filled(bg, CornerRadius::same(6), th::card_hi());
                painter.rect_stroke(bg, CornerRadius::same(6), Stroke::new(1.0, th::border()), egui::StrokeKind::Inside);
                painter.galley(pos + Vec2::new(8.0, 4.0), galley, th::text());
            }
        }
    }

    fn tab_presets(&mut self, ui: &mut egui::Ui, now: f64) {
        self.page_header(ui, "Presets", "Built-in FxSound profiles and your own saved snapshots.", |app, ui| {
            if primary_button(ui, "Save current").clicked() {
                app.new_preset_name.clear();
                app.dialog = Dialog::SaveNew;
            }
        });

        let built_in: Vec<String> = self
            .fx_status
            .as_ref()
            .map(|s| s.presets.built_in.iter().map(|p| p.name.clone()).collect())
            .unwrap_or_default();
        let selected = self
            .fx_status
            .as_ref()
            .map(|s| s.selected_preset.clone())
            .unwrap_or_default();

        ui.horizontal(|ui| {
            ui.label(RichText::new("FXSOUND PRESETS").size(10.0).strong().color(th::faint()));
            ui.label(RichText::new("click any card to load it into the engine").size(10.0).color(th::faint()));
        });
        ui.add_space(4.0);

        if built_in.is_empty() {
            th::card().show(ui, |ui| {
                empty_state(ui, "No FxSound presets yet", "Start FxSound once so beanxsound can read its preset library.");
            });
        } else {
            let mut clicked: Option<String> = None;
            for chunk in built_in.chunks(3) {
                ui.columns(3, |cols| {
                    for (i, name) in chunk.iter().enumerate() {
                        if builtin_preset_tile(&mut cols[i], name, *name == selected) {
                            clicked = Some(name.clone());
                        }
                    }
                });
                ui.add_space(4.0);
            }
            if let Some(name) = clicked {
                self.select_preset(&name, now);
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Save current FxSound state as a preset:").size(11.5).color(th::muted()));
                ui.add(egui::TextEdit::singleline(&mut self.new_preset_name).hint_text("Preset name").desired_width(190.0));
                if ghost_button(ui, "Save to FxSound").clicked() {
                    let name = self.new_preset_name.trim().to_string();
                    if !name.is_empty() {
                        self.fx.apply(vec![format!("--save_preset={name}")]);
                        self.toast("Asked FxSound to save the preset", true);
                        self.new_preset_name.clear();
                    }
                }
            });
        }

        ui.add_space(18.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("YOUR PRESETS").size(10.0).strong().color(th::faint()));
            ui.label(RichText::new(format!("{} saved", self.presets.presets.len())).size(10.0).color(th::faint()));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ghost_button(ui, "Open folder").clicked() {
                    if let Some(parent) = presets::store_path().parent() {
                        audio::open_target(&parent.display().to_string());
                    }
                }
            });
        });
        ui.add_space(4.0);

        if self.presets.presets.is_empty() {
            th::card().show(ui, |ui| {
                empty_state(
                    ui,
                    "No custom presets yet",
                    "Dial in the equalizer, then press “Save current” to freeze the whole state as a reusable profile.",
                );
            });
            return;
        }

        let mut action: Option<(String, &'static str)> = None;
        let presets = self.presets.presets.clone();
        for chunk in presets.chunks(3) {
            ui.columns(3, |cols| {
                for (i, preset) in chunk.iter().enumerate() {
                    let col = &mut cols[i];
                    th::card().show(col, |ui| {
                        ui.label(RichText::new(&preset.name).size(15.0).strong().color(th::text()));
                        ui.label(
                            RichText::new(format!(
                                "{} bands · {:+.1} dB · Q {:.1}",
                                preset.bands.len(),
                                preset.master_gain,
                                preset.filter_q
                            ))
                            .size(10.0)
                            .color(th::faint()),
                        );
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if primary_button(ui, "Apply").clicked() {
                                action = Some((preset.name.clone(), "apply"));
                            }
                            if ghost_button(ui, "Rename").clicked() {
                                action = Some((preset.name.clone(), "rename"));
                            }
                            if ghost_button(ui, "Delete").clicked() {
                                action = Some((preset.name.clone(), "delete"));
                            }
                        });
                    });
                }
            });
            ui.add_space(4.0);
        }
        if let Some((name, kind)) = action {
            match kind {
                "apply" => {
                    if let Some(preset) = self.presets.presets.iter().find(|p| p.name == name).cloned() {
                        self.editor = editor_from_preset(&preset);
                        self.apply_editor(now);
                        self.toast(format!("Applied “{name}”"), true);
                    }
                }
                "rename" => {
                    self.rename_name = name.clone();
                    self.dialog = Dialog::Rename(name);
                }
                "delete" => {
                    self.dialog = Dialog::Delete(name);
                }
                _ => {}
            }
        }

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if ghost_button(ui, "Export presets…").clicked() {
                let path = rfd_save();
                if let Some(path) = path {
                    match std::fs::write(&path, self.presets.export_json()) {
                        Ok(()) => self.toast("Presets exported", true),
                        Err(err) => self.toast(format!("Export failed: {err}"), false),
                    }
                }
            }
            if ghost_button(ui, "Import presets…").clicked() {
                if let Some(path) = rfd_open() {
                    match std::fs::read_to_string(&path) {
                        Ok(text) => match self.presets.import_json(&text) {
                            Ok(count) => {
                                self.toast(format!("Imported {count} presets"), true);
                                self.refresh_fx();
                            }
                            Err(err) => self.toast(err, false),
                        },
                        Err(err) => self.toast(format!("Could not read file: {err}"), false),
                    }
                }
            }
        });
    }

    fn tab_advanced(&mut self, ui: &mut egui::Ui, _now: f64) {
        self.page_header(ui, "Advanced", "System tools, truthful diagnostics, and the odd escape hatch.", |_app, _ui| {});

        th::card().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Appearance").size(16.0).strong().color(th::text()));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{} finishes · saved between launches", th::themes().len()))
                            .size(9.5)
                            .color(th::faint()),
                    );
                });
            });
            ui.label(
                RichText::new("Black glass, liquid chrome, and pink light. Pick a finish — the chrome follows you around the app.")
                    .size(10.5)
                    .color(th::faint()),
            );
            ui.add_space(8.0);
            let current = th::theme_index();
            ui.horizontal_wrapped(|ui| {
                for (i, theme) in th::themes().iter().enumerate() {
                    if theme_tile(ui, theme, i == current).clicked() {
                        apply_theme(ui.ctx(), i);
                        self.toast(format!("Theme: {}", theme.name), true);
                    }
                }
            });
        });

        ui.add_space(10.0);

        ui.columns(2, |cols| {
            th::card().show(&mut cols[0], |ui| {
                ui.label(RichText::new("Windows audio tools").size(16.0).strong().color(th::text()));
                ui.label(RichText::new("Jump straight to the relevant Control Panel applets.").size(10.5).color(th::faint()));
                ui.add_space(8.0);
                if ghost_button(ui, "Sound settings").clicked() {
                    audio::open_target("ms-settings:sound");
                }
                if ghost_button(ui, "Sound control panel").clicked() {
                    audio::open_target("control.exe mmsys.cpl,,0");
                }
                if ghost_button(ui, "Recording devices").clicked() {
                    audio::open_target("control.exe mmsys.cpl,,1");
                }
                if ghost_button(ui, "Device Manager").clicked() {
                    audio::open_target("devmgmt.msc");
                }
                if ghost_button(ui, "Startup apps").clicked() {
                    audio::open_target("ms-settings:startupapps");
                }
            });

            th::card().show(&mut cols[1], |ui| {
                ui.label(RichText::new("beanxsound").size(16.0).strong().color(th::text()));
                ui.label(RichText::new("A native shell around the tools Windows already ships.").size(10.5).color(th::faint()));
                ui.add_space(8.0);
                if ghost_button(ui, "Pin to taskbar").clicked() {
                    match winshell::pin_to_taskbar() {
                        Ok(()) => self.toast("Pinned. If Windows asked, approve the prompt.", true),
                        Err(err) => self.toast(err, false),
                    }
                }
                if ghost_button(ui, "Create Start Menu shortcut").clicked() {
                    match winshell::ensure_shortcut() {
                        Ok(path) => self.toast(format!("Shortcut ready: {}", path.display()), true),
                        Err(err) => self.toast(err, false),
                    }
                }
                if ghost_button(ui, "Open install folder").clicked() {
                    if let Ok(exe) = std::env::current_exe() {
                        if let Some(parent) = exe.parent() {
                            audio::open_target(&parent.display().to_string());
                        }
                    }
                }
                if ghost_button(ui, "Restart as administrator").clicked() {
                    match winshell::relaunch_elevated() {
                        Ok(()) => {
                            self.toast("Elevation prompt sent — this window can be closed.", true);
                        }
                        Err(err) => self.toast(err, false),
                    }
                }
                if ghost_button(ui, "About beanxsound").clicked() {
                    self.dialog = Dialog::About;
                }
            });
        });

        ui.add_space(10.0);

        th::card().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Diagnostics").size(16.0).strong().color(th::text()));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ghost_button(ui, "Refresh").clicked() {
                        self.refresh_audio();
                        self.refresh_fx();
                    }
                });
            });
            ui.add_space(8.0);
            let mut rows: Vec<(&str, String, Color32)> = vec![
                (
                    "Audio engine",
                    match &self.audio_error {
                        None => "WASAPI connected".to_string(),
                        Some(err) => err.clone(),
                    },
                    if self.audio_error.is_none() { th::green() } else { th::red() },
                ),
                (
                    "Administrator",
                    if self.admin { "Elevated".to_string() } else { "Standard user (device enable/disable needs elevation)".to_string() },
                    if self.admin { th::green() } else { th::amber() },
                ),
                (
                    "FxSound engine",
                    match (&self.fx.exe, self.fx_running) {
                        (Some(exe), true) => format!("Running · {}", exe.display()),
                        (Some(exe), false) => format!("Installed · not running · {}", exe.display()),
                        (None, _) => "Not installed".to_string(),
                    },
                    if self.fx_running { th::green() } else { th::amber() },
                ),
                (
                    "Status bridge",
                    self.fx.status_path.display().to_string(),
                    th::muted(),
                ),
                (
                    "Preset store",
                    presets::store_path().display().to_string(),
                    th::muted(),
                ),
                (
                    "Architecture",
                    format!("{} · {}", std::env::consts::ARCH, std::env::consts::OS),
                    th::muted(),
                ),
            ];
            if let Some(status) = &self.fx_status {
                rows.push((
                    "FxSound version",
                    format!("{} · {} bands · Q {:.1}", status.version, status.equalizer.num_bands, status.equalizer.filter_q),
                    th::muted(),
                ));
            }
            for (label, value, color) in rows {
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [150.0, 18.0],
                        egui::Label::new(RichText::new(label).size(10.5).color(th::faint())),
                    );
                    ui.label(RichText::new(value).size(11.0).color(color).monospace());
                });
            }
        });

        ui.add_space(10.0);
        th::card().show(ui, |ui| {
            ui.label(RichText::new("A word on safety").size(14.0).strong().color(th::text()));
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "beanxsound talks to WASAPI and the official FxSound CLI. It never touches audio drivers, never bundles FxSound code, and keeps every setting reversible from Windows itself.",
                )
                .size(11.5)
                .color(th::muted()),
            );
        });
    }

    fn dialogs(&mut self, ctx: &egui::Context, now: f64) {
        match &self.dialog {
            Dialog::None => {}
            Dialog::SaveNew => {
                let mut open = true;
                let mut close = false;
                egui::Window::new("Save preset")
                    .open(&mut open)
                    .collapsible(false)
                    .resizable(false)
                    .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                    .frame(th::card().inner_margin(Margin::same(20)))
                    .show(ctx, |ui| {
                        ui.set_width(340.0);
                        ui.label(RichText::new("Freeze the current equalizer, effects, and gain as a reusable profile.").size(11.0).color(th::muted()));
                        ui.add_space(8.0);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.new_preset_name)
                                .hint_text("e.g. Late night headphones")
                                .desired_width(f32::INFINITY),
                        );
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            if primary_button(ui, "Save preset").clicked() {
                                let name = self.new_preset_name.trim().to_string();
                                if name.is_empty() {
                                    self.toast("Give the preset a name first.", false);
                                } else {
                                    let preset = preset_from_editor(&name, &self.editor);
                                    match self.presets.add(preset) {
                                        Ok(()) => {
                                            self.toast(format!("Saved “{name}”"), true);
                                            close = true;
                                        }
                                        Err(err) => self.toast(err, false),
                                    }
                                }
                            }
                            if ghost_button(ui, "Cancel").clicked() {
                                close = true;
                            }
                        });
                    });
                if close || !open {
                    self.dialog = Dialog::None;
                }
            }
            Dialog::Rename(old) => {
                let old = old.clone();
                let mut open = true;
                let mut close = false;
                egui::Window::new("Rename preset")
                    .open(&mut open)
                    .collapsible(false)
                    .resizable(false)
                    .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                    .frame(th::card().inner_margin(Margin::same(20)))
                    .show(ctx, |ui| {
                        ui.set_width(340.0);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.rename_name)
                                .hint_text("Preset name")
                                .desired_width(f32::INFINITY),
                        );
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            if primary_button(ui, "Rename").clicked() {
                                match self.presets.rename(&old, &self.rename_name) {
                                    Ok(()) => {
                                        self.toast("Preset renamed", true);
                                        close = true;
                                    }
                                    Err(err) => self.toast(err, false),
                                }
                            }
                            if ghost_button(ui, "Cancel").clicked() {
                                close = true;
                            }
                        });
                    });
                if close || !open {
                    self.dialog = Dialog::None;
                }
            }
            Dialog::Delete(name) => {
                let name = name.clone();
                let mut open = true;
                let mut close = false;
                egui::Window::new("Delete preset")
                    .open(&mut open)
                    .collapsible(false)
                    .resizable(false)
                    .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                    .frame(th::card().inner_margin(Margin::same(20)))
                    .show(ctx, |ui| {
                        ui.set_width(340.0);
                        ui.label(RichText::new(format!("Delete “{name}”?")).size(15.0).strong().color(th::text()));
                        ui.label(RichText::new("This only removes your beanxsound copy. FxSound presets are untouched.").size(11.0).color(th::muted()));
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            if primary_button(ui, "Delete").clicked() {
                                match self.presets.remove(&name) {
                                    Ok(()) => {
                                        self.toast("Preset deleted", true);
                                        close = true;
                                    }
                                    Err(err) => self.toast(err, false),
                                }
                            }
                            if ghost_button(ui, "Keep it").clicked() {
                                close = true;
                            }
                        });
                    });
                if close || !open {
                    self.dialog = Dialog::None;
                }
            }
            Dialog::About => {
                let mut open = true;
                egui::Window::new("About beanxsound")
                    .open(&mut open)
                    .collapsible(false)
                    .resizable(false)
                    .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                    .frame(th::card().inner_margin(Margin::same(22)))
                    .show(ctx, |ui| {
                        ui.set_width(380.0);
                        ui.horizontal(|ui| {
                            logo_mark(ui, 40.0);
                            ui.add_space(4.0);
                            ui.vertical(|ui| {
                                brand_text(ui, 24.0);
                                ui.label(RichText::new("Windows audio, with taste.").size(11.0).color(th::muted()));
                            });
                        });
                        ui.add_space(12.0);
                        ui.label(RichText::new(format!("Version {} · {} · {}", env!("CARGO_PKG_VERSION"), std::env::consts::ARCH, std::env::consts::OS)).size(11.0).color(th::muted()));
                        ui.add_space(8.0);
                        ui.label(RichText::new("beanxsound is an independent control surface for Windows audio and the FxSound engine. FxSound is a trademark of FxSound LLC; this project is not affiliated with or endorsed by them.").size(11.0).color(th::muted()));
                        ui.add_space(10.0);
                        if ghost_button(ui, "Close").clicked() {
                            ui.close();
                        }
                    });
                if !open {
                    self.dialog = Dialog::None;
                }
            }
        }
        let _ = now;
    }

    fn toast_ui(&mut self, ctx: &egui::Context) {
        if let Some(toast) = &self.toast {
            egui::Area::new(Id::unique("toast"))
                .order(egui::Order::Foreground)
                .anchor(Align2::RIGHT_BOTTOM, Vec2::new(-22.0, -30.0))
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(th::card_hi())
                        .stroke(Stroke::new(
                            1.0,
                            if toast.kind { th::green().gamma_multiply(0.6) } else { th::red().gamma_multiply(0.6) },
                        ))
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(Margin::symmetric(14, 10))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let color = if toast.kind { th::green() } else { th::red() };
                                ui.label(RichText::new(if toast.kind { "●" } else { "▲" }).size(9.0).color(color));
                                ui.label(RichText::new(&toast.text).size(12.0).color(th::text()));
                            });
                        });
                });
            ctx.request_repaint_after(Duration::from_millis(200));
        }
    }
}

impl eframe::App for BeanXSound {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        self.poll(now);

        egui::Panel::top("top_bar")
            .exact_size(62.0)
            .frame(
                egui::Frame::new()
                    .fill(th::panel())
                    .inner_margin(Margin::symmetric(18, 12)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    logo_mark(ui, 34.0);
                    ui.add_space(2.0);
                    ui.scope(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        brand_text(ui, 21.0);
                    });
                    ui.add_space(6.0);
                    ui.label(RichText::new("WINDOWS AUDIO").size(8.5).strong().color(th::faint()));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ghost_button(ui, "Pin").on_hover_text("Attempt to pin beanxsound to the taskbar").clicked() {
                            match winshell::pin_to_taskbar() {
                                Ok(()) => self.toast("Pin request sent to Windows", true),
                                Err(err) => self.toast(err, false),
                            }
                        }
                        if ghost_button(ui, "Refresh").clicked() {
                            self.refresh_audio();
                            self.refresh_fx();
                        }
                        if self.fx_running {
                            th::chip(ui, "FXSOUND LIVE", th::green());
                        } else {
                            th::chip(ui, "FXSOUND IDLE", th::faint());
                        }
                        let active = self.snapshot.devices.iter().filter(|d| d.is_active()).count();
                        th::chip(ui, &format!("{} ACTIVE", active), th::accent());
                        ui.add_space(2.0);
                        theme_swatches(ui, &ctx);
                    });
                });
                let clip = ui.clip_rect();
                let line = Rect::from_min_max(
                    Pos2::new(clip.left(), clip.bottom() - 1.5),
                    Pos2::new(clip.right(), clip.bottom()),
                );
                th::gradient_rect(
                    ui.painter(),
                    line,
                    th::accent().gamma_multiply(0.85),
                    th::accent2().gamma_multiply(0.85),
                    true,
                );
            });

        egui::Panel::bottom("status_bar")
            .exact_size(28.0)
            .frame(
                egui::Frame::new()
                    .fill(th::panel())
                    .inner_margin(Margin::symmetric(18, 5)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("●").size(7.0).color(if self.audio_error.is_none() { th::green() } else { th::red() }));
                    ui.label(RichText::new(if let Some(err) = &self.audio_error { err.clone() } else { "WASAPI connected".to_string() }).size(10.0).color(th::muted()));
                    ui.add_space(10.0);
                    ui.label(RichText::new(format!("FxSound {}", self.fx_status.as_ref().map(|s| s.version.clone()).unwrap_or_else(|| "—".into()))).size(10.0).color(th::faint()));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(format!("{} · {}", std::env::consts::ARCH, "beanxsound 0.1.0")).size(10.0).color(th::faint()));
                        if self.admin {
                            ui.label(RichText::new("admin").size(10.0).color(th::green()));
                        }
                        ui.label(RichText::new(th::theme_name()).size(10.0).color(th::accent()));
                    });
                });
            });

        egui::Panel::left("nav")
            .exact_size(216.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(th::panel())
                    .inner_margin(Margin::symmetric(11, 16)),
            )
            .show(ui, |ui| {
                ui.label(RichText::new("SOUND").size(9.0).strong().color(th::faint()));
                ui.add_space(4.0);
                for tab in [Tab::Devices, Tab::Mixer, Tab::FxSound] {
                    if nav_item(ui, self.tab == tab, tab) {
                        self.tab = tab;
                    }
                    ui.add_space(2.0);
                }
                ui.add_space(14.0);
                ui.label(RichText::new("LIBRARY").size(9.0).strong().color(th::faint()));
                ui.add_space(4.0);
                for tab in [Tab::Presets, Tab::Advanced] {
                    if nav_item(ui, self.tab == tab, tab) {
                        self.tab = tab;
                    }
                    ui.add_space(2.0);
                }

                ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                    th::inner_card().show(ui, |ui| {
                        ui.label(RichText::new("beanxsound 0.1.0").size(10.5).strong().color(th::text()));
                        ui.label(RichText::new("A calmer control room for Windows audio.").size(9.5).color(th::faint()));
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            th::chip(ui, "WASAPI", th::accent());
                            th::chip(ui, "FxSound", th::green());
                        });
                    });
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(th::bg()))
            .show(ui, |ui| {
                paint_ambient(ui);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .id_salt(("page", self.tab as u8))
                    .show(ui, |ui| {
                        ui.add_space(20.0);
                        ui.horizontal(|ui| {
                            ui.add_space(24.0);
                            ui.vertical(|ui| {
                                ui.set_width((ui.available_width() - 24.0).clamp(480.0, 1080.0));
                                match self.tab {
                                    Tab::Devices => self.tab_devices(ui, now),
                                    Tab::Mixer => self.tab_mixer(ui, now),
                                    Tab::FxSound => self.tab_fxsound(ui, now),
                                    Tab::Presets => self.tab_presets(ui, now),
                                    Tab::Advanced => self.tab_advanced(ui, now),
                                }
                                ui.add_space(26.0);
                            });
                        });
                    });
            });

        self.dialogs(&ctx, now);
        self.toast_ui(&ctx);

        if self.editor_dirty {
            ctx.request_repaint_after(Duration::from_millis(120));
        } else {
            ctx.request_repaint_after(Duration::from_millis(180));
        }
    }
}

// ---------------------------------------------------------------------------
// Small painted widgets
// ---------------------------------------------------------------------------

fn brand_text(ui: &mut egui::Ui, size: f32) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label(RichText::new("bean").size(size).strong().color(th::text()));
        ui.label(RichText::new("x").size(size).strong().color(th::accent()));
        ui.label(RichText::new("sound").size(size).strong().color(th::text()));
    });
}

fn logo_mark(ui: &mut egui::Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let painter = ui.painter();
    let radius = CornerRadius::same((size * 0.30) as u8);
    let alpha = |c: Color32, a: f32| {
        Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
    };

    // Soft pink halo so the mark sits in the room rather than on top of it.
    for i in (1..=4).rev() {
        let halo = rect.expand(i as f32 * size * 0.05);
        painter.rect_filled(
            halo,
            CornerRadius::same((size * 0.30 + i as f32 * 1.4) as u8),
            alpha(th::accent(), 0.020),
        );
    }

    // Black glass tile.
    painter.rect_filled(rect, radius, th::input());
    th::gradient_rect(
        painter,
        rect,
        alpha(th::chrome(), 0.09),
        alpha(th::bg(), 0.0),
        false,
    );

    // Clipped gloss on the top half, the "liquid chrome" read.
    let clipped = painter.with_clip_rect(rect.shrink(1.0));
    let gloss = Rect::from_min_max(
        rect.left_top() + Vec2::new(1.0, 1.0),
        Pos2::new(rect.right() - 1.0, rect.top() + size * 0.48),
    );
    th::gradient_rect(
        &clipped,
        gloss,
        alpha(th::chrome(), 0.22),
        alpha(th::chrome(), 0.01),
        false,
    );

    // Chrome rim, brighter at the top edge.
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(1.0, alpha(th::chrome(), 0.30)),
        egui::StrokeKind::Inside,
    );
    painter.line_segment(
        [
            Pos2::new(rect.left() + size * 0.24, rect.top() + 1.0),
            Pos2::new(rect.right() - size * 0.24, rect.top() + 1.0),
        ],
        Stroke::new(1.0, alpha(th::chrome(), 0.55)),
    );

    // Five chrome-pink bars, tallest in the middle.
    let heights = [0.30f32, 0.58, 0.88, 0.54, 0.32];
    let bar_w = size * 0.095;
    let gap = size * 0.052;
    let total = bar_w * 5.0 + gap * 4.0;
    let mut x = rect.center().x - total / 2.0;
    for (i, h) in heights.iter().enumerate() {
        let h = size * h;
        let bar = Rect::from_min_size(
            Pos2::new(x, rect.center().y - h / 2.0),
            Vec2::new(bar_w, h),
        );
        let t = i as f32 / 4.0;
        let color = lerp_color(th::accent2(), th::accent(), t);
        painter.rect_filled(
            bar.expand(size * 0.035),
            CornerRadius::same(3),
            alpha(color, 0.16),
        );
        painter.rect_filled(bar, CornerRadius::same(2), color);
        let spark = Rect::from_min_max(
            bar.left_top() + Vec2::new(0.0, 1.0),
            Pos2::new(bar.right() - bar.width() * 0.35, bar.top() + h * 0.34),
        );
        painter.rect_filled(spark, CornerRadius::same(2), alpha(th::chrome(), 0.45));
        x += bar_w + gap;
    }
}

fn primary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(label).size(12.5).strong().color(Color32::WHITE))
            .fill(th::accent_dark())
            .stroke(Stroke::new(1.0, th::accent()))
            .corner_radius(CornerRadius::same(8)),
    )
}

fn ghost_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(label).size(11.5).color(th::text()))
            .fill(th::card_bg())
            .stroke(Stroke::new(1.0, th::border()))
            .corner_radius(CornerRadius::same(8)),
    )
}

fn apply_theme(ctx: &egui::Context, index: usize) {
    th::set_theme_index(index);
    th::apply(ctx);
    th::save_theme(index);
    ctx.request_repaint();
}

fn theme_swatches(ui: &mut egui::Ui, ctx: &egui::Context) {
    let current = th::theme_index();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (i, theme) in th::themes().iter().enumerate() {
            let (rect, resp) = ui.allocate_exact_size(Vec2::splat(17.0), Sense::click());
            let pal = theme.palette;
            let c = rect.center();
            let painter = ui.painter();
            painter.circle_filled(c, 6.4, pal.accent);
            painter.circle_filled(Pos2::new(c.x - 1.8, c.y - 2.0), 2.1, pal.chrome.gamma_multiply(0.9));
            if i == current {
                painter.circle_stroke(c, 7.8, Stroke::new(1.2, th::chrome()));
            } else if resp.hovered() {
                painter.circle_stroke(c, 7.8, Stroke::new(1.0, th::muted()));
            }
            if resp.hovered() {
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                apply_theme(ctx, i);
            }
            resp.on_hover_text(format!("{} — {}", theme.name, theme.note));
        }
    });
}

fn theme_tile(ui: &mut egui::Ui, theme: &th::Theme, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(208.0, 74.0), Sense::click());
    let pal = theme.palette;
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        CornerRadius::same(11),
        if selected { pal.card_hi } else { pal.card },
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(11),
        Stroke::new(
            if selected { 1.4 } else { 1.0 },
            if selected {
                pal.accent
            } else if resp.hovered() {
                pal.border
            } else {
                pal.border.gamma_multiply(0.7)
            },
        ),
        egui::StrokeKind::Inside,
    );
    let gloss = Rect::from_min_max(
        Pos2::new(rect.left() + 1.0, rect.top() + 1.0),
        Pos2::new(rect.right() - 1.0, rect.top() + 30.0),
    );
    painter.rect_filled(gloss, CornerRadius::same(9), pal.chrome.gamma_multiply(0.045));
    painter.text(
        Pos2::new(rect.left() + 12.0, rect.top() + 12.0),
        Align2::LEFT_TOP,
        theme.name,
        FontId::new(13.0, FontFamily::Proportional),
        th::text(),
    );
    painter.text(
        Pos2::new(rect.left() + 12.0, rect.top() + 31.0),
        Align2::LEFT_TOP,
        theme.note,
        FontId::new(9.0, FontFamily::Proportional),
        th::faint(),
    );
    for (j, color) in [
        pal.accent,
        pal.accent2,
        pal.chrome,
        pal.bg2,
        pal.card_hi,
    ]
    .iter()
    .enumerate()
    {
        painter.circle_filled(
            Pos2::new(rect.left() + 16.0 + j as f32 * 15.0, rect.bottom() - 13.0),
            5.5,
            *color,
        );
    }
    if selected {
        painter.text(
            Pos2::new(rect.right() - 11.0, rect.top() + 11.0),
            Align2::RIGHT_TOP,
            "ON",
            FontId::new(9.0, FontFamily::Proportional),
            pal.accent,
        );
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

fn nav_item(ui: &mut egui::Ui, selected: bool, tab: Tab) -> bool {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 46.0), Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, CornerRadius::same(10), th::accent_dark().gamma_multiply(0.72));
        painter.rect_stroke(
            rect,
            CornerRadius::same(10),
            Stroke::new(1.0, th::accent().gamma_multiply(0.45)),
            egui::StrokeKind::Inside,
        );
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(rect.left() + 1.0, rect.center().y - 9.0),
                Vec2::new(2.5, 18.0),
            ),
            CornerRadius::same(2),
            th::accent(),
        );
    } else if resp.hovered() {
        painter.rect_filled(rect, CornerRadius::same(10), th::card_bg().gamma_multiply(0.8));
    }
    let icon_color = if selected { Color32::WHITE } else { th::muted() };
    painter.text(
        Pos2::new(rect.left() + 18.0, rect.center().y),
        Align2::LEFT_CENTER,
        tab.icon(),
        FontId::new(15.0, FontFamily::Proportional),
        if selected { th::accent() } else { th::faint() },
    );
    painter.text(
        Pos2::new(rect.left() + 44.0, rect.center().y - 8.0),
        Align2::LEFT_CENTER,
        tab.label(),
        FontId::new(13.0, FontFamily::Proportional),
        icon_color,
    );
    painter.text(
        Pos2::new(rect.left() + 44.0, rect.center().y + 9.0),
        Align2::LEFT_CENTER,
        tab.subtitle(),
        FontId::new(9.0, FontFamily::Proportional),
        if selected { th::muted() } else { th::faint() },
    );
    resp.clicked()
}

fn toggle_switch(ui: &mut egui::Ui, on: bool) -> (Rect, egui::Response) {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(50.0, 28.0), Sense::click());
    let t = ui.ctx().animate_bool_with_time(resp.id, on, 0.16);
    let painter = ui.painter();
    let bg = lerp_color(th::card_hi(), th::accent(), t);
    painter.rect_filled(rect, CornerRadius::same(14), bg);
    painter.rect_stroke(
        rect,
        CornerRadius::same(14),
        Stroke::new(1.0, if on { th::accent() } else { th::border() }),
        egui::StrokeKind::Inside,
    );
    let knob_x = egui::lerp((rect.left() + 15.0)..=(rect.right() - 15.0), t);
    painter.circle_filled(Pos2::new(knob_x, rect.center().y), 10.0, if on { Color32::WHITE } else { th::muted() });
    (rect, resp)
}

fn meter_bar(ui: &mut egui::Ui, value: f32, width: f32, height: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same((height * 0.5) as u8), th::input());
    let filled = (value.clamp(0.0, 1.0) * rect.width()).max(0.0);
    if filled > 0.5 {
        let bar = Rect::from_min_size(rect.min, Vec2::new(filled, rect.height()));
        let color = th::peak_color(value);
        painter.rect_filled(bar, CornerRadius::same((height * 0.5) as u8), color.gamma_multiply(0.9));
        painter.rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(filled, rect.height() * 0.5)),
            CornerRadius::same((height * 0.5) as u8),
            Color32::WHITE.gamma_multiply(0.10),
        );
    }
}

fn device_card(
    ui: &mut egui::Ui,
    device: &mut AudioDevice,
    role: Role,
    expanded: &mut Option<String>,
    actions: &mut Vec<DeviceAction>,
) {
    let active = device.is_active();
    let is_default = match role {
        Role::Console => device.default_console,
        Role::Multimedia => device.default_multimedia,
        Role::Communications => device.default_communications,
    };
    egui::Frame::new()
        .fill(th::card_bg())
        .stroke(Stroke::new(
            1.0,
            if is_default { th::accent().gamma_multiply(0.65) } else { th::border() },
        ))
        .corner_radius(CornerRadius::same(13))
        .inner_margin(Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                device_badge(ui, device.is_capture, if active { th::accent() } else { th::faint() });
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&device.name).size(14.0).strong().color(if active { th::text() } else { th::muted() }));
                        if device.default_console {
                            th::chip(ui, "DEFAULT", th::accent());
                        }
                        if device.default_multimedia {
                            th::chip(ui, "MULTIMEDIA", th::accent());
                        }
                        if device.default_communications {
                            th::chip(ui, "COMMS", th::accent());
                        }
                        if !active {
                            th::chip(ui, device.state_label().to_uppercase().as_str(), th::faint());
                        }
                    });
                    ui.label(
                        RichText::new(if device.format_label().is_empty() {
                            if device.description.is_empty() { device.state_label().to_string() } else { device.description.clone() }
                        } else {
                            format!("{} · {}", device.description, device.format_label())
                        })
                        .size(10.0)
                        .color(th::faint()),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.menu_button(RichText::new("⋯").size(16.0), |ui| {
                        if ui.button("Set as default").clicked() {
                            actions.push(DeviceAction::SetDefault(device.id.clone(), role));
                            ui.close();
                        }
                        if ui.button("Set as multimedia default").clicked() {
                            actions.push(DeviceAction::SetDefault(device.id.clone(), Role::Multimedia));
                            ui.close();
                        }
                        if ui.button("Set as communications default").clicked() {
                            actions.push(DeviceAction::SetDefault(device.id.clone(), Role::Communications));
                            ui.close();
                        }
                        ui.separator();
                        if active {
                            if ui.button("Disable device").clicked() {
                                actions.push(DeviceAction::SetEnabled(device.id.clone(), false));
                                ui.close();
                            }
                        } else if ui.button("Enable device").clicked() {
                            actions.push(DeviceAction::SetEnabled(device.id.clone(), true));
                            ui.close();
                        }
                    });
                    if active {
                        if ghost_button(ui, if device.muted { "Unmute" } else { "Mute" }).clicked() {
                            let muted = !device.muted;
                            device.set_mute(muted);
                        }
                        let mut volume = device.volume;
                        let resp = ui.add_sized(
                            [200.0, 22.0],
                            egui::Slider::new(&mut volume, 0.0..=1.0)
                                .show_value(false)
                                .trailing_fill(true),
                        );
                        if resp.changed() {
                            device.set_volume(volume);
                        }
                        ui.add_sized(
                            [42.0, 18.0],
                            egui::Label::new(
                                RichText::new(audio::format_percent(device.volume))
                                    .size(11.0)
                                    .color(th::muted())
                                    .monospace(),
                            ),
                        );
                    } else if ghost_button(ui, "Enable").clicked() {
                        actions.push(DeviceAction::SetEnabled(device.id.clone(), true));
                    }
                });
            });

            if active {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add_space(48.0);
                    meter_bar(ui, device.peak, ui.available_width().min(340.0), 3.0);
                    ui.label(
                        RichText::new(if device.peak > 0.001 { format!("{:.0}% peak", device.peak * 100.0) } else { "silent".to_string() })
                            .size(9.0)
                            .color(th::faint()),
                    );
                });
                if device.channels > 1 {
                    let open = expanded.as_deref() == Some(device.id.as_str());
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.add_space(48.0);
                        if ui.selectable_label(open, format!("{} channels", device.channels)).clicked() {
                            *expanded = if open { None } else { Some(device.id.clone()) };
                        }
                    });
                    if open {
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.add_space(48.0);
                            for ch in 0..device.channel_volumes.len() {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(format!("{}", ch + 1)).size(9.5).color(th::faint()));
                                    let mut v = device.channel_volumes[ch];
                                    let resp = ui.add_sized(
                                        [88.0, 18.0],
                                        egui::Slider::new(&mut v, 0.0..=1.0).show_value(false),
                                    );
                                    if resp.changed() {
                                        device.set_channel(ch, v);
                                    }
                                });
                            }
                        });
                    }
                }
            }
        });
}

fn device_badge(ui: &mut egui::Ui, capture: bool, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(38.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(11), color.gamma_multiply(0.14));
    painter.rect_stroke(
        rect,
        CornerRadius::same(11),
        Stroke::new(1.0, color.gamma_multiply(0.35)),
        egui::StrokeKind::Inside,
    );
    if capture {
        let mic = Rect::from_center_size(rect.center() + Vec2::new(0.0, -2.0), Vec2::new(8.0, 13.0));
        painter.rect_filled(mic, CornerRadius::same(4), color);
        painter.line_segment(
            [
                Pos2::new(rect.center().x - 7.0, rect.center().y),
                Pos2::new(rect.center().x - 7.0, rect.center().y + 4.0),
            ],
            Stroke::new(1.5, color),
        );
        painter.line_segment(
            [
                Pos2::new(rect.center().x + 7.0, rect.center().y),
                Pos2::new(rect.center().x + 7.0, rect.center().y + 4.0),
            ],
            Stroke::new(1.5, color),
        );
        painter.line_segment(
            [
                Pos2::new(rect.center().x - 7.0, rect.center().y + 4.0),
                Pos2::new(rect.center().x + 7.0, rect.center().y + 4.0),
            ],
            Stroke::new(1.5, color),
        );
    } else {
        let box_rect = Rect::from_center_size(rect.center() + Vec2::new(-2.0, 0.0), Vec2::new(11.0, 15.0));
        painter.rect_filled(box_rect, CornerRadius::same(3), color);
        painter.add(Shape::convex_polygon(
            vec![
                Pos2::new(rect.center().x + 4.0, rect.center().y - 3.0),
                Pos2::new(rect.center().x + 11.0, rect.center().y - 8.0),
                Pos2::new(rect.center().x + 11.0, rect.center().y + 8.0),
                Pos2::new(rect.center().x + 4.0, rect.center().y + 3.0),
            ],
            color,
            Stroke::NONE,
        ));
    }
}

fn app_badge(ui: &mut egui::Ui, letter: &str, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(9), color.gamma_multiply(0.15));
    painter.rect_stroke(
        rect,
        CornerRadius::same(9),
        Stroke::new(1.0, color.gamma_multiply(0.35)),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        letter,
        FontId::new(13.0, FontFamily::Proportional),
        color,
    );
}

fn effect_row(ui: &mut egui::Ui, name: &str, hint: &str, color: Color32, value: &mut f32) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let (dot, _) = ui.allocate_exact_size(Vec2::new(8.0, 8.0), Sense::hover());
        ui.painter().circle_filled(dot.center(), 3.0, color);
        ui.add_sized(
            [112.0, 18.0],
            egui::Label::new(RichText::new(name).size(12.0).strong().color(th::text())),
        );
        ui.add_sized(
            [180.0, 18.0],
            egui::Label::new(RichText::new(hint).size(9.5).color(th::faint())),
        );
        let mut local = *value;
        let resp = ui.scope(|ui| {
            ui.visuals_mut().selection.bg_fill = color;
            ui.visuals_mut().widgets.inactive.bg_fill = th::card_hi();
            ui.add_sized(
                [ui.available_width() - 54.0, 20.0],
                egui::Slider::new(&mut local, 0.0..=10.0)
                    .show_value(false)
                    .trailing_fill(true),
            )
        });
        if resp.inner.changed() {
            *value = local;
            changed = true;
        }
        ui.label(
            RichText::new(format!("{:.1}", local))
                .size(11.0)
                .strong()
                .color(color)
                .monospace(),
        );
    });
    ui.add_space(3.0);
    changed
}

fn signal_row(
    ui: &mut egui::Ui,
    label: &str,
    suffix: &str,
    range: std::ops::RangeInclusive<f32>,
    step: f64,
    value: &mut f32,
) -> bool {
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.add_sized(
            [116.0, 18.0],
            egui::Label::new(RichText::new(label).size(11.0).color(th::muted())),
        );
        let mut local = *value;
        let resp = ui.add_sized(
            [ui.available_width() - 64.0, 20.0],
            egui::Slider::new(&mut local, range)
                .show_value(false)
                .step_by(step)
                .trailing_fill(true),
        );
        if resp.changed() {
            *value = local;
            return true;
        }
        ui.label(
            RichText::new(format!("{:+.1} {suffix}", local))
                .size(10.5)
                .color(th::muted())
                .monospace(),
        );
        false
    })
    .inner
}

fn empty_state(ui: &mut egui::Ui, title: &str, body: &str) {
    ui.add_space(10.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new("◌").size(22.0).color(th::faint()));
        ui.label(RichText::new(title).size(14.0).strong().color(th::text()));
        ui.label(RichText::new(body).size(11.0).color(th::muted()));
    });
    ui.add_space(10.0);
}

fn builtin_preset_tile(ui: &mut egui::Ui, name: &str, selected: bool) -> bool {
    let card = egui::Frame::new()
        .fill(if selected { th::accent_dark().gamma_multiply(0.45) } else { th::card_bg() })
        .stroke(Stroke::new(
            1.0,
            if selected { th::accent() } else { th::border() },
        ))
        .corner_radius(CornerRadius::same(13))
        .inner_margin(Margin::symmetric(13, 11))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                th::icon_badge(ui, "♪", if selected { th::accent() } else { th::faint() });
                ui.vertical(|ui| {
                    ui.label(RichText::new(name).size(13.0).strong().color(th::text()));
                    ui.label(
                        RichText::new(if selected { "Currently loaded" } else { "FxSound built-in" })
                            .size(9.5)
                            .color(if selected { th::accent() } else { th::faint() }),
                    );
                });
            });
        });
    let resp = ui.interact(
        card.response.rect,
        Id::unique(("builtin_preset", name)),
        Sense::click(),
    );
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        ui.painter().rect_stroke(
            card.response.rect,
            CornerRadius::same(13),
            Stroke::new(1.0, th::accent().gamma_multiply(0.7)),
            egui::StrokeKind::Inside,
        );
    }
    resp.clicked()
}

fn sample_curve(points: &[Pos2], x: f32) -> f32 {
    if points.is_empty() {
        return 0.0;
    }
    if x <= points[0].x {
        return points[0].y;
    }
    for pair in points.windows(2) {
        if x >= pair[0].x && x <= pair[1].x {
            let t = ((x - pair[0].x) / (pair[1].x - pair[0].x).max(0.001)).clamp(0.0, 1.0);
            return egui::lerp(pair[0].y..=pair[1].y, t);
        }
    }
    points.last().map(|p| p.y).unwrap_or(0.0)
}

fn nearest_band(bands: &[EqBand], x: f32, plot: &Rect) -> Option<usize> {
    if bands.is_empty() {
        return None;
    }
    let count = bands.len();
    if count == 1 {
        return Some(0);
    }
    let t = ((x - plot.left()) / plot.width()).clamp(0.0, 1.0);
    let idx = (t * (count - 1) as f32).round() as usize;
    Some(idx.min(count - 1))
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgba_unmultiplied(
        egui::lerp(a.r() as f32..=b.r() as f32, t) as u8,
        egui::lerp(a.g() as f32..=b.g() as f32, t) as u8,
        egui::lerp(a.b() as f32..=b.b() as f32, t) as u8,
        egui::lerp(a.a() as f32..=b.a() as f32, t) as u8,
    )
}

fn paint_ambient(ui: &mut egui::Ui) {
    let rect = ui.max_rect();
    let painter = ui.painter();
    let p = th::palette();
    let alpha = |c: Color32, a: f32| {
        Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
    };

    // Base wash: slightly lifted at the top, falling to pure black.
    th::gradient_rect(
        painter,
        rect,
        p.bg2,
        p.bg,
        false,
    );

    // Pink light bleeding in from the top right.
    let c1 = Pos2::new(rect.right() - 70.0, rect.top() + 30.0);
    for i in (1..=12).rev() {
        let r = 60.0 + i as f32 * 34.0;
        painter.circle_filled(c1, r, alpha(p.accent, 0.013));
    }

    // Cool chrome/cyan counter-light low on the left.
    let c2 = Pos2::new(rect.left() + 90.0, rect.bottom() - 20.0);
    for i in (1..=9).rev() {
        let r = 80.0 + i as f32 * 40.0;
        painter.circle_filled(c2, r, alpha(p.accent2, 0.010));
    }

    // Chrome sheen across the top of the page.
    let sheen = Rect::from_min_max(
        rect.left_top(),
        Pos2::new(rect.right(), rect.top() + 180.0),
    );
    th::gradient_rect(
        painter,
        sheen,
        alpha(p.chrome, 0.055),
        alpha(p.chrome, 0.0),
        false,
    );

    // Faint Y2K hairline grid.
    let grid = alpha(p.chrome, 0.022);
    let step = 46.0;
    let mut x = rect.left();
    while x < rect.right() {
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            Stroke::new(1.0, grid),
        );
        x += step;
    }
    let mut y = rect.top();
    while y < rect.bottom() {
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            Stroke::new(1.0, grid),
        );
        y += step;
    }

    // Horizon: a single accent line so the grid reads as designed, not noisy.
    painter.line_segment(
        [
            Pos2::new(rect.left(), rect.top() + 180.0),
            Pos2::new(rect.right(), rect.top() + 180.0),
        ],
        Stroke::new(1.0, alpha(p.accent, 0.09)),
    );
}

fn editor_from_preset(preset: &UserPreset) -> Editor {
    Editor {
        bands: preset.bands.clone(),
        master_gain: preset.master_gain,
        balance: preset.balance,
        volume_leveling: preset.volume_leveling,
        filter_q: preset.filter_q,
        effects: preset.effects.clone(),
    }
}

fn preset_from_editor(name: &str, editor: &Editor) -> UserPreset {
    UserPreset {
        name: name.to_string(),
        bands: editor.bands.clone(),
        master_gain: editor.master_gain,
        balance: editor.balance,
        volume_leveling: editor.volume_leveling,
        filter_q: editor.filter_q,
        effects: editor.effects.clone(),
    }
}

fn rfd_open() -> Option<std::path::PathBuf> {
    let script = r#"
Add-Type -AssemblyName System.Windows.Forms
$d = New-Object System.Windows.Forms.OpenFileDialog
$d.Filter = 'beanxsound presets (*.json)|*.json|All files (*.*)|*.*'
if ($d.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { $d.FileName }
"#;
    run_dialog(script)
}

fn rfd_save() -> Option<std::path::PathBuf> {
    let script = r#"
Add-Type -AssemblyName System.Windows.Forms
$d = New-Object System.Windows.Forms.SaveFileDialog
$d.Filter = 'beanxsound presets (*.json)|*.json'
$d.FileName = 'beanxsound-presets.json'
if ($d.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { $d.FileName }
"#;
    run_dialog(script)
}

fn run_dialog(script: &str) -> Option<std::path::PathBuf> {
    use std::os::windows::process::CommandExt;
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-STA", "-Command", script])
        .creation_flags(0x08000000)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(std::path::PathBuf::from(text))
    }
}

fn _unused(_: Snapshot, _: &FxStatus) {}
