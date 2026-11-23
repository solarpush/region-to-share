//! Geometric types, aspect-ratio locking, and coordinate utilities.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// A 2D point in screen coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    #[inline]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub fn distance_from_origin(&self) -> f64 {
        let x = self.x as f64;
        let y = self.y as f64;
        (x * x + y * y).sqrt()
    }

    pub fn distance_to(&self, other: &Point) -> f64 {
        let dx = (self.x - other.x) as f64;
        let dy = (self.y - other.y) as f64;
        (dx * dx + dy * dy).sqrt()
    }
}

/// Common aspect ratios for region locking during selection or presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AspectRatio {
    #[default]
    Free,
    Ratio16x9,
    Ratio16x10,
    Ratio4x3,
    Ratio1x1,
    Ratio9x16,
    Ratio21x9,
}

impl AspectRatio {
    /// Returns the numeric ratio `width / height`, or `None` for `Free`.
    pub const fn value(self) -> Option<f32> {
        match self {
            Self::Free => None,
            Self::Ratio16x9 => Some(16.0 / 9.0),
            Self::Ratio16x10 => Some(16.0 / 10.0),
            Self::Ratio4x3 => Some(4.0 / 3.0),
            Self::Ratio1x1 => Some(1.0),
            Self::Ratio9x16 => Some(9.0 / 16.0),
            Self::Ratio21x9 => Some(21.0 / 9.0),
        }
    }

    /// Short UI label for the aspect ratio pill.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Free => "Free",
            Self::Ratio16x9 => "16:9",
            Self::Ratio16x10 => "16:10",
            Self::Ratio4x3 => "4:3",
            Self::Ratio1x1 => "1:1",
            Self::Ratio9x16 => "9:16",
            Self::Ratio21x9 => "21:9",
        }
    }

    pub const fn all() -> &'static [AspectRatio] {
        &[
            Self::Free,
            Self::Ratio16x9,
            Self::Ratio16x10,
            Self::Ratio4x3,
            Self::Ratio1x1,
            Self::Ratio9x16,
            Self::Ratio21x9,
        ]
    }
}

/// A rectangular region on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rectangle {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Default for Rectangle {
    fn default() -> Self {
        Self::new(0, 0, 1280, 720)
    }
}

impl Rectangle {
    /// Create a new rectangle.
    #[inline]
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Create a rectangle from two arbitrary opposite corners.
    pub fn from_points(p1: Point, p2: Point) -> Self {
        let x = p1.x.min(p2.x);
        let y = p1.y.min(p2.y);
        let width = (p1.x - p2.x).unsigned_abs();
        let height = (p1.y - p2.y).unsigned_abs();
        Self::new(x, y, width, height)
    }

    /// Area in square pixels.
    #[inline]
    pub const fn area(&self) -> u64 {
        self.width as u64 * self.height as u64
    }

    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    pub fn center(&self) -> Point {
        Point::new(
            self.x + (self.width / 2) as i32,
            self.y + (self.height / 2) as i32,
        )
    }

    pub fn contains_point(&self, point: &Point) -> bool {
        point.x >= self.x
            && point.x < self.x + self.width as i32
            && point.y >= self.y
            && point.y < self.y + self.height as i32
    }

    pub fn intersects(&self, other: &Rectangle) -> bool {
        self.x < other.x + other.width as i32
            && self.x + self.width as i32 > other.x
            && self.y < other.y + other.height as i32
            && self.y + self.height as i32 > other.y
    }

    pub fn intersection(&self, other: &Rectangle) -> Option<Rectangle> {
        if !self.intersects(other) {
            return None;
        }

        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = (self.x + self.width as i32).min(other.x + other.width as i32);
        let y2 = (self.y + self.height as i32).min(other.y + other.height as i32);

        Some(Rectangle::new(x1, y1, (x2 - x1) as u32, (y2 - y1) as u32))
    }

    /// Clamp rectangle into `[0..max_w, 0..max_h]` while guaranteeing >= 1x1 if bounds > 0.
    pub fn clamp_to_bounds(&self, max_w: u32, max_h: u32) -> Self {
        if max_w == 0 || max_h == 0 {
            return *self;
        }
        let x = self.x.clamp(0, max_w.saturating_sub(1) as i32);
        let y = self.y.clamp(0, max_h.saturating_sub(1) as i32);
        let avail_w = max_w.saturating_sub(x as u32).max(1);
        let avail_h = max_h.saturating_sub(y as u32).max(1);
        Self {
            x,
            y,
            width: self.width.clamp(1, avail_w),
            height: self.height.clamp(1, avail_h),
        }
    }

    /// Ensure width and height are even numbers (>= 2), required by YUV420p video encoders
    /// (H.264, HEVC, VA-API, NVENC, AV1).
    pub fn ensure_even_dimensions(&self) -> Self {
        let w = (self.width.max(2)) & !1;
        let h = (self.height.max(2)) & !1;
        Self {
            x: self.x,
            y: self.y,
            width: w.max(2),
            height: h.max(2),
        }
    }

    /// Adjust height to satisfy an aspect ratio (keeping x, y, width anchored).
    pub fn with_aspect_ratio(&self, ratio: AspectRatio) -> Self {
        match ratio.value() {
            None => *self,
            Some(r) => {
                let height = ((self.width as f32) / r).round().max(2.0) as u32;
                Self {
                    x: self.x,
                    y: self.y,
                    width: self.width.max(2),
                    height,
                }
            }
        }
    }

    /// Approximate display aspect ratio label (e.g. `"16:9"`, `"4:3"`, or `"1.78:1"`).
    pub fn aspect_label(&self) -> String {
        if self.height == 0 {
            return "—".to_string();
        }
        let r = self.width as f32 / self.height as f32;
        for preset in AspectRatio::all() {
            if let Some(val) = preset.value() {
                if (r - val).abs() < 0.02 {
                    return preset.label().to_string();
                }
            }
        }
        format!("{:.2}:1", r)
    }

    #[inline]
    pub const fn top_left(&self) -> Point {
        Point::new(self.x, self.y)
    }

    #[inline]
    pub const fn top_right(&self) -> Point {
        Point::new(self.x + self.width as i32, self.y)
    }

    #[inline]
    pub const fn bottom_left(&self) -> Point {
        Point::new(self.x, self.y + self.height as i32)
    }

    #[inline]
    pub const fn bottom_right(&self) -> Point {
        Point::new(self.x + self.width as i32, self.y + self.height as i32)
    }
}

impl fmt::Display for Rectangle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{},{},{}x{}", self.x, self.y, self.width, self.height)
    }
}

/// Parses `"X,Y,WxH"` or `"WxH+X+Y"` (X11 geometry style) or `"X,Y,W,H"`.
impl FromStr for Rectangle {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();

        // Format: WxH+X+Y (e.g. 1280x720+100+200)
        if let Some((wh, rest)) = s.split_once('+') {
            if let Some((w, h)) = wh.split_once(['x', 'X']) {
                let (x, y) = rest
                    .split_once('+')
                    .ok_or_else(|| format!("Invalid geometry '{s}': expected WxH+X+Y"))?;
                return Ok(Self::new(
                    x.parse().map_err(|_| format!("Invalid x in '{s}'"))?,
                    y.parse().map_err(|_| format!("Invalid y in '{s}'"))?,
                    w.parse().map_err(|_| format!("Invalid width in '{s}'"))?,
                    h.parse().map_err(|_| format!("Invalid height in '{s}'"))?,
                ));
            }
        }

        // Format: X,Y,WxH or X,Y,W,H
        let parts: Vec<&str> = s.split(',').collect();
        match parts.as_slice() {
            [x, y, wh] => {
                let (w, h) = wh
                    .split_once(['x', 'X'])
                    .ok_or_else(|| format!("Invalid WxH segment in '{s}'"))?;
                let width: u32 = w.parse().map_err(|_| format!("Invalid width in '{s}'"))?;
                let height: u32 = h.parse().map_err(|_| format!("Invalid height in '{s}'"))?;
                if width == 0 || height == 0 {
                    return Err("Width and height must be > 0".to_string());
                }
                Ok(Self::new(
                    x.parse().map_err(|_| format!("Invalid x in '{s}'"))?,
                    y.parse().map_err(|_| format!("Invalid y in '{s}'"))?,
                    width,
                    height,
                ))
            }
            [x, y, w, h] => {
                let width: u32 = w.parse().map_err(|_| format!("Invalid width in '{s}'"))?;
                let height: u32 = h.parse().map_err(|_| format!("Invalid height in '{s}'"))?;
                if width == 0 || height == 0 {
                    return Err("Width and height must be > 0".to_string());
                }
                Ok(Self::new(
                    x.parse().map_err(|_| format!("Invalid x in '{s}'"))?,
                    y.parse().map_err(|_| format!("Invalid y in '{s}'"))?,
                    width,
                    height,
                ))
            }
            _ => Err(format!(
                "Invalid region '{s}': expected 'X,Y,WxH', 'X,Y,W,H', or 'WxH+X+Y'"
            )),
        }
    }
}

/// Pixel format for frame buffers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum PixelFormat {
    /// 32-bit BGRA (Blue, Green, Red, Alpha)
    BGRA8888,
    /// 32-bit RGBA (Red, Green, Blue, Alpha)
    RGBA8888,
    /// 32-bit ARGB (Alpha, Red, Green, Blue)
    ARGB8888,
    /// 24-bit RGB (Red, Green, Blue)
    RGB888,
    /// 24-bit BGR (Blue, Green, Red)
    BGR888,
}

impl PixelFormat {
    pub const fn bytes_per_pixel(&self) -> usize {
        match self {
            PixelFormat::BGRA8888 | PixelFormat::RGBA8888 | PixelFormat::ARGB8888 => 4,
            PixelFormat::RGB888 | PixelFormat::BGR888 => 3,
        }
    }

    pub const fn has_alpha(&self) -> bool {
        matches!(
            self,
            PixelFormat::BGRA8888 | PixelFormat::RGBA8888 | PixelFormat::ARGB8888
        )
    }

    /// Corresponding FFmpeg `-pix_fmt` argument string.
    pub const fn ffmpeg_pix_fmt(&self) -> &'static str {
        match self {
            PixelFormat::BGRA8888 => "bgra",
            PixelFormat::RGBA8888 => "rgba",
            PixelFormat::ARGB8888 => "argb",
            PixelFormat::RGB888 => "rgb24",
            PixelFormat::BGR888 => "bgr24",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rectangle_parsing_variants() {
        assert_eq!(
            "100,200,1280x720".parse::<Rectangle>().unwrap(),
            Rectangle::new(100, 200, 1280, 720)
        );
        assert_eq!(
            "100,200,1280,720".parse::<Rectangle>().unwrap(),
            Rectangle::new(100, 200, 1280, 720)
        );
        assert_eq!(
            "1280x720+100+200".parse::<Rectangle>().unwrap(),
            Rectangle::new(100, 200, 1280, 720)
        );
        assert!("0,0,0x100".parse::<Rectangle>().is_err());
        assert!("invalid".parse::<Rectangle>().is_err());
    }

    #[test]
    fn test_rectangle_even_and_clamp() {
        let r = Rectangle::new(10, 20, 1281, 721);
        assert_eq!(r.ensure_even_dimensions(), Rectangle::new(10, 20, 1280, 720));

        let ultrawide = Rectangle::new(4000, 1000, 2000, 2000).clamp_to_bounds(5120, 2160);
        assert_eq!(ultrawide, Rectangle::new(4000, 1000, 1120, 1160));
    }

    #[test]
    fn test_aspect_ratio_helpers() {
        let r = Rectangle::new(0, 0, 1920, 1080);
        assert_eq!(r.aspect_label(), "16:9");

        let locked = Rectangle::new(0, 0, 1600, 500).with_aspect_ratio(AspectRatio::Ratio16x9);
        assert_eq!(locked.height, 900);
    }

    #[test]
    fn test_rectangle_display_roundtrip() {
        let r = Rectangle::new(50, 75, 1440, 900);
        assert_eq!(r.to_string().parse::<Rectangle>().unwrap(), r);
    }
}
