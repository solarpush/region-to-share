//! Crisp Lucide-style vector icons rendered directly via `egui::Painter`.
//!
//! Zero broken font emojis, zero external SVG asset failures in Flatpak/Snap,
//! razor-sharp at 100%, 125%, 150%, 200%, and 300% HiDPI scaling.

use egui::{pos2, vec2, Color32, Painter, Pos2, Rect, Rounding, Stroke};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Share,
    Record,
    Stop,
    Pause,
    Play,
    Camera,
    VirtualCam,
    Settings,
    Layers,
    Bookmark,
    Plus,
    Trash,
    Copy,
    Check,
    Close,
    Border,
    Activity,
    ArrowDown,
    Refresh,
    Monitor,
    Eye,
    EyeOff,
    Sliders,
    Sun,
    Moon,
    Sparkles,
}

/// Paint a crisp vector icon centered inside `rect`.
pub fn paint_icon(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height()) * 0.42;
    let sw = (s * 0.22).clamp(1.4, 2.2);
    let stroke = Stroke::new(sw, color);

    match icon {
        Icon::Share => {
            // Dashed/corner crop frame + diagonal share arrow
            let r = Rect::from_center_size(c, vec2(s * 1.9, s * 1.6));
            painter.rect_stroke(r, Rounding::same(s * 0.28), stroke);
            let p0 = pos2(c.x - s * 0.3, c.y + s * 0.3);
            let p1 = pos2(c.x + s * 0.35, c.y - s * 0.35);
            painter.line_segment([p0, p1], stroke);
            painter.line_segment([p1, pos2(p1.x - s * 0.42, p1.y)], stroke);
            painter.line_segment([p1, pos2(p1.x, p1.y + s * 0.42)], stroke);
        }
        Icon::Record => {
            painter.circle_stroke(c, s * 0.92, stroke);
            painter.circle_filled(c, s * 0.46, color);
        }
        Icon::Stop => {
            let r = Rect::from_center_size(c, vec2(s * 1.35, s * 1.35));
            painter.rect_filled(r, Rounding::same(s * 0.25), color);
        }
        Icon::Pause => {
            let w = s * 0.36;
            let h = s * 1.4;
            let gap = s * 0.44;
            painter.rect_filled(
                Rect::from_center_size(pos2(c.x - gap, c.y), vec2(w, h)),
                Rounding::same(w * 0.4),
                color,
            );
            painter.rect_filled(
                Rect::from_center_size(pos2(c.x + gap, c.y), vec2(w, h)),
                Rounding::same(w * 0.4),
                color,
            );
        }
        Icon::Play => {
            let pts = vec![
                pos2(c.x - s * 0.5, c.y - s * 0.72),
                pos2(c.x + s * 0.75, c.y),
                pos2(c.x - s * 0.5, c.y + s * 0.72),
            ];
            painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        Icon::Camera => {
            let body = Rect::from_center_size(pos2(c.x, c.y + s * 0.1), vec2(s * 1.9, s * 1.3));
            painter.rect_stroke(body, Rounding::same(s * 0.26), stroke);
            painter.circle_stroke(pos2(c.x, c.y + s * 0.1), s * 0.38, stroke);
            painter.line_segment(
                [
                    pos2(c.x - s * 0.35, body.top()),
                    pos2(c.x - s * 0.2, body.top() - s * 0.28),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    pos2(c.x - s * 0.2, body.top() - s * 0.28),
                    pos2(c.x + s * 0.2, body.top() - s * 0.28),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    pos2(c.x + s * 0.2, body.top() - s * 0.28),
                    pos2(c.x + s * 0.35, body.top()),
                ],
                stroke,
            );
        }
        Icon::VirtualCam => {
            let body = Rect::from_center_size(pos2(c.x - s * 0.22, c.y), vec2(s * 1.35, s * 1.15));
            painter.rect_stroke(body, Rounding::same(s * 0.25), stroke);
            let lens = [
                pos2(body.right() + s * 0.08, c.y - s * 0.2),
                pos2(c.x + s * 0.95, c.y - s * 0.52),
                pos2(c.x + s * 0.95, c.y + s * 0.52),
                pos2(body.right() + s * 0.08, c.y + s * 0.2),
            ];
            painter.line_segment([lens[0], lens[1]], stroke);
            painter.line_segment([lens[1], lens[2]], stroke);
            painter.line_segment([lens[2], lens[3]], stroke);
        }
        Icon::Settings => {
            painter.circle_stroke(c, s * 0.38, stroke);
            painter.circle_stroke(c, s * 0.78, stroke);
            for i in 0..8 {
                let angle = (i as f32) * std::f32::consts::FRAC_PI_4;
                let dir = vec2(angle.cos(), angle.sin());
                painter.line_segment([c + dir * (s * 0.76), c + dir * (s * 1.02)], stroke);
            }
        }
        Icon::Layers => {
            let top = [
                pos2(c.x, c.y - s * 0.8),
                pos2(c.x + s * 0.9, c.y - s * 0.3),
                pos2(c.x, c.y + s * 0.2),
                pos2(c.x - s * 0.9, c.y - s * 0.3),
            ];
            for i in 0..4 {
                painter.line_segment([top[i], top[(i + 1) % 4]], stroke);
            }
            painter.line_segment(
                [pos2(c.x - s * 0.85, c.y + s * 0.25), pos2(c.x, c.y + s * 0.72)],
                stroke,
            );
            painter.line_segment(
                [pos2(c.x, c.y + s * 0.72), pos2(c.x + s * 0.85, c.y + s * 0.25)],
                stroke,
            );
        }
        Icon::Bookmark => {
            let pts = [
                pos2(c.x - s * 0.58, c.y - s * 0.85),
                pos2(c.x + s * 0.58, c.y - s * 0.85),
                pos2(c.x + s * 0.58, c.y + s * 0.85),
                pos2(c.x, c.y + s * 0.38),
                pos2(c.x - s * 0.58, c.y + s * 0.85),
            ];
            for i in 0..pts.len() {
                painter.line_segment([pts[i], pts[(i + 1) % pts.len()]], stroke);
            }
        }
        Icon::Plus => {
            painter.line_segment([pos2(c.x - s * 0.7, c.y), pos2(c.x + s * 0.7, c.y)], stroke);
            painter.line_segment([pos2(c.x, c.y - s * 0.7), pos2(c.x, c.y + s * 0.7)], stroke);
        }
        Icon::Trash => {
            painter.line_segment(
                [pos2(c.x - s * 0.75, c.y - s * 0.52), pos2(c.x + s * 0.75, c.y - s * 0.52)],
                stroke,
            );
            let box_r = Rect::from_min_max(
                pos2(c.x - s * 0.55, c.y - s * 0.52),
                pos2(c.x + s * 0.55, c.y + s * 0.82),
            );
            painter.rect_stroke(box_r, Rounding::same(s * 0.16), stroke);
            painter.line_segment(
                [pos2(c.x - s * 0.25, c.y - s * 0.8), pos2(c.x + s * 0.25, c.y - s * 0.8)],
                stroke,
            );
        }
        Icon::Copy => {
            let back = Rect::from_center_size(
                pos2(c.x - s * 0.2, c.y - s * 0.2),
                vec2(s * 1.15, s * 1.25),
            );
            let front = Rect::from_center_size(
                pos2(c.x + s * 0.24, c.y + s * 0.24),
                vec2(s * 1.15, s * 1.25),
            );
            painter.rect_stroke(back, Rounding::same(s * 0.2), stroke);
            painter.rect_stroke(front, Rounding::same(s * 0.2), stroke);
        }
        Icon::Check => {
            painter.line_segment(
                [pos2(c.x - s * 0.7, c.y + s * 0.05), pos2(c.x - s * 0.15, c.y + s * 0.58)],
                stroke,
            );
            painter.line_segment(
                [pos2(c.x - s * 0.15, c.y + s * 0.58), pos2(c.x + s * 0.75, c.y - s * 0.52)],
                stroke,
            );
        }
        Icon::Close => {
            painter.line_segment(
                [pos2(c.x - s * 0.62, c.y - s * 0.62), pos2(c.x + s * 0.62, c.y + s * 0.62)],
                stroke,
            );
            painter.line_segment(
                [pos2(c.x + s * 0.62, c.y - s * 0.62), pos2(c.x - s * 0.62, c.y + s * 0.62)],
                stroke,
            );
        }
        Icon::Border => {
            // 4 corner brackets
            let d = s * 0.82;
            let l = s * 0.48;
            let corners: [(Pos2, Pos2, Pos2); 4] = [
                (pos2(c.x - d, c.y - d + l), pos2(c.x - d, c.y - d), pos2(c.x - d + l, c.y - d)),
                (pos2(c.x + d - l, c.y - d), pos2(c.x + d, c.y - d), pos2(c.x + d, c.y - d + l)),
                (pos2(c.x + d, c.y + d - l), pos2(c.x + d, c.y + d), pos2(c.x + d - l, c.y + d)),
                (pos2(c.x - d + l, c.y + d), pos2(c.x - d, c.y + d), pos2(c.x - d, c.y + d - l)),
            ];
            for (a, b, e) in corners {
                painter.line_segment([a, b], stroke);
                painter.line_segment([b, e], stroke);
            }
        }
        Icon::Activity => {
            let pts = [
                pos2(c.x - s * 0.9, c.y),
                pos2(c.x - s * 0.4, c.y),
                pos2(c.x - s * 0.15, c.y - s * 0.7),
                pos2(c.x + s * 0.2, c.y + s * 0.65),
                pos2(c.x + s * 0.45, c.y),
                pos2(c.x + s * 0.9, c.y),
            ];
            for w in pts.windows(2) {
                painter.line_segment([w[0], w[1]], stroke);
            }
        }
        Icon::ArrowDown => {
            painter.line_segment([pos2(c.x, c.y - s * 0.72), pos2(c.x, c.y + s * 0.68)], stroke);
            painter.line_segment(
                [pos2(c.x - s * 0.5, c.y + s * 0.2), pos2(c.x, c.y + s * 0.7)],
                stroke,
            );
            painter.line_segment(
                [pos2(c.x + s * 0.5, c.y + s * 0.2), pos2(c.x, c.y + s * 0.7)],
                stroke,
            );
        }
        Icon::Refresh => {
            painter.circle_stroke(c, s * 0.72, stroke);
            painter.line_segment(
                [pos2(c.x + s * 0.35, c.y - s * 0.68), pos2(c.x + s * 0.82, c.y - s * 0.68)],
                stroke,
            );
            painter.line_segment(
                [pos2(c.x + s * 0.76, c.y - s * 0.68), pos2(c.x + s * 0.76, c.y - s * 0.22)],
                stroke,
            );
        }
        Icon::Monitor => {
            let screen = Rect::from_center_size(pos2(c.x, c.y - s * 0.15), vec2(s * 1.9, s * 1.25));
            painter.rect_stroke(screen, Rounding::same(s * 0.22), stroke);
            painter.line_segment(
                [pos2(c.x, screen.bottom()), pos2(c.x, c.y + s * 0.85)],
                stroke,
            );
            painter.line_segment(
                [pos2(c.x - s * 0.52, c.y + s * 0.85), pos2(c.x + s * 0.52, c.y + s * 0.85)],
                stroke,
            );
        }
        Icon::Eye => {
            let top = [
                pos2(c.x - s * 0.95, c.y),
                pos2(c.x - s * 0.45, c.y - s * 0.52),
                pos2(c.x, c.y - s * 0.65),
                pos2(c.x + s * 0.45, c.y - s * 0.52),
                pos2(c.x + s * 0.95, c.y),
            ];
            let bot = [
                pos2(c.x - s * 0.95, c.y),
                pos2(c.x - s * 0.45, c.y + s * 0.52),
                pos2(c.x, c.y + s * 0.65),
                pos2(c.x + s * 0.45, c.y + s * 0.52),
                pos2(c.x + s * 0.95, c.y),
            ];
            for w in top.windows(2) {
                painter.line_segment([w[0], w[1]], stroke);
            }
            for w in bot.windows(2) {
                painter.line_segment([w[0], w[1]], stroke);
            }
            painter.circle_stroke(c, s * 0.32, stroke);
        }
        Icon::EyeOff => {
            let top = [
                pos2(c.x - s * 0.92, c.y),
                pos2(c.x - s * 0.42, c.y - s * 0.48),
                pos2(c.x, c.y - s * 0.58),
                pos2(c.x + s * 0.42, c.y - s * 0.48),
                pos2(c.x + s * 0.92, c.y),
            ];
            let bot = [
                pos2(c.x - s * 0.92, c.y),
                pos2(c.x - s * 0.42, c.y + s * 0.48),
                pos2(c.x, c.y + s * 0.58),
                pos2(c.x + s * 0.42, c.y + s * 0.48),
                pos2(c.x + s * 0.92, c.y),
            ];
            for w in top.windows(2) {
                painter.line_segment([w[0], w[1]], stroke);
            }
            for w in bot.windows(2) {
                painter.line_segment([w[0], w[1]], stroke);
            }
            painter.line_segment(
                [pos2(c.x - s * 0.82, c.y - s * 0.72), pos2(c.x + s * 0.82, c.y + s * 0.72)],
                stroke,
            );
        }
        Icon::Sliders => {
            // 3 clean horizontal dots (`⋯`) for secondary tools drawer
            let r = (s * 0.22).max(1.8);
            painter.circle_filled(pos2(c.x - s * 0.62, c.y), r, color);
            painter.circle_filled(c, r, color);
            painter.circle_filled(pos2(c.x + s * 0.62, c.y), r, color);
        }
        Icon::Sun => {
            painter.circle_stroke(c, s * 0.45, stroke);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                let d = vec2(a.cos(), a.sin());
                painter.line_segment([c + d * (s * 0.72), c + d * (s * 0.98)], stroke);
            }
        }
        Icon::Moon => {
            painter.circle_stroke(c, s * 0.72, stroke);
            painter.circle_filled(pos2(c.x + s * 0.32, c.y - s * 0.26), s * 0.42, color);
        }
        Icon::Sparkles => {
            // 4-point star
            painter.line_segment([pos2(c.x, c.y - s * 0.9), pos2(c.x, c.y + s * 0.9)], stroke);
            painter.line_segment([pos2(c.x - s * 0.9, c.y), pos2(c.x + s * 0.9, c.y)], stroke);
            painter.circle_filled(c, s * 0.24, color);
        }
    }
}
