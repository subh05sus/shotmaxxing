//! Points, sizes and rectangles in capture space (logical points, origin top-left).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::ops::{Add, Mul, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance(self, other: Self) -> f64 {
        (other.x - self.x).hypot(other.y - self.y)
    }

    pub fn lerp(self, other: Self, t: f64) -> Self {
        Self::new(self.x + (other.x - self.x) * t, self.y + (other.y - self.y) * t)
    }

    pub fn length(self) -> f64 {
        self.x.hypot(self.y)
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

impl Add for Point {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Point {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Mul<f64> for Point {
    type Output = Self;
    fn mul(self, rhs: f64) -> Self {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

impl Size {
    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }

    pub fn is_empty(self) -> bool {
        !(self.width > 0.0 && self.height > 0.0)
    }

    pub fn aspect(self) -> f64 {
        if self.height > 0.0 { self.width / self.height } else { 0.0 }
    }

    pub fn center(self) -> Point {
        Point::new(self.width / 2.0, self.height / 2.0)
    }

    pub fn bounds(self) -> Rect {
        Rect::new(0.0, 0.0, self.width, self.height)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self { x, y, width, height }
    }

    pub fn from_center(center: Point, size: Size) -> Self {
        Self::new(center.x - size.width / 2.0, center.y - size.height / 2.0, size.width, size.height)
    }

    /// A box of the given size centred on a point.
    pub fn around(center: Point, width: f64, height: f64) -> Self {
        Self::from_center(center, Size::new(width, height))
    }

    pub fn min_x(self) -> f64 {
        self.x
    }
    pub fn min_y(self) -> f64 {
        self.y
    }
    pub fn max_x(self) -> f64 {
        self.x + self.width
    }
    pub fn max_y(self) -> f64 {
        self.y + self.height
    }

    pub fn center(self) -> Point {
        Point::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    pub fn size(self) -> Size {
        Size::new(self.width, self.height)
    }

    pub fn area(self) -> f64 {
        self.width.max(0.0) * self.height.max(0.0)
    }

    pub fn is_empty(self) -> bool {
        self.size().is_empty()
    }

    pub fn contains(self, p: Point) -> bool {
        p.x >= self.x && p.x <= self.max_x() && p.y >= self.y && p.y <= self.max_y()
    }

    pub fn inflate(self, dx: f64, dy: f64) -> Self {
        Self::new(self.x - dx, self.y - dy, self.width + 2.0 * dx, self.height + 2.0 * dy)
    }

    pub fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Self::new(x, y, self.max_x().max(other.max_x()) - x, self.max_y().max(other.max_y()) - y)
    }

    pub fn intersection(self, other: Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let w = self.max_x().min(other.max_x()) - x;
        let h = self.max_y().min(other.max_y()) - y;
        (w > 0.0 && h > 0.0).then(|| Self::new(x, y, w, h))
    }

    /// Fraction of `self` covered by `other`, from 0 to 1.
    pub fn coverage_by(self, other: Self) -> f64 {
        let area = self.area();
        if area <= 0.0 {
            return 0.0;
        }
        self.intersection(other).map_or(0.0, |i| i.area() / area)
    }

    /// Moves the rectangle so it sits inside `bounds`. A rectangle larger than `bounds` on an axis is centred on it.
    pub fn clamp_inside(self, bounds: Self) -> Self {
        let clamp_axis = |pos: f64, len: f64, min: f64, max_len: f64| {
            if len >= max_len { min + (max_len - len) / 2.0 } else { pos.clamp(min, min + max_len - len) }
        };
        Self::new(
            clamp_axis(self.x, self.width, bounds.x, bounds.width),
            clamp_axis(self.y, self.height, bounds.y, bounds.height),
            self.width,
            self.height,
        )
    }

    /// Grows the rectangle around its centre until it has the given aspect ratio (width / height).
    pub fn expand_to_aspect(self, aspect: f64) -> Self {
        if aspect <= 0.0 || self.is_empty() {
            return self;
        }
        let current = self.width / self.height;
        let size = if current < aspect {
            Size::new(self.height * aspect, self.height)
        } else {
            Size::new(self.width, self.width / aspect)
        };
        Self::from_center(self.center(), size)
    }

    /// Rounds the rectangle to whole points, used for stable fingerprints.
    pub fn rounded(self) -> Self {
        Self::new(self.x.round(), self.y.round(), self.width.round(), self.height.round())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn union_and_intersection() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, 5.0, 10.0, 10.0);
        assert_eq!(a.union(b), Rect::new(0.0, 0.0, 15.0, 15.0));
        assert_eq!(a.intersection(b), Some(Rect::new(5.0, 5.0, 5.0, 5.0)));
        assert_eq!(a.intersection(Rect::new(20.0, 20.0, 1.0, 1.0)), None);
        assert!((a.coverage_by(b) - 0.25).abs() < 1e-9);
    }

    #[test]
    fn clamp_inside_keeps_rect_in_bounds() {
        let bounds = Rect::new(0.0, 0.0, 100.0, 50.0);
        assert_eq!(Rect::new(-10.0, 45.0, 20.0, 20.0).clamp_inside(bounds), Rect::new(0.0, 30.0, 20.0, 20.0));
        // Larger than bounds: centred.
        let big = Rect::new(30.0, 0.0, 200.0, 10.0).clamp_inside(bounds);
        assert_eq!(big.x, -50.0);
    }

    #[test]
    fn expand_to_aspect_grows_the_short_side() {
        let r = Rect::new(0.0, 0.0, 100.0, 100.0).expand_to_aspect(16.0 / 9.0);
        assert!((r.width / r.height - 16.0 / 9.0).abs() < 1e-9);
        assert_eq!(r.center(), Point::new(50.0, 50.0));
        assert!(r.height >= 100.0 - 1e-9);
    }
}
