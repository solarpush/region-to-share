//! Interactive selection box state machine with 8 resize handles, drag-move,
//! keyboard nudging, grid snapping, and aspect-ratio locking.

use region_core::{AspectRatio, Rectangle};

/// Drag/resize handle identifiers around an active selection rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeHandle {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
    MoveInterior,
}

/// Pure-logic region selection editor (independent of GUI framework).
#[derive(Debug, Clone)]
pub struct SelectionEditor {
    /// Current normalized selection rectangle in logical coordinates.
    pub rect: Option<Rectangle>,
    /// Active aspect-ratio constraint.
    pub aspect_ratio: AspectRatio,
    /// Optional grid snap spacing (0 = disabled).
    pub snap_grid: u32,
    /// Active handle being dragged.
    pub active_handle: Option<ResizeHandle>,
    drag_origin: (f32, f32),
    rect_at_drag_start: Option<Rectangle>,
    drawing_new: bool,
}

impl SelectionEditor {
    pub fn new(aspect_ratio: AspectRatio) -> Self {
        Self {
            rect: None,
            aspect_ratio,
            snap_grid: 0,
            active_handle: None,
            drag_origin: (0.0, 0.0),
            rect_at_drag_start: None,
            drawing_new: false,
        }
    }

    /// Seed an existing rectangle (e.g. from last selection or a preset).
    pub fn set_rect(&mut self, rect: Rectangle) {
        self.rect = Some(rect);
    }

    pub fn clear(&mut self) {
        self.rect = None;
        self.active_handle = None;
        self.drawing_new = false;
    }

    /// Hit-test a pointer position against the 8 handles and interior of the current rect.
    pub fn hit_test(&self, px: f32, py: f32, handle_radius: f32) -> Option<ResizeHandle> {
        let r = self.rect?;
        let x1 = r.x as f32;
        let y1 = r.y as f32;
        let x2 = (r.x + r.width as i32) as f32;
        let y2 = (r.y + r.height as i32) as f32;
        let mx = (x1 + x2) * 0.5;
        let my = (y1 + y2) * 0.5;

        let corners = [
            (ResizeHandle::TopLeft, x1, y1),
            (ResizeHandle::TopRight, x2, y1),
            (ResizeHandle::BottomLeft, x1, y2),
            (ResizeHandle::BottomRight, x2, y2),
            (ResizeHandle::Top, mx, y1),
            (ResizeHandle::Bottom, mx, y2),
            (ResizeHandle::Left, x1, my),
            (ResizeHandle::Right, x2, my),
        ];

        for (h, cx, cy) in corners {
            if (px - cx).abs() <= handle_radius && (py - cy).abs() <= handle_radius {
                return Some(h);
            }
        }

        if px >= x1 && px <= x2 && py >= y1 && py <= y2 {
            return Some(ResizeHandle::MoveInterior);
        }

        None
    }

    /// Begin pointer drag at `(px, py)`.
    pub fn begin_drag(&mut self, px: f32, py: f32, handle_radius: f32) {
        self.drag_origin = (px, py);
        if let Some(handle) = self.hit_test(px, py, handle_radius) {
            self.active_handle = Some(handle);
            self.rect_at_drag_start = self.rect;
            self.drawing_new = false;
        } else {
            self.active_handle = None;
            self.rect_at_drag_start = None;
            self.drawing_new = true;
            self.rect = Some(Rectangle::new(px.round() as i32, py.round() as i32, 1, 1));
        }
    }

    /// Update pointer drag to `(px, py)` clamped inside `(bounds_w, bounds_h)`.
    pub fn update_drag(&mut self, px: f32, py: f32, bounds_w: f32, bounds_h: f32) {
        let px = px.clamp(0.0, bounds_w);
        let py = py.clamp(0.0, bounds_h);

        if self.drawing_new {
            let (ox, oy) = self.drag_origin;
            let dx = px - ox;
            let mut dy = py - oy;

            if let Some(ratio) = self.aspect_ratio.value() {
                let target_h = dx.abs() / ratio;
                dy = if dy >= 0.0 { target_h } else { -target_h };
            }

            let x1 = ox.min(ox + dx).clamp(0.0, bounds_w);
            let y1 = oy.min(oy + dy).clamp(0.0, bounds_h);
            let x2 = ox.max(ox + dx).clamp(0.0, bounds_w);
            let y2 = oy.max(oy + dy).clamp(0.0, bounds_h);

            let rect = Rectangle::new(
                self.snap(x1.round() as i32),
                self.snap(y1.round() as i32),
                (x2 - x1).round().max(1.0) as u32,
                (y2 - y1).round().max(1.0) as u32,
            );
            self.rect = Some(rect.clamp_to_bounds(bounds_w as u32, bounds_h as u32));
            return;
        }

        let (Some(handle), Some(orig)) = (self.active_handle, self.rect_at_drag_start) else {
            return;
        };

        let dx = (px - self.drag_origin.0).round() as i32;
        let dy = (py - self.drag_origin.1).round() as i32;

        let mut x1 = orig.x;
        let mut y1 = orig.y;
        let mut x2 = orig.x + orig.width as i32;
        let mut y2 = orig.y + orig.height as i32;

        match handle {
            ResizeHandle::MoveInterior => {
                let max_x = (bounds_w as i32 - orig.width as i32).max(0);
                let max_y = (bounds_h as i32 - orig.height as i32).max(0);
                let nx = self.snap((orig.x + dx).clamp(0, max_x));
                let ny = self.snap((orig.y + dy).clamp(0, max_y));
                self.rect = Some(Rectangle::new(nx, ny, orig.width, orig.height));
                return;
            }
            ResizeHandle::TopLeft => {
                x1 += dx;
                y1 += dy;
            }
            ResizeHandle::Top => {
                y1 += dy;
            }
            ResizeHandle::TopRight => {
                x2 += dx;
                y1 += dy;
            }
            ResizeHandle::Right => {
                x2 += dx;
            }
            ResizeHandle::BottomRight => {
                x2 += dx;
                y2 += dy;
            }
            ResizeHandle::Bottom => {
                y2 += dy;
            }
            ResizeHandle::BottomLeft => {
                x1 += dx;
                y2 += dy;
            }
            ResizeHandle::Left => {
                x1 += dx;
            }
        }

        let min_x = x1.min(x2).clamp(0, bounds_w as i32);
        let max_x = x1.max(x2).clamp(0, bounds_w as i32);
        let min_y = y1.min(y2).clamp(0, bounds_h as i32);
        let max_y = y1.max(y2).clamp(0, bounds_h as i32);

        let mut rect = Rectangle::new(
            min_x,
            min_y,
            (max_x - min_x).max(16) as u32,
            (max_y - min_y).max(16) as u32,
        );
        if self.aspect_ratio != AspectRatio::Free {
            rect = rect.with_aspect_ratio(self.aspect_ratio);
        }
        self.rect = Some(rect.clamp_to_bounds(bounds_w as u32, bounds_h as u32));
    }

    pub fn end_drag(&mut self) {
        self.active_handle = None;
        self.rect_at_drag_start = None;
        self.drawing_new = false;
    }

    /// Nudge selection position with arrow keys (`shift` = 10px steps).
    pub fn nudge(&mut self, dx: i32, dy: i32, bounds_w: u32, bounds_h: u32) {
        if let Some(mut r) = self.rect {
            let max_x = bounds_w.saturating_sub(r.width) as i32;
            let max_y = bounds_h.saturating_sub(r.height) as i32;
            r.x = (r.x + dx).clamp(0, max_x);
            r.y = (r.y + dy).clamp(0, max_y);
            self.rect = Some(r);
        }
    }

    /// Apply a new aspect ratio immediately to the active rectangle.
    pub fn set_aspect_ratio(&mut self, ratio: AspectRatio, bounds_w: u32, bounds_h: u32) {
        self.aspect_ratio = ratio;
        if let Some(r) = self.rect {
            self.rect = Some(
                r.with_aspect_ratio(ratio)
                    .clamp_to_bounds(bounds_w, bounds_h),
            );
        }
    }

    fn snap(&self, val: i32) -> i32 {
        if self.snap_grid > 1 {
            let g = self.snap_grid as i32;
            ((val + g / 2) / g) * g
        } else {
            val
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_draw_and_resize_selection() {
        let mut ed = SelectionEditor::new(AspectRatio::Ratio16x9);
        ed.begin_drag(100.0, 100.0, 8.0);
        ed.update_drag(580.0, 300.0, 1920.0, 1080.0);
        ed.end_drag();

        let r = ed.rect.unwrap();
        assert_eq!(r.x, 100);
        assert_eq!(r.width, 480);
        assert_eq!(r.height, 270); // 480 / (16/9) = 270

        // Move interior
        assert_eq!(ed.hit_test(200.0, 200.0, 8.0), Some(ResizeHandle::MoveInterior));
        ed.nudge(20, 10, 1920, 1080);
        assert_eq!(ed.rect.unwrap().x, 120);
        assert_eq!(ed.rect.unwrap().y, 110);
    }
}
