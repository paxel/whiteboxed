//! Plain 2D geometry in diagram pixels (y grows downward).

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Pos {
    pub x: f32,
    pub y: f32,
}

impl Pos {
    pub const fn new(x: f32, y: f32) -> Self {
        Pos { x, y }
    }

    pub fn dist(self, other: Pos) -> f32 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub min: Pos,
    pub max: Pos,
}

impl Rect {
    pub fn from_min_size(min: Pos, w: f32, h: f32) -> Self {
        Rect {
            min,
            max: Pos::new(min.x + w, min.y + h),
        }
    }

    pub fn from_center(c: Pos, w: f32, h: f32) -> Self {
        Rect::from_min_size(Pos::new(c.x - w / 2.0, c.y - h / 2.0), w, h)
    }

    pub fn width(&self) -> f32 {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f32 {
        self.max.y - self.min.y
    }

    pub fn center(&self) -> Pos {
        Pos::new(
            (self.min.x + self.max.x) / 2.0,
            (self.min.y + self.max.y) / 2.0,
        )
    }

    pub fn contains(&self, p: Pos) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }

    pub fn expand(&self, by: f32) -> Rect {
        Rect {
            min: Pos::new(self.min.x - by, self.min.y - by),
            max: Pos::new(self.max.x + by, self.max.y + by),
        }
    }
}

/// Distance from `p` to the segment `a`-`b`.
pub fn segment_dist(p: Pos, a: Pos, b: Pos) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    if len2 == 0.0 {
        return p.dist(a);
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0);
    p.dist(Pos::new(a.x + t * dx, a.y + t * dy))
}

pub fn polyline_dist(p: Pos, points: &[Pos]) -> f32 {
    points
        .windows(2)
        .map(|w| segment_dist(p, w[0], w[1]))
        .fold(f32::INFINITY, f32::min)
}
