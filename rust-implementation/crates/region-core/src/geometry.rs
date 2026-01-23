//! Geometric types for screen regions and coordinates.

use serde::{Deserialize, Serialize};

/// A point on the screen with x and y coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    /// Create a new point.
    #[inline]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Distance from origin.
    pub fn distance_from_origin(&self) -> f64 {
        ((self.x.pow(2) + self.y.pow(2)) as f64).sqrt()
    }

    /// Distance to another point.
    pub fn distance_to(&self, other: &Point) -> f64 {
        let dx = (self.x - other.x) as f64;
        let dy = (self.y - other.y) as f64;
        (dx * dx + dy * dy).sqrt()
    }
}

/// A rectangular region on the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rectangle {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
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

    /// Create a rectangle from two points (top-left and bottom-right).
    pub fn from_points(p1: Point, p2: Point) -> Self {
        let x = p1.x.min(p2.x);
        let y = p1.y.min(p2.y);
        let width = (p1.x - p2.x).unsigned_abs();
        let height = (p1.y - p2.y).unsigned_abs();
        Self::new(x, y, width, height)
    }

    /// Calculate the area of the rectangle.
    #[inline]
    pub const fn area(&self) -> u64 {
        self.width as u64 * self.height as u64
    }

    /// Get the center point of the rectangle.
    pub fn center(&self) -> Point {
        Point::new(
            self.x + (self.width / 2) as i32,
            self.y + (self.height / 2) as i32,
        )
    }

    /// Check if a point is inside the rectangle.
    pub fn contains_point(&self, point: &Point) -> bool {
        point.x >= self.x
            && point.x < self.x + self.width as i32
            && point.y >= self.y
            && point.y < self.y + self.height as i32
    }

    /// Check if this rectangle intersects with another.
    pub fn intersects(&self, other: &Rectangle) -> bool {
        self.x < other.x + other.width as i32
            && self.x + self.width as i32 > other.x
            && self.y < other.y + other.height as i32
            && self.y + self.height as i32 > other.y
    }

    /// Get the intersection of two rectangles, if any.
    pub fn intersection(&self, other: &Rectangle) -> Option<Rectangle> {
        if !self.intersects(other) {
            return None;
        }

        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = (self.x + self.width as i32).min(other.x + other.width as i32);
        let y2 = (self.y + self.height as i32).min(other.y + other.height as i32);

        Some(Rectangle::new(
            x1,
            y1,
            (x2 - x1) as u32,
            (y2 - y1) as u32,
        ))
    }

    /// Get the top-left point.
    #[inline]
    pub const fn top_left(&self) -> Point {
        Point::new(self.x, self.y)
    }

    /// Get the top-right point.
    #[inline]
    pub const fn top_right(&self) -> Point {
        Point::new(self.x + self.width as i32, self.y)
    }

    /// Get the bottom-left point.
    #[inline]
    pub const fn bottom_left(&self) -> Point {
        Point::new(self.x, self.y + self.height as i32)
    }

    /// Get the bottom-right point.
    #[inline]
    pub const fn bottom_right(&self) -> Point {
        Point::new(
            self.x + self.width as i32,
            self.y + self.height as i32,
        )
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
    /// Get the number of bytes per pixel.
    pub const fn bytes_per_pixel(&self) -> usize {
        match self {
            PixelFormat::BGRA8888 | PixelFormat::RGBA8888 | PixelFormat::ARGB8888 => 4,
            PixelFormat::RGB888 | PixelFormat::BGR888 => 3,
        }
    }

    /// Check if the format has an alpha channel.
    pub const fn has_alpha(&self) -> bool {
        matches!(
            self,
            PixelFormat::BGRA8888 | PixelFormat::RGBA8888 | PixelFormat::ARGB8888
        )
    }

    /// Get the DRM fourcc code for this format (used for DMA-BUF).
    #[cfg(feature = "drm")]
    pub const fn drm_fourcc(&self) -> u32 {
        match self {
            PixelFormat::BGRA8888 => drm_fourcc::DRM_FORMAT_ARGB8888,
            PixelFormat::RGBA8888 => drm_fourcc::DRM_FORMAT_ABGR8888,
            PixelFormat::ARGB8888 => drm_fourcc::DRM_FORMAT_BGRA8888,
            PixelFormat::RGB888 => drm_fourcc::DRM_FORMAT_RGB888,
            PixelFormat::BGR888 => drm_fourcc::DRM_FORMAT_BGR888,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_creation() {
        let p = Point::new(10, 20);
        assert_eq!(p.x, 10);
        assert_eq!(p.y, 20);
    }

    #[test]
    fn test_point_distance() {
        let p1 = Point::new(0, 0);
        let p2 = Point::new(3, 4);
        assert_eq!(p2.distance_from_origin(), 5.0);
        assert_eq!(p1.distance_to(&p2), 5.0);
    }

    #[test]
    fn test_rectangle_area() {
        let rect = Rectangle::new(0, 0, 100, 50);
        assert_eq!(rect.area(), 5000);
    }

    #[test]
    fn test_rectangle_from_points() {
        let p1 = Point::new(10, 20);
        let p2 = Point::new(50, 80);
        let rect = Rectangle::from_points(p1, p2);
        assert_eq!(rect.x, 10);
        assert_eq!(rect.y, 20);
        assert_eq!(rect.width, 40);
        assert_eq!(rect.height, 60);
    }

    #[test]
    fn test_rectangle_center() {
        let rect = Rectangle::new(10, 20, 100, 50);
        let center = rect.center();
        assert_eq!(center.x, 60);
        assert_eq!(center.y, 45);
    }

    #[test]
    fn test_rectangle_contains_point() {
        let rect = Rectangle::new(10, 10, 100, 100);
        assert!(rect.contains_point(&Point::new(50, 50)));
        assert!(rect.contains_point(&Point::new(10, 10)));
        assert!(!rect.contains_point(&Point::new(110, 110)));
        assert!(!rect.contains_point(&Point::new(5, 50)));
    }

    #[test]
    fn test_rectangle_intersects() {
        let rect1 = Rectangle::new(0, 0, 100, 100);
        let rect2 = Rectangle::new(50, 50, 100, 100);
        let rect3 = Rectangle::new(200, 200, 100, 100);

        assert!(rect1.intersects(&rect2));
        assert!(rect2.intersects(&rect1));
        assert!(!rect1.intersects(&rect3));
    }

    #[test]
    fn test_rectangle_intersection() {
        let rect1 = Rectangle::new(0, 0, 100, 100);
        let rect2 = Rectangle::new(50, 50, 100, 100);
        let intersection = rect1.intersection(&rect2).unwrap();

        assert_eq!(intersection.x, 50);
        assert_eq!(intersection.y, 50);
        assert_eq!(intersection.width, 50);
        assert_eq!(intersection.height, 50);

        let rect3 = Rectangle::new(200, 200, 100, 100);
        assert!(rect1.intersection(&rect3).is_none());
    }

    #[test]
    fn test_rectangle_corners() {
        let rect = Rectangle::new(10, 20, 100, 50);
        assert_eq!(rect.top_left(), Point::new(10, 20));
        assert_eq!(rect.top_right(), Point::new(110, 20));
        assert_eq!(rect.bottom_left(), Point::new(10, 70));
        assert_eq!(rect.bottom_right(), Point::new(110, 70));
    }

    #[test]
    fn test_pixel_format_bytes() {
        assert_eq!(PixelFormat::BGRA8888.bytes_per_pixel(), 4);
        assert_eq!(PixelFormat::RGBA8888.bytes_per_pixel(), 4);
        assert_eq!(PixelFormat::ARGB8888.bytes_per_pixel(), 4);
        assert_eq!(PixelFormat::RGB888.bytes_per_pixel(), 3);
        assert_eq!(PixelFormat::BGR888.bytes_per_pixel(), 3);
    }

    #[test]
    fn test_pixel_format_alpha() {
        assert!(PixelFormat::BGRA8888.has_alpha());
        assert!(PixelFormat::RGBA8888.has_alpha());
        assert!(PixelFormat::ARGB8888.has_alpha());
        assert!(!PixelFormat::RGB888.has_alpha());
        assert!(!PixelFormat::BGR888.has_alpha());
    }
}
