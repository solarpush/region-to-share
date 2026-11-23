//! Reusable polished v3 UI components (Action Cards, Glass Pills, Status Badges, Preset Cards).

use crate::icons::{paint_icon, Icon};
use crate::theme::Palette;
use egui::{pos2, vec2, Align2, Color32, FontId, Rect, Response, Rounding, Sense, Stroke, Ui};

/// Elevated primary action card with tinted icon badge, title, subtitle, and keyboard shortcut cap.
#[allow(clippy::too_many_arguments)]
pub fn action_card(
    ui: &mut Ui,
    palette: Palette,
    icon: Icon,
    title: &str,
    subtitle: &str,
    shortcut: &str,
    accent: Color32,
    width: f32,
) -> Response {
    let desired = vec2(width, 98.0);
    let (rect, resp) = ui.allocate_exact_size(desired, Sense::click());
    let hovered = resp.hovered();

    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    let bg = if resp.is_pointer_button_down_on() {
        palette.bg_active
    } else if hovered {
        palette.bg_hover
    } else {
        palette.bg_surface
    };

    let border_col = if hovered {
        Palette::tint(accent, 190)
    } else {
        palette.border_subtle
    };

    let p = ui.painter();
    p.rect_filled(rect, Rounding::same(12.0), bg);
    p.rect_stroke(rect, Rounding::same(12.0), Stroke::new(1.2_f32, border_col));

    // Icon badge (top-left)
    let badge_rect = Rect::from_min_size(rect.min + vec2(12.0, 12.0), vec2(32.0, 32.0));
    p.rect_filled(badge_rect, Rounding::same(9.0), Palette::tint(accent, 36));
    p.rect_stroke(
        badge_rect,
        Rounding::same(9.0),
        Stroke::new(1.0_f32, Palette::tint(accent, 80)),
    );
    paint_icon(p, badge_rect, icon, accent);

    // Shortcut keycap (top-right)
    if !shortcut.is_empty() {
        let cap = Rect::from_min_size(pos2(rect.right() - 32.0, rect.top() + 12.0), vec2(20.0, 20.0));
        p.rect_filled(cap, Rounding::same(5.0), palette.bg_elevated);
        p.rect_stroke(cap, Rounding::same(5.0), Stroke::new(1.0_f32, palette.border_subtle));
        p.text(
            cap.center(),
            Align2::CENTER_CENTER,
            shortcut,
            FontId::monospace(10.5),
            palette.text_muted,
        );
    }

    // Title
    p.text(
        pos2(rect.left() + 12.0, rect.top() + 54.0),
        Align2::LEFT_TOP,
        title,
        FontId::proportional(13.5),
        palette.text_primary,
    );

    // Subtitle
    p.text(
        pos2(rect.left() + 12.0, rect.top() + 73.0),
        Align2::LEFT_TOP,
        subtitle,
        FontId::proportional(11.0),
        palette.text_muted,
    );

    resp
}

/// Sleek rounded pill button with icon and optional label.
pub fn pill_button(
    ui: &mut Ui,
    palette: Palette,
    icon: Icon,
    label: &str,
    emphasized: bool,
    accent: Color32,
) -> Response {
    let font = FontId::proportional(12.5);
    let galley = ui.fonts(|f| f.layout_no_wrap(label.to_string(), font.clone(), Color32::WHITE));
    let has_text = !label.is_empty();
    let w = if has_text {
        12.0 + 16.0 + 6.0 + galley.size().x + 12.0
    } else {
        32.0
    };
    let h = 32.0;

    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
    let hovered = resp.hovered();
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    let (bg, fg, border) = if emphasized {
        let bg = if hovered {
            accent
        } else {
            Palette::tint(accent, 220)
        };
        (bg, Color32::WHITE, accent)
    } else {
        let bg = if hovered {
            palette.bg_hover
        } else {
            palette.bg_elevated
        };
        let border = if hovered {
            palette.border_strong
        } else {
            palette.border_subtle
        };
        (bg, palette.text_primary, border)
    };

    let p = ui.painter();
    p.rect_filled(rect, Rounding::same(9.0), bg);
    p.rect_stroke(rect, Rounding::same(9.0), Stroke::new(1.0_f32, border));

    let icon_rect = if has_text {
        Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), vec2(16.0, 16.0))
    } else {
        Rect::from_center_size(rect.center(), vec2(16.0, 16.0))
    };
    paint_icon(p, icon_rect, icon, fg);

    if has_text {
        p.text(
            pos2(icon_rect.right() + 6.0, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            font,
            fg,
        );
    }

    resp
}

/// Compact rounded status pill with optional pulsing status dot.
pub fn status_badge(ui: &mut Ui, palette: Palette, text: &str, dot: Color32, pulsing: bool) {
    let font = FontId::monospace(11.0);
    let galley = ui.fonts(|f| f.layout_no_wrap(text.to_string(), font.clone(), dot));
    let w = 22.0 + galley.size().x + 10.0;
    let h = 24.0;
    let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());

    let p = ui.painter();
    p.rect_filled(rect, Rounding::same(12.0), Palette::tint(dot, 28));
    p.rect_stroke(
        rect,
        Rounding::same(12.0),
        Stroke::new(1.0_f32, Palette::tint(dot, 85)),
    );

    let dot_center = pos2(rect.left() + 12.0, rect.center().y);
    if pulsing {
        let t = (ui.input(|i| i.time) * 3.2).sin() as f32 * 0.5 + 0.5;
        p.circle_filled(dot_center, 4.0 + t * 2.5, Palette::tint(dot, (70.0 * (1.0 - t)) as u8));
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(60));
    }
    p.circle_filled(dot_center, 3.6, dot);

    p.text(
        pos2(rect.left() + 21.0, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        font,
        if palette.dark {
            palette.text_primary
        } else {
            dot
        },
    );
}

/// Format byte count cleanly (e.g. `14.2 MB`).
pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

/// Format elapsed seconds as `MM:SS` or `HH:MM:SS`.
pub fn format_duration(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}
