//! Bespoke Raycast / CleanShot-inspired Design System for Region to Share v3.
//!
//! Provides cohesive Midnight Slate (Dark) and Crisp Alabaster (Light) palettes,
//! dynamic system font loading (Inter + Monospace), and polished egui visuals.

use egui::{Color32, FontData, FontDefinitions, FontFamily, Rounding, Shadow, Stroke, Visuals};
use std::path::Path;

/// Semantic color tokens used throughout the UI.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub bg_canvas: Color32,
    pub bg_surface: Color32,
    pub bg_elevated: Color32,
    pub bg_hover: Color32,
    pub bg_active: Color32,
    pub border_subtle: Color32,
    pub border_strong: Color32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_muted: Color32,
    pub accent_share: Color32,
    pub accent_record: Color32,
    pub accent_live: Color32,
    pub accent_warn: Color32,
    pub accent_purple: Color32,
    pub scrim: Color32,
}

impl Palette {
    pub const fn dark() -> Self {
        Self {
            dark: true,
            bg_canvas: Color32::from_rgb(15, 17, 23),       // #0F1117
            bg_surface: Color32::from_rgb(23, 26, 35),      // #171A23
            bg_elevated: Color32::from_rgb(31, 35, 48),     // #1F2330
            bg_hover: Color32::from_rgb(39, 44, 60),        // #272C3C
            bg_active: Color32::from_rgb(47, 54, 74),       // #2F364A
            border_subtle: Color32::from_rgb(40, 45, 62),   // #282D3E
            border_strong: Color32::from_rgb(64, 73, 98),   // #404962
            text_primary: Color32::from_rgb(241, 245, 249), // #F1F5F9
            text_secondary: Color32::from_rgb(165, 177, 198),
            text_muted: Color32::from_rgb(108, 120, 145),
            accent_share: Color32::from_rgb(91, 140, 255),  // #5B8CFF
            accent_record: Color32::from_rgb(255, 77, 94),  // #FF4D5E
            accent_live: Color32::from_rgb(45, 212, 191),   // #2DD4BF
            accent_warn: Color32::from_rgb(245, 158, 11),   // #F59E0B
            accent_purple: Color32::from_rgb(167, 139, 250),
            scrim: Color32::from_rgba_premultiplied(9, 11, 16, 175),
        }
    }

    pub const fn light() -> Self {
        Self {
            dark: false,
            bg_canvas: Color32::from_rgb(246, 247, 251),    // #F6F7FB
            bg_surface: Color32::from_rgb(255, 255, 255),   // #FFFFFF
            bg_elevated: Color32::from_rgb(241, 243, 249),  // #F1F3F9
            bg_hover: Color32::from_rgb(232, 236, 246),     // #E8ECF6
            bg_active: Color32::from_rgb(222, 227, 240),
            border_subtle: Color32::from_rgb(224, 228, 238),
            border_strong: Color32::from_rgb(193, 200, 218),
            text_primary: Color32::from_rgb(18, 22, 34),
            text_secondary: Color32::from_rgb(74, 85, 108),
            text_muted: Color32::from_rgb(122, 134, 158),
            accent_share: Color32::from_rgb(59, 112, 246),
            accent_record: Color32::from_rgb(228, 52, 72),
            accent_live: Color32::from_rgb(16, 168, 148),
            accent_warn: Color32::from_rgb(217, 119, 6),
            accent_purple: Color32::from_rgb(124, 92, 236),
            scrim: Color32::from_rgba_premultiplied(15, 18, 28, 155),
        }
    }

    pub const fn for_dark_mode(dark: bool) -> Self {
        if dark {
            Self::dark()
        } else {
            Self::light()
        }
    }

    pub fn tint(color: Color32, alpha: u8) -> Color32 {
        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
    }
}

/// Load crisp system fonts (`Inter` + monospace) when available on the host OS.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    let proportional_candidates = [
        "/usr/share/fonts/rsms-inter-fonts/Inter-Regular.ttf",
        "/usr/share/fonts/inter/Inter-Regular.ttf",
        "/usr/share/fonts/truetype/inter/Inter-Regular.ttf",
        "/usr/share/fonts/adwaita-sans-fonts/AdwaitaSans-Regular.ttf",
        "/usr/share/fonts/google-noto/NotoSans-Regular.ttf",
        "/System/Library/Fonts/SFNS.ttf",
        "C:\\Windows\\Fonts\\segoeui.ttf",
    ];

    for path in proportional_candidates {
        if Path::new(path).exists() {
            if let Ok(bytes) = std::fs::read(path) {
                fonts
                    .font_data
                    .insert("ui_sans".to_owned(), FontData::from_owned(bytes).into());
                fonts
                    .families
                    .entry(FontFamily::Proportional)
                    .or_default()
                    .insert(0, "ui_sans".to_owned());
                break;
            }
        }
    }

    let mono_candidates = [
        "/usr/share/fonts/jetbrains-mono-fonts/JetBrainsMono-Regular.ttf",
        "/usr/share/fonts/adwaita-mono-fonts/AdwaitaMono-Regular.ttf",
        "/usr/share/fonts/google-noto/NotoSansMono-Regular.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        "/System/Library/Fonts/SFNSMono.ttf",
        "C:\\Windows\\Fonts\\consola.ttf",
    ];

    for path in mono_candidates {
        if Path::new(path).exists() {
            if let Ok(bytes) = std::fs::read(path) {
                fonts
                    .font_data
                    .insert("ui_mono".to_owned(), FontData::from_owned(bytes).into());
                fonts
                    .families
                    .entry(FontFamily::Monospace)
                    .or_default()
                    .insert(0, "ui_mono".to_owned());
                break;
            }
        }
    }

    ctx.set_fonts(fonts);
}

/// Apply the v3 Design System theme to `egui::Context`.
pub fn apply_theme(ctx: &egui::Context, palette: Palette) {
    let mut visuals = if palette.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };

    visuals.panel_fill = palette.bg_canvas;
    visuals.window_fill = palette.bg_surface;
    visuals.faint_bg_color = palette.bg_elevated;
    visuals.extreme_bg_color = palette.bg_canvas;
    visuals.code_bg_color = palette.bg_elevated;
    visuals.override_text_color = Some(palette.text_primary);

    visuals.window_rounding = Rounding::same(14.0);
    visuals.window_stroke = Stroke::new(1.0_f32, palette.border_subtle);
    visuals.window_shadow = Shadow {
        offset: egui::vec2(0.0, 12.0),
        blur: 28.0,
        spread: 0.0,
        color: Color32::from_black_alpha(if palette.dark { 110 } else { 36 }),
    };

    visuals.popup_shadow = visuals.window_shadow;
    visuals.menu_rounding = Rounding::same(10.0);

    visuals.selection.bg_fill = Palette::tint(palette.accent_share, 75);
    visuals.selection.stroke = Stroke::new(1.5_f32, palette.accent_share);

    visuals.widgets.noninteractive.bg_fill = palette.bg_surface;
    visuals.widgets.noninteractive.weak_bg_fill = palette.bg_elevated;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, palette.border_subtle);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, palette.text_secondary);
    visuals.widgets.noninteractive.rounding = Rounding::same(8.0);

    visuals.widgets.inactive.bg_fill = palette.bg_elevated;
    visuals.widgets.inactive.weak_bg_fill = palette.bg_elevated;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, palette.border_subtle);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.2_f32, palette.text_primary);
    visuals.widgets.inactive.rounding = Rounding::same(8.0);

    visuals.widgets.hovered.bg_fill = palette.bg_hover;
    visuals.widgets.hovered.weak_bg_fill = palette.bg_hover;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, palette.border_strong);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.4_f32, palette.text_primary);
    visuals.widgets.hovered.rounding = Rounding::same(8.0);

    visuals.widgets.active.bg_fill = palette.bg_active;
    visuals.widgets.active.weak_bg_fill = palette.bg_active;
    visuals.widgets.active.bg_stroke = Stroke::new(1.2_f32, palette.accent_share);
    visuals.widgets.active.fg_stroke = Stroke::new(1.5_f32, palette.text_primary);
    visuals.widgets.active.rounding = Rounding::same(8.0);

    let mut style = (*ctx.style()).clone();
    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.spacing.window_margin = egui::Margin::same(16.0);
    style.spacing.interact_size.y = 30.0;
    style.spacing.slider_width = 160.0;
    ctx.set_style(style);
}
