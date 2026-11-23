//! Sleek, uncluttered Floating Mini-HUD Pill & Auto-Hide Hover Bar.
//!
//! - Keeps the shared mirror window **100% pure pixels** with zero buttons leaking
//!   into Google Meet, Discord, Teams, Zoom, or OBS.
//! - Also powers **background / mirror-hidden mode** (live recording or virtual
//!   camera without displaying any mirror window at all!).

use crate::icons::{paint_icon, Icon};
use crate::theme::Palette;
use crate::widgets::{format_bytes, format_duration, pill_button, status_badge};
use egui::{pos2, vec2, Align2, Color32, FontId, Rect, Response, Rounding, Sense, Stroke, ViewportBuilder, ViewportId};
use region_core::HudMode;
use region_platform::DisplayServer;
use rust_i18n::t;

/// Actions returned from the HUD controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudAction {
    ToggleRecord,
    TogglePause,
    Snapshot,
    Reselect,
    ToggleKeepBelow,
    ToggleMirrorVisible,
    ToggleTools,
    ToggleVirtualCam,
    ToggleStats,
    Stop,
}

/// Live state & metrics rendered in the HUD pill.
#[derive(Debug, Clone)]
pub struct HudState {
    pub is_recording: bool,
    pub is_paused: bool,
    pub mirror_visible: bool,
    pub mirror_keep_below: bool,
    pub virtual_cam_active: bool,
    pub show_tools: bool,
    pub show_stats: bool,
    pub fps: f64,
    pub cpu_percent: f64,
    pub memory_mb: f64,
    pub rec_elapsed_secs: u64,
    pub rec_bytes: u64,
    pub encoder_label: String,
}

/// Compute exact HUD viewport dimensions so controls NEVER clip in any state.
pub fn hud_dimensions(state: &HudState) -> egui::Vec2 {
    let base_w: f32 = if state.is_recording { 536.0 } else { 436.0 };
    let tools_min_w: f32 = if state.show_tools { 210.0 } else { 0.0 };
    let width = base_w.max(tools_min_w);

    let mut height = 50.0;
    if state.show_tools {
        height += 36.0;
    }
    if state.show_stats {
        height += 24.0;
    }
    vec2(width, height)
}

/// Render the HUD directly inside the current (root) viewport when running in
/// **mirror-hidden / background capture** mode.
pub fn render_embedded_hud(
    ctx: &egui::Context,
    palette: Palette,
    state: &HudState,
) -> Option<HudAction> {
    let mut action = None;
    egui::CentralPanel::default()
        .frame(
            egui::Frame::none()
                .fill(palette.bg_surface)
                .stroke(Stroke::new(1.0_f32, palette.border_strong))
                .rounding(Rounding::same(12.0))
                .inner_margin(egui::Margin::symmetric(10.0, 8.0)),
        )
        .show(ctx, |ui| {
            action = render_hud_contents(ui, ctx, palette, state, true);
        });
    action
}

/// Render either a detached floating pill viewport (`HudMode::DetachedPill`)
/// or an auto-hiding hover pill inside the mirror window (`HudMode::AutoHide`).
pub fn render_hud(
    ctx: &egui::Context,
    palette: Palette,
    hud_mode: HudMode,
    state: &HudState,
    mirror_hovered: bool,
) -> Option<HudAction> {
    match hud_mode {
        HudMode::TrayOnly => None,
        HudMode::DetachedPill => render_detached_pill(ctx, palette, state),
        HudMode::AutoHide => {
            if !mirror_hovered && !state.show_stats && !state.show_tools {
                return None;
            }
            render_hover_overlay_pill(ctx, palette, state)
        }
    }
}

fn render_detached_pill(
    ctx: &egui::Context,
    palette: Palette,
    state: &HudState,
) -> Option<HudAction> {
    let mut action = None;
    let size = hud_dimensions(state);
    let vp_id = ViewportId::from_hash_of("region_to_share_detached_hud");

    ctx.show_viewport_immediate(
        vp_id,
        ViewportBuilder::default()
            .with_title("Region to Share Controls")
            .with_position(pos2(24.0, 24.0))
            .with_inner_size(size)
            .with_min_inner_size(size)
            .with_max_inner_size(size)
            .with_decorations(false)
            .with_always_on_top()
            .with_resizable(false)
            .with_taskbar(false),
        |ctx, _class| {
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::none()
                        .fill(palette.bg_surface)
                        .stroke(Stroke::new(1.0_f32, palette.border_strong))
                        .rounding(Rounding::same(12.0))
                        .inner_margin(egui::Margin::symmetric(10.0, 8.0)),
                )
                .show(ctx, |ui| {
                    action = render_hud_contents(ui, ctx, palette, state, true);
                });
        },
    );
    action
}

fn render_hover_overlay_pill(
    ctx: &egui::Context,
    palette: Palette,
    state: &HudState,
) -> Option<HudAction> {
    let mut action = None;
    egui::Area::new(egui::Id::new("mirror_autohide_hud"))
        .anchor(Align2::CENTER_TOP, vec2(0.0, 10.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(Color32::from_rgba_unmultiplied(18, 21, 30, 232))
                .stroke(Stroke::new(1.0_f32, palette.border_strong))
                .rounding(Rounding::same(12.0))
                .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                .show(ui, |ui| {
                    action = render_hud_contents(ui, ctx, palette, state, false);
                });
        });
    action
}

fn render_hud_contents(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    palette: Palette,
    state: &HudState,
    show_drag_grip: bool,
) -> Option<HudAction> {
    let mut action = None;
    ui.spacing_mut().item_spacing = vec2(6.0, 6.0);

    ui.vertical(|ui| {
        // ── Row 1: Ultra-clean Core Controls (zero clutter, zero clipping) ───
        ui.horizontal(|ui| {
            // Dedicated 6-dot drag grip isolated from clickable buttons
            if show_drag_grip {
                let (grip_rect, grip_resp) =
                    ui.allocate_exact_size(vec2(14.0, 32.0), Sense::drag());
                if grip_resp.hovered() {
                    ctx.set_cursor_icon(egui::CursorIcon::Grab);
                }
                if grip_resp.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                let p = ui.painter();
                let dot_col = if grip_resp.hovered() {
                    palette.text_primary
                } else {
                    palette.text_muted
                };
                for row in 0..3 {
                    for col in 0..2 {
                        let x = grip_rect.left() + 3.5 + col as f32 * 6.0;
                        let y = grip_rect.top() + 9.0 + row as f32 * 7.0;
                        p.circle_filled(pos2(x, y), 1.4, dot_col);
                    }
                }
            }

            // Status pill
            if state.is_recording {
                let txt = if state.is_paused {
                    format!("{} {}", t!("hud.paused"), format_duration(state.rec_elapsed_secs))
                } else {
                    format!(
                        "{} {} · {}",
                        t!("hud.rec"),
                        format_duration(state.rec_elapsed_secs),
                        format_bytes(state.rec_bytes)
                    )
                };
                status_badge(
                    ui,
                    palette,
                    &txt,
                    if state.is_paused {
                        palette.accent_warn
                    } else {
                        palette.accent_record
                    },
                    !state.is_paused,
                );
            } else if state.is_paused {
                status_badge(ui, palette, &t!("hud.paused"), palette.accent_warn, false);
            } else if state.virtual_cam_active {
                status_badge(ui, palette, "CAM", palette.accent_purple, true);
            } else {
                status_badge(ui, palette, &t!("hud.live"), palette.accent_live, true);
            }

            // Record / Stop Rec
            let rec_btn = pill_button(
                ui,
                palette,
                if state.is_recording { Icon::Stop } else { Icon::Record },
                "",
                state.is_recording,
                palette.accent_record,
            )
            .on_hover_text(if state.is_recording {
                t!("hud.stop_rec")
            } else {
                t!("hud.record")
            });
            if rec_btn.clicked() {
                action = Some(HudAction::ToggleRecord);
            }

            // Pause / Resume Stream (cuts live screen feed while keeping Portal open & shows pause card/video)
            let p_btn = pill_button(
                ui,
                palette,
                if state.is_paused { Icon::Play } else { Icon::Pause },
                "",
                state.is_paused,
                palette.accent_warn,
            )
            .on_hover_text(if state.is_paused {
                t!("hud.resume")
            } else {
                t!("hud.pause")
            });
            if p_btn.clicked() {
                action = Some(HudAction::TogglePause);
            }

            // Quick Snapshot
            if pill_button(ui, palette, Icon::Camera, "", false, palette.accent_live)
                .on_hover_text(t!("hud.snapshot"))
                .clicked()
            {
                action = Some(HudAction::Snapshot);
            }

            // Reselect Region
            if pill_button(ui, palette, Icon::Refresh, "", false, palette.accent_share)
                .on_hover_text(t!("hud.reselect"))
                .clicked()
            {
                action = Some(HudAction::Reselect);
            }

            // When mirror is visible (Sharing):
            // - Wayland: minimize window (PipeWire screencast keeps sharing minimized windows!)
            // - X11: keep window mapped & lowered behind apps (XComposite requires mapped window)
            // When mirror is hidden (Record-only): 1-click Show Mirror at exact 1:1!
            if state.mirror_visible {
                let below_tip = if state.mirror_keep_below {
                    t!("hud.bring_front")
                } else if DisplayServer::detect().is_wayland() {
                    t!("hud.minimize_wayland")
                } else {
                    t!("hud.keep_below")
                };
                if pill_button(
                    ui,
                    palette,
                    Icon::ArrowDown,
                    "",
                    state.mirror_keep_below,
                    palette.accent_share,
                )
                .on_hover_text(below_tip)
                .clicked()
                {
                    action = Some(HudAction::ToggleKeepBelow);
                }
            } else if pill_button(
                ui,
                palette,
                Icon::Eye,
                "",
                false,
                palette.accent_share,
            )
            .on_hover_text(t!("hud.show_mirror"))
            .clicked()
            {
                action = Some(HudAction::ToggleMirrorVisible);
            }

            // Subtle vertical divider
            let (sep_rect, _) = ui.allocate_exact_size(vec2(4.0, 22.0), Sense::hover());
            ui.painter().line_segment(
                [
                    pos2(sep_rect.center().x, sep_rect.top()),
                    pos2(sep_rect.center().x, sep_rect.bottom()),
                ],
                Stroke::new(1.0_f32, palette.border_subtle),
            );

            // Secondary Tools Drawer toggle (`⋯`)
            let any_tool_active =
                state.show_tools || state.virtual_cam_active || state.show_stats;
            if pill_button(
                ui,
                palette,
                Icon::Sliders,
                "",
                state.show_tools,
                if any_tool_active {
                    palette.accent_share
                } else {
                    palette.text_secondary
                },
            )
            .on_hover_text(t!("hud.tools"))
            .clicked()
            {
                action = Some(HudAction::ToggleTools);
            }

            // End Session (Stop)
            if pill_button(ui, palette, Icon::Close, "", true, palette.accent_record)
                .on_hover_text(t!("hud.stop"))
                .clicked()
            {
                action = Some(HudAction::Stop);
            }
        });

        // ── Row 2: Optional Labeled Tools Strip (only when `⋯` is expanded) ──
        if state.show_tools {
            ui.horizontal(|ui| {
                if tool_chip(
                    ui,
                    palette,
                    Icon::VirtualCam,
                    &t!("hud.vcam_short"),
                    state.virtual_cam_active,
                    palette.accent_purple,
                )
                .on_hover_text(t!("hud.vcam"))
                .clicked()
                {
                    action = Some(HudAction::ToggleVirtualCam);
                }

                if tool_chip(
                    ui,
                    palette,
                    Icon::Activity,
                    &t!("hud.stats"),
                    state.show_stats,
                    palette.accent_share,
                )
                .clicked()
                {
                    action = Some(HudAction::ToggleStats);
                }
            });
        }

        // ── Row 3: Optional Live Telemetry Strip ─────────────────────────────
        if state.show_stats {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "FPS {:.1}  ·  CPU {:.1}%  ·  RAM {:.0} MB{}",
                        state.fps,
                        state.cpu_percent,
                        state.memory_mb,
                        if state.encoder_label.is_empty() {
                            String::new()
                        } else {
                            format!("  ·  ENC {}", state.encoder_label)
                        }
                    ))
                    .monospace()
                    .size(11.0)
                    .color(palette.text_secondary),
                );
            });
        }
    });
    action
}

/// Compact labeled chip used inside the expandable HUD Tools row.
fn tool_chip(
    ui: &mut egui::Ui,
    palette: Palette,
    icon: Icon,
    label: &str,
    active: bool,
    accent: Color32,
) -> Response {
    let font = FontId::proportional(11.5);
    let galley = ui.fonts(|f| f.layout_no_wrap(label.to_string(), font.clone(), Color32::WHITE));
    let w = 8.0 + 13.0 + 5.0 + galley.size().x + 9.0;
    let h = 26.0;

    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
    let hovered = resp.hovered();
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    let (bg, fg, border) = if active {
        (
            Palette::tint(accent, if hovered { 235 } else { 205 }),
            Color32::WHITE,
            accent,
        )
    } else {
        (
            if hovered {
                palette.bg_hover
            } else {
                palette.bg_elevated
            },
            palette.text_secondary,
            if hovered {
                palette.border_strong
            } else {
                palette.border_subtle
            },
        )
    };

    let p = ui.painter();
    p.rect_filled(rect, Rounding::same(7.0), bg);
    p.rect_stroke(rect, Rounding::same(7.0), Stroke::new(1.0_f32, border));

    let icon_rect = Rect::from_center_size(pos2(rect.left() + 14.5, rect.center().y), vec2(13.0, 13.0));
    paint_icon(p, icon_rect, icon, fg);
    p.text(
        pos2(icon_rect.right() + 5.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        font,
        fg,
    );

    resp
}
