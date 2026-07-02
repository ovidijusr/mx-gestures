use crate::config::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gesture {
    Tap,
    Left,
    Right,
    Up,
    Down,
}

/// Accumulates raw XY deltas while the gesture button is held.
#[derive(Default)]
pub struct Tracker {
    pub held: bool,
    dx: i32,
    dy: i32,
}

impl Tracker {
    pub fn press(&mut self) {
        self.held = true;
        self.dx = 0;
        self.dy = 0;
    }

    pub fn motion(&mut self, dx: i16, dy: i16) {
        if self.held {
            self.dx += dx as i32;
            self.dy += dy as i32;
        }
    }

    /// Button released — classify what the hold amounted to.
    pub fn release(&mut self, cfg: &Config) -> Gesture {
        self.held = false;
        classify(self.dx, self.dy, cfg)
    }
}

/// Decide what a completed hold+move was.
///
/// dx: total horizontal raw counts (positive = right)
/// dy: total vertical raw counts (positive = down, HID convention)
///
/// TODO(user): implement the real classifier. Trade-offs to weigh:
///   - `cfg.tap_max_distance`: radius (of total displacement) under which this
///     is a Tap. Too small → shaky-hand taps become accidental swipes; too
///     large → short deliberate swipes get eaten as taps.
///   - `cfg.axis_ratio`: how dominant one axis must be (e.g. |dx| > ratio*|dy|)
///     to pick horizontal over vertical. 1.0 = whichever is bigger wins
///     (diagonals feel twitchy); higher = diagonal moves near the boundary
///     could classify "wrong" — you may want whichever-is-bigger as fallback
///     rather than returning Tap.
///   - Remember +dy is DOWN.
pub fn classify(dx: i32, dy: i32, cfg: &Config) -> Gesture {
    let (ax, ay) = (dx.abs(), dy.abs());
    if ax + ay < cfg.tap_max_distance {
        return Gesture::Tap;
    }
    // Dominant-axis pick; axis_ratio biases toward vertical so slightly
    // slanted horizontal drags don't accidentally trigger Mission Control.
    if ax as f32 > ay as f32 * cfg.axis_ratio {
        if dx < 0 { Gesture::Left } else { Gesture::Right }
    } else if dy < 0 {
        Gesture::Up // +dy is down in HID convention
    } else {
        Gesture::Down
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> Config {
        Config::default() // tap_max_distance: 40, axis_ratio: 1.2
    }

    #[test]
    fn tiny_wiggle_is_tap() {
        assert_eq!(classify(5, -8, &cfg()), Gesture::Tap);
        assert_eq!(classify(0, 0, &cfg()), Gesture::Tap);
    }

    #[test]
    fn clear_horizontal_swipes() {
        assert_eq!(classify(300, 20, &cfg()), Gesture::Right);
        assert_eq!(classify(-250, -30, &cfg()), Gesture::Left);
    }

    #[test]
    fn clear_vertical_swipes() {
        // +dy is DOWN in HID convention
        assert_eq!(classify(10, -200, &cfg()), Gesture::Up);
        assert_eq!(classify(-15, 180, &cfg()), Gesture::Down);
    }

    #[test]
    fn diagonal_still_resolves_to_something() {
        // A 45°-ish drag well past tap distance should NOT be swallowed as a tap.
        let g = classify(150, -140, &cfg());
        assert_ne!(g, Gesture::Tap);
    }
}
