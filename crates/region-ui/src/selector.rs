//! Pro Region Selector Overlay (`Spotlight Scrim` + 8 resize handles + 4x loupe + floating action pill).

use crate::icons::Icon;
use crate::theme::Palette;
use crate::widgets::pill_button;
use egui::{pos2, vec2, Align2, Color32, CursorIcon, FontId, Key, Pos2, Rect, Rounding, Sense, Stroke, Ui};
use region_core::{AspectRatio, Rectangle};
use region_overlay::{ResizeHandle, SelectionEditor};
use rust_i18n::t;

/// Outcome chosen by the user in the region selector.
#[derive(Debug, Clone)]
pub enum SelectorOutcome {
    Share(Rectangle),
    Record(Rectangle),
    ShareAndRecord(Rectangle),
    VirtualCam(Rectangle),
    SnapshotClipboard(Rectangle),
    SavePreset { name: String, rect: Rectangle },
    Cancel,
}

/// Interactive state for the full-screen spotlight region selector.
pub struct RegionSelectorView {
    pub editor: SelectionEditor,
    pub show_grid: bool,
    pub show_magnifier: bool,
    pub preset_name_draft: String,
    pub saving_preset: bool,
}

impl RegionSelectorView {
    pub fn new(aspect: AspectRatio, show_grid: bool, show_magnifier: bool) -> Self {
        Self {
            editor: SelectionEditor::new(aspect),
            show_grid,
            show_magnifier,
            preset_name_draft: String::new(),
            saving_preset: false,
        }
    }

    /// Compute an aspect-ratio-preserving rect centered inside `bounds`.
    pub fn fit_aspect_rect(bounds: Rect, content_w: f32, content_h: f32) -> Rect {
        if content_w <= 0.0 || content_h <= 0.0 || bounds.width() <= 0.0 || bounds.height() <= 0.0 {
            return bounds;
        }
        let content_aspect = content_w / content_h;
        let bounds_aspect = bounds.width() / bounds.height();
        let (w, h) = if (content_aspect - bounds_aspect).abs() < 0.005 {
            (bounds.width(), bounds.height())
        } else if content_aspect > bounds_aspect {
            let w = bounds.width();
            (w, w / content_aspect)
        } else {
            let h = bounds.height();
            (h * content_aspect, h)
        };
        Rect::from_center_size(bounds.center(), vec2(w, h))
    }

    /// Render the full-screen spotlight selector over `screenshot`.
    /// Returns `Some(SelectorOutcome)` once the user triggers an action or cancels.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        ui: &mut Ui,
        palette: Palette,
        screenshot: &egui::TextureHandle,
        backend_screen_size: Option<(u32, u32)>,
    ) -> Option<SelectorOutcome> {
        let screen_rect = ui.max_rect();
        let tex_size = screenshot.size_vec2();
        // Preserve exact aspect ratio (100% full-bleed on matching monitor, zero squish on multi-monitor/ultrawide)
        let image_rect = Self::fit_aspect_rect(screen_rect, tex_size.x, tex_size.y);
        let iw = image_rect.width().max(1.0);
        let ih = image_rect.height().max(1.0);

        // 1. Crisp frozen desktop background preserving aspect ratio
        ui.painter().rect_filled(screen_rect, 0.0, Color32::BLACK);
        ui.put(image_rect, egui::Image::new((screenshot.id(), image_rect.size())));

        // 2. Keyboard controls
        let mut outcome: Option<SelectorOutcome> = None;
        if !self.saving_preset {
            let shift = ctx.input(|i| i.modifiers.shift);
            let step = if shift { 10 } else { 1 };
            if ctx.input(|i| i.key_pressed(Key::Escape)) {
                return Some(SelectorOutcome::Cancel);
            }
            if ctx.input(|i| i.key_pressed(Key::ArrowLeft)) {
                self.editor.nudge(-step, 0, iw as u32, ih as u32);
            }
            if ctx.input(|i| i.key_pressed(Key::ArrowRight)) {
                self.editor.nudge(step, 0, iw as u32, ih as u32);
            }
            if ctx.input(|i| i.key_pressed(Key::ArrowUp)) {
                self.editor.nudge(0, -step, iw as u32, ih as u32);
            }
            if ctx.input(|i| i.key_pressed(Key::ArrowDown)) {
                self.editor.nudge(0, step, iw as u32, ih as u32);
            }
        }

        // Convert UI selection rect (relative to image_rect) into backend coordinate space
        // (portal logical space on Wayland, physical pixels on X11 — matching v2's fractional scaling math)
        let to_backend_rect = |r: Rectangle| -> Rectangle {
            let (ref_w, ref_h) = backend_screen_size
                .map(|(w, h)| (w as f32, h as f32))
                .unwrap_or((tex_size.x, tex_size.y));
            let sx = ref_w / iw;
            let sy = ref_h / ih;
            Rectangle::new(
                (r.x as f32 * sx).round() as i32,
                (r.y as f32 * sy).round() as i32,
                ((r.width as f32 * sx).round() as u32).max(2),
                ((r.height as f32 * sy).round() as u32).max(2),
            )
        };

        // 3. Draw 4-panel INVERTED spotlight scrim (interior stays 100% bright & crystal clear!)
        let painter = ui.painter();
        let sel_egui_rect: Option<Rect> = self.editor.rect.map(|r| {
            Rect::from_min_size(
                pos2(image_rect.left() + r.x as f32, image_rect.top() + r.y as f32),
                vec2(r.width as f32, r.height as f32),
            )
        });

        if let Some(sel) = sel_egui_rect {
            // Top
            painter.rect_filled(
                Rect::from_min_max(screen_rect.min, pos2(screen_rect.right(), sel.top())),
                0.0,
                palette.scrim,
            );
            // Bottom
            painter.rect_filled(
                Rect::from_min_max(pos2(screen_rect.left(), sel.bottom()), screen_rect.max),
                0.0,
                palette.scrim,
            );
            // Left
            painter.rect_filled(
                Rect::from_min_max(pos2(screen_rect.left(), sel.top()), pos2(sel.left(), sel.bottom())),
                0.0,
                palette.scrim,
            );
            // Right
            painter.rect_filled(
                Rect::from_min_max(pos2(sel.right(), sel.top()), pos2(screen_rect.right(), sel.bottom())),
                0.0,
                palette.scrim,
            );

            // Rule-of-thirds subtle guides inside the bright selection
            if self.show_grid && sel.width() > 90.0 && sel.height() > 90.0 {
                let grid_stroke = Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 45));
                for k in [1.0 / 3.0, 2.0 / 3.0] {
                    let gx = sel.left() + sel.width() * k;
                    let gy = sel.top() + sel.height() * k;
                    painter.line_segment([pos2(gx, sel.top()), pos2(gx, sel.bottom())], grid_stroke);
                    painter.line_segment([pos2(sel.left(), gy), pos2(sel.right(), gy)], grid_stroke);
                }
            }

            // Crisp glowing accent border + outer hair shadow
            painter.rect_stroke(
                sel.expand(1.5),
                Rounding::same(2.0),
                Stroke::new(1.0_f32, Color32::from_black_alpha(150)),
            );
            painter.rect_stroke(
                sel,
                Rounding::same(1.5),
                Stroke::new(2.0_f32, palette.accent_share),
            );

            // 8 interactive white resize handles
            let handle_pts = [
                sel.left_top(),
                pos2(sel.center().x, sel.top()),
                sel.right_top(),
                pos2(sel.right(), sel.center().y),
                sel.right_bottom(),
                pos2(sel.center().x, sel.bottom()),
                sel.left_bottom(),
                pos2(sel.left(), sel.center().y),
            ];
            for pt in handle_pts {
                painter.circle_filled(pt, 5.0, Color32::WHITE);
                painter.circle_stroke(pt, 5.0, Stroke::new(1.5_f32, palette.accent_share));
            }

            // Sleek dimension pill above selection
            if let Some(raw_r) = self.editor.rect {
                let br = to_backend_rect(raw_r);
                let dim_text = format!(
                    "{} × {}  ·  {}  ·  ({}, {})",
                    br.width,
                    br.height,
                    br.aspect_label(),
                    br.x,
                    br.y
                );
                let font = FontId::monospace(12.0);
                let galley = ui.fonts(|f| f.layout_no_wrap(dim_text.clone(), font.clone(), Color32::WHITE));
                let pill_w = galley.size().x + 20.0;
                let pill_h = 26.0;
                let pill_y = if sel.top() > pill_h + 14.0 {
                    sel.top() - pill_h - 8.0
                } else {
                    sel.top() + 8.0
                };
                let pill_x = sel.left().clamp(8.0, (screen_rect.right() - pill_w - 8.0).max(8.0));
                let pill_rect = Rect::from_min_size(pos2(pill_x, pill_y), vec2(pill_w, pill_h));
                painter.rect_filled(
                    pill_rect,
                    Rounding::same(8.0),
                    Color32::from_rgba_unmultiplied(15, 17, 23, 225),
                );
                painter.rect_stroke(
                    pill_rect,
                    Rounding::same(8.0),
                    Stroke::new(1.0_f32, Palette::tint(palette.accent_share, 130)),
                );
                painter.text(
                    pill_rect.center(),
                    Align2::CENTER_CENTER,
                    dim_text,
                    font,
                    Color32::WHITE,
                );
            }
        } else {
            // Before any selection is drawn: soft dim backdrop + crosshair guides
            painter.rect_filled(screen_rect, 0.0, palette.scrim);
            if let Some(hover) = ctx.input(|i| i.pointer.hover_pos()) {
                let guide = Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 55));
                painter.line_segment([pos2(hover.x, screen_rect.top()), pos2(hover.x, screen_rect.bottom())], guide);
                painter.line_segment([pos2(screen_rect.left(), hover.y), pos2(screen_rect.right(), hover.y)], guide);
            }
        }

        // 4. Top floating Aspect-Ratio & Hint Toolbar
        let top_bar_id = egui::Id::new("selector_top_toolbar");
        let top_bar_area = egui::Area::new(top_bar_id)
            .anchor(Align2::CENTER_TOP, vec2(0.0, 16.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(Color32::from_rgba_unmultiplied(18, 21, 30, 232))
                    .stroke(Stroke::new(1.0_f32, palette.border_strong))
                    .rounding(Rounding::same(12.0))
                    .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for ratio in AspectRatio::all() {
                                let selected = self.editor.aspect_ratio == *ratio;
                                let btn = ui.add(
                                    egui::Button::new(
                                        egui::RichText::new(ratio.label())
                                            .size(11.5)
                                            .color(if selected {
                                                Color32::WHITE
                                            } else {
                                                palette.text_secondary
                                            }),
                                    )
                                    .fill(if selected {
                                        palette.accent_share
                                    } else {
                                        Color32::TRANSPARENT
                                    })
                                    .rounding(Rounding::same(6.0)),
                                );
                                if btn.clicked() {
                                    self.editor.set_aspect_ratio(*ratio, iw as u32, ih as u32);
                                }
                            }
                            ui.separator();
                            let grid_btn = ui.selectable_label(self.show_grid, "Grid");
                            if grid_btn.clicked() {
                                self.show_grid = !self.show_grid;
                            }
                            let loupe_btn = ui.selectable_label(self.show_magnifier, "4×");
                            if loupe_btn.clicked() {
                                self.show_magnifier = !self.show_magnifier;
                            }
                        });
                        ui.add_space(2.0);
                        let hint = if self.editor.rect.is_some() {
                            t!("selector.hint_adjust")
                        } else {
                            t!("selector.hint_draw")
                        };
                        ui.label(
                            egui::RichText::new(hint)
                                .size(11.0)
                                .color(palette.text_muted),
                        );
                    });
            });

        // 5. Floating Glass Action Pill below selection (when a region is ready & not dragging)
        let mut action_bar_rect: Option<Rect> = None;
        if let (Some(sel), Some(raw_r)) = (sel_egui_rect, self.editor.rect) {
            if raw_r.width >= 24 && raw_r.height >= 24 && self.editor.active_handle.is_none() {
                let backend_r = to_backend_rect(raw_r);

                // Keyboard instant triggers
                if !self.saving_preset {
                    if ctx.input(|i| i.key_pressed(Key::Enter)) {
                        return Some(SelectorOutcome::Share(backend_r));
                    }
                    if ctx.input(|i| i.key_pressed(Key::R)) {
                        return Some(SelectorOutcome::Record(backend_r));
                    }
                    if ctx.input(|i| i.key_pressed(Key::V)) {
                        return Some(SelectorOutcome::VirtualCam(backend_r));
                    }
                    if ctx.input(|i| i.key_pressed(Key::C)) {
                        return Some(SelectorOutcome::SnapshotClipboard(backend_r));
                    }
                }

                let bar_y = if sel.bottom() + 62.0 < screen_rect.bottom() {
                    sel.bottom() + 12.0
                } else {
                    (sel.bottom() - 56.0).max(screen_rect.top() + 72.0)
                };
                let bar_pos = pos2(
                    (sel.center().x - 235.0).clamp(16.0, (screen_rect.right() - 486.0).max(16.0)),
                    bar_y,
                );

                let area_resp = egui::Area::new(egui::Id::new("selector_action_pill"))
                    .fixed_pos(bar_pos)
                    .order(egui::Order::Foreground)
                    .show(ctx, |ui| {
                        egui::Frame::none()
                            .fill(Color32::from_rgba_unmultiplied(18, 21, 30, 240))
                            .stroke(Stroke::new(1.0_f32, palette.border_strong))
                            .rounding(Rounding::same(13.0))
                            .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                            .show(ui, |ui| {
                                if self.saving_preset {
                                    ui.horizontal(|ui| {
                                        let te = ui.add(
                                            egui::TextEdit::singleline(&mut self.preset_name_draft)
                                                .hint_text(t!("selector.save_preset_placeholder"))
                                                .desired_width(170.0),
                                        );
                                        te.request_focus();
                                        let enter = ctx.input(|i| i.key_pressed(Key::Enter));
                                        if pill_button(ui, palette, Icon::Check, "Save", true, palette.accent_live).clicked() || enter {
                                            let name = if self.preset_name_draft.trim().is_empty() {
                                                format!("{}×{}", backend_r.width, backend_r.height)
                                            } else {
                                                self.preset_name_draft.trim().to_string()
                                            };
                                            outcome = Some(SelectorOutcome::SavePreset {
                                                name,
                                                rect: backend_r,
                                            });
                                            self.saving_preset = false;
                                            self.preset_name_draft.clear();
                                        }
                                        if pill_button(ui, palette, Icon::Close, "", false, palette.text_muted).clicked() {
                                            self.saving_preset = false;
                                        }
                                    });
                                } else {
                                    ui.horizontal(|ui| {
                                        if pill_button(ui, palette, Icon::Share, &t!("hud.share"), true, palette.accent_share).clicked() {
                                            outcome = Some(SelectorOutcome::Share(backend_r));
                                        }
                                        if pill_button(ui, palette, Icon::Record, &t!("hud.record"), true, palette.accent_record).clicked() {
                                            outcome = Some(SelectorOutcome::Record(backend_r));
                                        }
                                        if pill_button(ui, palette, Icon::VirtualCam, "", false, palette.accent_purple)
                                            .on_hover_text(t!("hud.vcam"))
                                            .clicked()
                                        {
                                            outcome = Some(SelectorOutcome::VirtualCam(backend_r));
                                        }
                                        if pill_button(ui, palette, Icon::Camera, "", false, palette.accent_live)
                                            .on_hover_text(t!("hub.snapshot_title"))
                                            .clicked()
                                        {
                                            outcome = Some(SelectorOutcome::SnapshotClipboard(backend_r));
                                        }
                                        if pill_button(ui, palette, Icon::Bookmark, "", false, palette.accent_warn)
                                            .on_hover_text("Save Preset")
                                            .clicked()
                                        {
                                            self.saving_preset = true;
                                        }
                                        if pill_button(ui, palette, Icon::Close, "", false, palette.text_muted)
                                            .on_hover_text("Esc")
                                            .clicked()
                                        {
                                            outcome = Some(SelectorOutcome::Cancel);
                                        }
                                    });
                                }
                            });
                    });
                action_bar_rect = Some(area_resp.response.rect);
            }
        }

        if outcome.is_some() {
            return outcome;
        }

        // 6. Pointer interaction (ignore clicks hitting floating toolbars)
        let top_rect = top_bar_area.response.rect;
        let pointer_over_ui = ctx.input(|i| i.pointer.hover_pos()).is_some_and(|pos| {
            top_rect.contains(pos) || action_bar_rect.is_some_and(|r| r.contains(pos))
        });

        let bg_resp = ui.allocate_rect(screen_rect, Sense::click_and_drag());

        if !pointer_over_ui {
            if let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) {
                let rel_x = pos.x - image_rect.left();
                let rel_y = pos.y - image_rect.top();
                let cursor = match self.editor.hit_test(rel_x, rel_y, 10.0) {
                    Some(ResizeHandle::TopLeft | ResizeHandle::BottomRight) => CursorIcon::ResizeNwSe,
                    Some(ResizeHandle::TopRight | ResizeHandle::BottomLeft) => CursorIcon::ResizeNeSw,
                    Some(ResizeHandle::Top | ResizeHandle::Bottom) => CursorIcon::ResizeVertical,
                    Some(ResizeHandle::Left | ResizeHandle::Right) => CursorIcon::ResizeHorizontal,
                    Some(ResizeHandle::MoveInterior) => CursorIcon::Grab,
                    None => CursorIcon::Crosshair,
                };
                ctx.set_cursor_icon(cursor);

                // 4x Magnifier Loupe near pointer when drawing/resizing
                if self.show_magnifier && (self.editor.rect.is_none() || bg_resp.dragged()) {
                    Self::paint_magnifier(ui, palette, screenshot, image_rect, pos);
                }
            }

            if bg_resp.drag_started() {
                if let Some(pos) = bg_resp.interact_pointer_pos() {
                    self.editor.begin_drag(
                        pos.x - image_rect.left(),
                        pos.y - image_rect.top(),
                        10.0,
                    );
                }
            }
            if bg_resp.dragged() {
                if let Some(pos) = bg_resp.interact_pointer_pos() {
                    self.editor.update_drag(
                        pos.x - image_rect.left(),
                        pos.y - image_rect.top(),
                        iw,
                        ih,
                    );
                }
            }
            if bg_resp.drag_stopped() {
                self.editor.end_drag();
            }
        }

        None
    }

    /// Render a 4x zoomed circular pixel loupe offset cleanly from the pointer.
    fn paint_magnifier(
        ui: &Ui,
        palette: Palette,
        screenshot: &egui::TextureHandle,
        screen_rect: Rect,
        cursor: Pos2,
    ) {
        let loupe_size = 116.0;
        let offset_x = if cursor.x + loupe_size + 28.0 < screen_rect.right() {
            24.0
        } else {
            -loupe_size - 24.0
        };
        let offset_y = if cursor.y + loupe_size + 28.0 < screen_rect.bottom() {
            24.0
        } else {
            -loupe_size - 24.0
        };

        let loupe_rect = Rect::from_min_size(
            pos2(cursor.x + offset_x, cursor.y + offset_y),
            vec2(loupe_size, loupe_size),
        );

        let uv_x = ((cursor.x - screen_rect.left()) / screen_rect.width()).clamp(0.0, 1.0);
        let uv_y = ((cursor.y - screen_rect.top()) / screen_rect.height()).clamp(0.0, 1.0);
        let half_u = (loupe_size / 8.0) / screen_rect.width().max(1.0);
        let half_v = (loupe_size / 8.0) / screen_rect.height().max(1.0);
        let uv = Rect::from_min_max(
            pos2((uv_x - half_u).clamp(0.0, 1.0), (uv_y - half_v).clamp(0.0, 1.0)),
            pos2((uv_x + half_u).clamp(0.0, 1.0), (uv_y + half_v).clamp(0.0, 1.0)),
        );

        let p = ui.painter();
        p.rect_filled(loupe_rect.expand(2.0), Rounding::same(14.0), Color32::from_black_alpha(190));
        p.image(screenshot.id(), loupe_rect, uv, Color32::WHITE);
        p.rect_stroke(loupe_rect, Rounding::same(12.0), Stroke::new(2.0_f32, palette.accent_share));

        // Crosshair inside loupe
        let c = loupe_rect.center();
        let ch = Stroke::new(1.0_f32, Palette::tint(palette.accent_share, 200));
        p.line_segment([pos2(loupe_rect.left(), c.y), pos2(loupe_rect.right(), c.y)], ch);
        p.line_segment([pos2(c.x, loupe_rect.top()), pos2(c.x, loupe_rect.bottom())], ch);
    }
}
