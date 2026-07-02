use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    SpaceLeft,
    SpaceRight,
    MissionControl,
    AppExpose,
    None,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Total displacement (raw counts) below which a press counts as a tap.
    pub tap_max_distance: i32,
    /// Dominant axis must exceed the other by this factor to count as a swipe.
    pub axis_ratio: f32,
    /// Swap left/right so moving the mouse right goes to the space on the left
    /// (matches "natural scrolling" trackpad direction).
    pub invert_horizontal: bool,
    pub swipe_left: Action,
    pub swipe_right: Action,
    pub swipe_up: Action,
    pub swipe_down: Action,
    pub tap: Action,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            tap_max_distance: 40,
            axis_ratio: 1.2,
            invert_horizontal: false,
            swipe_left: Action::SpaceLeft,
            swipe_right: Action::SpaceRight,
            swipe_up: Action::MissionControl,
            swipe_down: Action::AppExpose,
            tap: Action::MissionControl,
        }
    }
}

impl Config {
    pub fn path() -> std::path::PathBuf {
        let home = std::env::var("HOME").expect("HOME not set");
        std::path::Path::new(&home).join(".config/mx-gestures/config.toml")
    }

    pub fn load() -> Config {
        match std::fs::read_to_string(Self::path()) {
            Ok(s) => toml::from_str(&s).unwrap_or_else(|e| {
                eprintln!("[mx-gestures] bad config ({e}); using defaults");
                Config::default()
            }),
            Err(_) => Config::default(),
        }
    }
}
