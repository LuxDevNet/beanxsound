use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Margin, RichText, Stroke, TextStyle,
};

use serde::{Deserialize, Serialize};

/// Every colour the interface needs. Themes are just different palettes.
#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub bg2: Color32,
    pub panel: Color32,
    pub card: Color32,
    pub card_hi: Color32,
    pub input: Color32,
    pub border: Color32,
    pub accent: Color32,
    pub accent_dark: Color32,
    pub accent2: Color32,
    pub chrome: Color32,
    pub green: Color32,
    pub red: Color32,
    pub amber: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub faint: Color32,
}

pub struct Theme {
    pub name: &'static str,
    pub note: &'static str,
    pub palette: Palette,
}

const fn c(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// Nine takes on the same idea: black rooms, liquid chrome, pink light.
pub static THEMES: [Theme; 9] = [
    Theme {
        name: "Y2K Chrome",
        note: "Black glass, liquid chrome, hot pink.",
        palette: Palette {
            bg: c(5, 5, 7),
            bg2: c(10, 10, 14),
            panel: c(11, 11, 15),
            card: c(19, 19, 24),
            card_hi: c(27, 27, 35),
            input: c(8, 8, 11),
            border: c(42, 42, 52),
            accent: c(255, 79, 163),
            accent_dark: c(122, 30, 78),
            accent2: c(201, 214, 232),
            chrome: c(242, 246, 252),
            green: c(82, 229, 168),
            red: c(255, 92, 122),
            amber: c(255, 196, 107),
            text: c(242, 243, 247),
            muted: c(162, 166, 178),
            faint: c(107, 111, 124),
        },
    },
    Theme {
        name: "Liquid Metal",
        note: "Mercury finish, blush highlight.",
        palette: Palette {
            bg: c(4, 5, 8),
            bg2: c(8, 10, 13),
            panel: c(10, 12, 16),
            card: c(18, 21, 26),
            card_hi: c(26, 30, 37),
            input: c(6, 7, 9),
            border: c(44, 49, 58),
            accent: c(231, 237, 247),
            accent_dark: c(56, 64, 78),
            accent2: c(255, 95, 168),
            chrome: c(255, 255, 255),
            green: c(120, 235, 190),
            red: c(255, 104, 128),
            amber: c(255, 205, 130),
            text: c(245, 247, 250),
            muted: c(166, 173, 184),
            faint: c(109, 116, 128),
        },
    },
    Theme {
        name: "Cyber Pink",
        note: "Magenta neon on ink black.",
        palette: Palette {
            bg: c(6, 4, 6),
            bg2: c(11, 7, 11),
            panel: c(13, 8, 13),
            card: c(21, 14, 21),
            card_hi: c(30, 21, 30),
            input: c(9, 5, 9),
            border: c(51, 32, 47),
            accent: c(255, 46, 136),
            accent_dark: c(140, 18, 71),
            accent2: c(255, 158, 201),
            chrome: c(246, 239, 244),
            green: c(90, 226, 168),
            red: c(255, 84, 108),
            amber: c(255, 190, 112),
            text: c(250, 242, 247),
            muted: c(182, 163, 174),
            faint: c(124, 104, 116),
        },
    },
    Theme {
        name: "Bubblegum",
        note: "Powder pink over polished pewter.",
        palette: Palette {
            bg: c(8, 6, 9),
            bg2: c(13, 10, 14),
            panel: c(16, 12, 18),
            card: c(23, 18, 26),
            card_hi: c(32, 25, 36),
            input: c(10, 7, 11),
            border: c(53, 42, 56),
            accent: c(255, 122, 184),
            accent_dark: c(94, 37, 69),
            accent2: c(230, 217, 226),
            chrome: c(255, 244, 250),
            green: c(120, 228, 186),
            red: c(255, 104, 124),
            amber: c(255, 202, 138),
            text: c(247, 240, 245),
            muted: c(180, 164, 176),
            faint: c(124, 107, 120),
        },
    },
    Theme {
        name: "Shanghai Neon",
        note: "Midnight violet, cyan and candy pink.",
        palette: Palette {
            bg: c(7, 5, 19),
            bg2: c(12, 8, 32),
            panel: c(14, 10, 36),
            card: c(23, 16, 49),
            card_hi: c(33, 22, 65),
            input: c(8, 6, 20),
            border: c(52, 32, 86),
            accent: c(255, 94, 210),
            accent_dark: c(91, 31, 115),
            accent2: c(86, 224, 255),
            chrome: c(234, 247, 255),
            green: c(96, 236, 188),
            red: c(255, 96, 128),
            amber: c(255, 205, 126),
            text: c(243, 238, 255),
            muted: c(169, 155, 200),
            faint: c(114, 99, 153),
        },
    },
    Theme {
        name: "Gunmetal",
        note: "Machined steel with a rose warning light.",
        palette: Palette {
            bg: c(6, 7, 8),
            bg2: c(10, 11, 13),
            panel: c(12, 14, 16),
            card: c(20, 23, 26),
            card_hi: c(29, 33, 37),
            input: c(8, 9, 10),
            border: c(46, 51, 56),
            accent: c(242, 245, 249),
            accent_dark: c(44, 50, 58),
            accent2: c(255, 92, 122),
            chrome: c(255, 255, 255),
            green: c(133, 232, 190),
            red: c(255, 104, 124),
            amber: c(255, 208, 138),
            text: c(241, 243, 245),
            muted: c(160, 167, 174),
            faint: c(106, 113, 120),
        },
    },
    Theme {
        name: "Chrome Rouge",
        note: "Black lacquer, cherry red, chrome trim.",
        palette: Palette {
            bg: c(8, 4, 5),
            bg2: c(16, 7, 9),
            panel: c(18, 8, 10),
            card: c(26, 13, 16),
            card_hi: c(36, 19, 24),
            input: c(10, 5, 6),
            border: c(58, 32, 39),
            accent: c(255, 59, 92),
            accent_dark: c(110, 22, 38),
            accent2: c(255, 179, 193),
            chrome: c(255, 241, 243),
            green: c(112, 228, 176),
            red: c(255, 74, 92),
            amber: c(255, 198, 120),
            text: c(250, 240, 242),
            muted: c(185, 160, 165),
            faint: c(130, 102, 108),
        },
    },
    Theme {
        name: "Frosted Pink",
        note: "Soft pearl, smoke, and a blush shimmer.",
        palette: Palette {
            bg: c(10, 9, 11),
            bg2: c(16, 14, 18),
            panel: c(18, 16, 20),
            card: c(25, 22, 28),
            card_hi: c(35, 31, 40),
            input: c(11, 10, 13),
            border: c(58, 51, 64),
            accent: c(255, 182, 213),
            accent_dark: c(94, 58, 76),
            accent2: c(239, 230, 238),
            chrome: c(255, 255, 255),
            green: c(128, 232, 190),
            red: c(255, 112, 134),
            amber: c(255, 210, 148),
            text: c(248, 244, 247),
            muted: c(181, 169, 179),
            faint: c(126, 112, 121),
        },
    },
    Theme {
        name: "Hologram",
        note: "Oil-slick violet with an iridescent edge.",
        palette: Palette {
            bg: c(7, 4, 11),
            bg2: c(12, 7, 20),
            panel: c(15, 9, 24),
            card: c(23, 15, 34),
            card_hi: c(33, 22, 47),
            input: c(9, 5, 16),
            border: c(57, 34, 79),
            accent: c(232, 121, 255),
            accent_dark: c(92, 27, 122),
            accent2: c(255, 127, 178),
            chrome: c(251, 240, 255),
            green: c(104, 236, 190),
            red: c(255, 98, 130),
            amber: c(255, 202, 130),
            text: c(247, 238, 255),
            muted: c(177, 155, 198),
            faint: c(121, 99, 144),
        },
    },
];

static ACTIVE: AtomicUsize = AtomicUsize::new(0);

pub fn themes() -> &'static [Theme] {
    &THEMES
}

pub fn theme_index() -> usize {
    ACTIVE.load(Ordering::Relaxed).min(THEMES.len() - 1)
}

pub fn set_theme_index(index: usize) {
    ACTIVE.store(index.min(THEMES.len() - 1), Ordering::Relaxed);
}

pub fn theme_name() -> &'static str {
    THEMES[theme_index()].name
}

#[inline]
pub fn palette() -> Palette {
    THEMES[theme_index()].palette
}

#[inline]
pub fn bg() -> Color32 {
    palette().bg
}
#[inline]
#[allow(dead_code)]
pub fn bg2() -> Color32 {
    palette().bg2
}
#[inline]
pub fn panel() -> Color32 {
    palette().panel
}
#[inline]
pub fn card_bg() -> Color32 {
    palette().card
}
#[inline]
pub fn card_hi() -> Color32 {
    palette().card_hi
}
#[inline]
pub fn input() -> Color32 {
    palette().input
}
#[inline]
pub fn border() -> Color32 {
    palette().border
}
#[inline]
pub fn accent() -> Color32 {
    palette().accent
}
#[inline]
pub fn accent_dark() -> Color32 {
    palette().accent_dark
}
#[inline]
pub fn accent2() -> Color32 {
    palette().accent2
}
#[inline]
pub fn chrome() -> Color32 {
    palette().chrome
}
#[inline]
pub fn green() -> Color32 {
    palette().green
}
#[inline]
pub fn red() -> Color32 {
    palette().red
}
#[inline]
pub fn amber() -> Color32 {
    palette().amber
}
#[inline]
pub fn text() -> Color32 {
    palette().text
}
#[inline]
pub fn muted() -> Color32 {
    palette().muted
}
#[inline]
pub fn faint() -> Color32 {
    palette().faint
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let candidates = [
        r"C:\Windows\Fonts\segoeui.ttf",
        r"C:\Windows\Fonts\SegoeUI.ttf",
        r"C:\Windows\Fonts\tahoma.ttf",
    ];
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(
                "ui".to_owned(),
                std::sync::Arc::new(egui::FontData::from_owned(bytes)),
            );
            fonts
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .insert(0, "ui".to_owned());
            fonts
                .families
                .entry(FontFamily::Monospace)
                .or_default()
                .push("ui".to_owned());
            break;
        }
    }
    ctx.set_fonts(fonts);
}

pub fn apply(ctx: &egui::Context) {
    let p = palette();
    let mut v = egui::Visuals::dark();
    v.panel_fill = p.bg;
    v.window_fill = p.panel;
    v.extreme_bg_color = p.input;
    v.faint_bg_color = p.card;
    v.override_text_color = Some(p.text);
    v.hyperlink_color = p.accent;
    v.slider_trailing_fill = true;
    v.selection.bg_fill = p.accent_dark;
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(14);
    v.menu_corner_radius = CornerRadius::same(10);

    v.widgets.noninteractive.bg_fill = p.panel;
    v.widgets.noninteractive.weak_bg_fill = p.panel;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.muted);
    v.widgets.noninteractive.corner_radius = CornerRadius::same(10);

    v.widgets.inactive.bg_fill = p.card;
    v.widgets.inactive.weak_bg_fill = p.card;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.inactive.corner_radius = CornerRadius::same(9);

    v.widgets.hovered.bg_fill = p.card_hi;
    v.widgets.hovered.weak_bg_fill = p.card_hi;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.accent);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.hovered.corner_radius = CornerRadius::same(9);

    v.widgets.active.bg_fill = p.accent_dark;
    v.widgets.active.weak_bg_fill = p.accent_dark;
    v.widgets.active.bg_stroke = Stroke::new(1.0, p.accent);
    v.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    v.widgets.active.corner_radius = CornerRadius::same(9);

    v.widgets.open.bg_fill = p.card_hi;
    v.widgets.open.weak_bg_fill = p.card_hi;
    v.widgets.open.bg_stroke = Stroke::new(1.0, p.accent);

    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
    style.visuals = v;
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.spacing.slider_width = 160.0;
    style.spacing.interact_size.y = 30.0;
    style.spacing.scroll.bar_width = 8.0;
    style.text_styles = [
        (TextStyle::Small, FontId::new(11.0, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Heading, FontId::new(22.0, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(12.0, FontFamily::Monospace)),
    ]
    .into();
    ctx.set_style_of(egui::Theme::Dark, style);
}

pub fn card() -> egui::Frame {
    let p = palette();
    egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::symmetric(16, 14))
}

pub fn inner_card() -> egui::Frame {
    let p = palette();
    egui::Frame::new()
        .fill(p.bg2)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(12, 10))
}

pub fn chip(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.14))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.4)))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(7, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(10.0).color(color).strong());
        });
}

#[allow(dead_code)]
pub fn section(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(18.0).strong().color(text()));
        if !subtitle.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new(subtitle).size(12.0).color(faint()));
        }
    });
    ui.add_space(2.0);
}

/// Paint a two-colour gradient into an arbitrary rect. Used for the chrome
/// sheen, top-bar hairline and background wash.
pub fn gradient_rect(
    painter: &egui::Painter,
    rect: egui::Rect,
    a: Color32,
    b: Color32,
    horizontal: bool,
) {
    use egui::epaint::{Mesh, Vertex, WHITE_UV};
    let mut mesh = Mesh::default();
    let colors = if horizontal { [a, b, a, b] } else { [a, a, b, b] };
    mesh.vertices = vec![
        Vertex {
            pos: rect.left_top(),
            uv: WHITE_UV,
            color: colors[0],
        },
        Vertex {
            pos: rect.right_top(),
            uv: WHITE_UV,
            color: colors[1],
        },
        Vertex {
            pos: rect.left_bottom(),
            uv: WHITE_UV,
            color: colors[2],
        },
        Vertex {
            pos: rect.right_bottom(),
            uv: WHITE_UV,
            color: colors[3],
        },
    ];
    mesh.indices = vec![0, 1, 2, 1, 3, 2];
    painter.add(egui::Shape::mesh(mesh));
}

pub fn peak_color(v: f32) -> Color32 {
    if v > 0.9 {
        red()
    } else if v > 0.65 {
        amber()
    } else {
        green()
    }
}

pub fn tab_button(ui: &mut egui::Ui, selected: bool, label: &str) -> bool {
    let p = palette();
    let text = RichText::new(label)
        .size(13.5)
        .strong()
        .color(if selected { Color32::WHITE } else { p.muted });
    let button = egui::Button::new(text)
        .fill(if selected { p.accent_dark } else { Color32::TRANSPARENT })
        .stroke(Stroke::new(
            1.0,
            if selected { p.accent } else { Color32::TRANSPARENT },
        ))
        .corner_radius(CornerRadius::same(8));
    ui.add(button).clicked()
}

pub fn icon_badge(ui: &mut egui::Ui, label: &str, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(36.0, 36.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(10), color.gamma_multiply(0.16));
    p.rect_stroke(
        rect,
        CornerRadius::same(10),
        Stroke::new(1.0, color.gamma_multiply(0.45)),
        egui::StrokeKind::Inside,
    );
    p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        FontId::new(10.0, FontFamily::Proportional),
        color,
    );
}

// ---------------------------------------------------------------------------
// Saved appearance choice

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct UiSettings {
    theme: Option<String>,
}

fn settings_path() -> PathBuf {
    std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(r"beanxsound\ui.json")
}

/// Load the saved theme index, falling back to Chrome Noir.
pub fn load_saved() -> usize {
    let Ok(text) = std::fs::read_to_string(settings_path()) else {
        return 0;
    };
    let Ok(settings) = serde_json::from_str::<UiSettings>(&text) else {
        return 0;
    };
    settings
        .theme
        .and_then(|name| {
            THEMES
                .iter()
                .position(|theme| theme.name.eq_ignore_ascii_case(&name))
        })
        .unwrap_or(0)
}

pub fn save_theme(index: usize) {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let settings = UiSettings {
        theme: Some(THEMES[index.min(THEMES.len() - 1)].name.to_string()),
    };
    if let Ok(text) = serde_json::to_string_pretty(&settings) {
        let _ = std::fs::write(path, text);
    }
}
