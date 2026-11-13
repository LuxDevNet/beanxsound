#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod audio;
mod fxsound;
mod presets;
mod theme;
mod winshell;

use std::ffi::c_void;

use eframe::egui;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};

fn main() -> eframe::Result {
    std::panic::set_hook(Box::new(|info| {
        let path = std::env::temp_dir().join("beanxsound-panic.log");
        let text = format!("{}\n{:?}\n", info, std::backtrace::Backtrace::force_capture());
        let _ = std::fs::write(path, text);
    }));

    if std::env::args().any(|arg| arg == "--diagnose") {
        run_diagnostics();
        return Ok(());
    }

    if std::env::args().any(|arg| arg == "--pin") {
        let _ = winshell::pin_to_taskbar();
        return Ok(());
    }

    if std::env::args().any(|arg| arg == "--unpin") {
        let _ = winshell::unpin_from_taskbar();
        return Ok(());
    }

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("beanxsound")
        .with_app_id("beanxsound")
        .with_inner_size([1240.0, 820.0])
        .with_min_inner_size([1000.0, 660.0]);

    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        renderer: if cfg!(target_arch = "aarch64") {
            eframe::Renderer::Wgpu
        } else {
            eframe::Renderer::Glow
        },
        ..Default::default()
    };

    eframe::run_native(
        "beanxsound",
        options,
        Box::new(|cc| {
            style_window(cc);
            cc.egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::SetTheme(egui::SystemTheme::Dark));
            Ok(Box::new(app::BeanXSound::new(cc)))
        }),
    )
}

fn window_icon() -> Option<egui::IconData> {
    let bytes = include_bytes!("../assets/icon.png");
    let image = image::load_from_memory(bytes).ok()?.into_rgba8();
    let (width, height) = image.dimensions();
    Some(egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    })
}

/// Dark title bar, rounded Win11 corners, and a caption colour that matches
/// the app instead of the default Windows grey.
fn style_window(cc: &eframe::CreationContext<'_>) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = cc.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        return;
    };
    let hwnd = HWND(win32.hwnd.get() as *mut c_void);
    unsafe {
        let dark: i32 = 1;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const _ as *const c_void,
            std::mem::size_of::<i32>() as u32,
        );
        let corner = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const _ as *const c_void,
            std::mem::size_of::<i32>() as u32,
        );
        // COLORREF is 0x00BBGGRR. These match theme::PANEL and theme::TEXT.
        let caption: u32 = 0x001F120C;
        let text: u32 = 0x00F7EDE7;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_CAPTION_COLOR,
            &caption as *const _ as *const c_void,
            std::mem::size_of::<u32>() as u32,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_TEXT_COLOR,
            &text as *const _ as *const c_void,
            std::mem::size_of::<u32>() as u32,
        );
    }
}

/// Headless smoke test: opens WASAPI, lists endpoints and sessions, then asks
/// FxSound for its status. Results go to `%TEMP%\beanxsound-diagnostics.txt`
/// because the release binary has no console.
fn run_diagnostics() {
    use std::fmt::Write as _;
    use std::time::{Duration, Instant};

    let mut out = String::new();
    let _ = writeln!(out, "beanxsound diagnostics");
    let _ = writeln!(out, "arch: {}", std::env::consts::ARCH);
    let _ = writeln!(
        out,
        "exe: {}",
        std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "<unknown>".to_string())
    );
    let _ = writeln!(out, "temp: {}", std::env::temp_dir().display());
    out.push('\n');

    let _ = writeln!(
        out,
        "COM: {}",
        audio::init_com().unwrap_or_else(|| "initialised".to_string())
    );

    match audio::AudioEngine::new() {
        Ok(engine) => {
            let snapshot = engine.refresh(true);
            let _ = writeln!(out, "\nDEVICES ({})", snapshot.devices.len());
            for d in &snapshot.devices {
                let roles = [
                    (d.default_console, "console"),
                    (d.default_multimedia, "multimedia"),
                    (d.default_communications, "communications"),
                ]
                .iter()
                .filter(|(on, _)| *on)
                .map(|(_, name)| *name)
                .collect::<Vec<_>>()
                .join(", ");
                let _ = writeln!(
                    out,
                    "  [{}] {:<12} {} | vol {:.0}%{} | peak {:.3} | {} | {}{}",
                    if d.is_capture { "in " } else { "out" },
                    d.state_label(),
                    d.name,
                    d.volume * 100.0,
                    if d.muted { " (muted)" } else { "" },
                    d.peak,
                    d.format_label(),
                    d.id,
                    if roles.is_empty() {
                        String::new()
                    } else {
                        format!(" | default: {roles}")
                    }
                );
            }

            let _ = writeln!(out, "\nSESSIONS ({})", snapshot.sessions.len());
            for s in &snapshot.sessions {
                let _ = writeln!(
                    out,
                    "  {:<10} {:<24} pid {:<6} vol {:.0}%{} | {}",
                    s.state_label(),
                    s.app_name(),
                    s.pid,
                    s.volume * 100.0,
                    if s.muted { " (muted)" } else { "" },
                    s.device_name
                );
            }
        }
        Err(err) => {
            let _ = writeln!(out, "\nDEVICES: error: {err}");
        }
    }

    let fx = fxsound::FxClient::new();
    let _ = writeln!(out, "\nFXSOUND");
    let _ = writeln!(
        out,
        "  exe: {}",
        fx.exe
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "not found".to_string())
    );
    let _ = writeln!(out, "  status file: {}", fx.status_path.display());
    let _ = writeln!(out, "  running: {}", fxsound::is_running());

    fx.refresh();
    let deadline = Instant::now() + Duration::from_secs(4);
    let mut reported = false;
    while Instant::now() < deadline && !reported {
        for event in fx.poll() {
            reported = true;
            match event {
                fxsound::FxEvent::Status(status) => {
                    let _ = writeln!(out, "  version: {}", status.version);
                    let _ = writeln!(out, "  power: {}", status.power);
                    let _ = writeln!(out, "  preset: {}", status.selected_preset);
                    let _ = writeln!(out, "  output: {}", status.selected_output);
                    let _ = writeln!(
                        out,
                        "  bands: {} | master {:.1} | balance {:.1} | leveling {:.1} | Q {:.1}",
                        status.equalizer.num_bands,
                        status.equalizer.master_gain,
                        status.equalizer.balance,
                        status.equalizer.volume_leveling,
                        status.equalizer.filter_q
                    );
                    let _ = writeln!(
                        out,
                        "  effects: clarity {:.0} | ambience {:.0} | surround {:.0} | boost {:.0} | bass {:.0}",
                        status.effects.clarity,
                        status.effects.ambience,
                        status.effects.surround,
                        status.effects.dynamicboost,
                        status.effects.bass
                    );
                    let _ = writeln!(
                        out,
                        "  presets: {} built-in, {} user | outputs: {}",
                        status.presets.built_in.len(),
                        status.presets.user_defined.len(),
                        status.output_devices.len()
                    );
                }
                fxsound::FxEvent::NotRunning => {
                    let _ = writeln!(out, "  status: FxSound is not running");
                }
                fxsound::FxEvent::Error(err) => {
                    let _ = writeln!(out, "  status error: {err}");
                }
            }
        }
        if !reported {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    if !reported {
        let _ = writeln!(out, "  status: no response within 4s");
    }

    let path = std::env::temp_dir().join("beanxsound-diagnostics.txt");
    match std::fs::write(&path, &out) {
        Ok(()) => {
            println!("{out}");
            println!("wrote {}", path.display());
        }
        Err(err) => eprintln!("could not write {}: {err}", path.display()),
    }
}
