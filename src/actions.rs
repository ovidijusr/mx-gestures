use crate::config::{Action, Config};
use crate::gesture::Gesture;
use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode, EventField};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

const KC_LEFT: CGKeyCode = 0x7B;
const KC_RIGHT: CGKeyCode = 0x7C;
const KC_DOWN: CGKeyCode = 0x7D;
const KC_UP: CGKeyCode = 0x7E;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

pub fn accessibility_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

pub fn resolve(gesture: Gesture, cfg: &Config) -> Action {
    let (l, r) = if cfg.invert_horizontal {
        (cfg.swipe_right, cfg.swipe_left)
    } else {
        (cfg.swipe_left, cfg.swipe_right)
    };
    match gesture {
        Gesture::Tap => cfg.tap,
        Gesture::Left => l,
        Gesture::Right => r,
        Gesture::Up => cfg.swipe_up,
        Gesture::Down => cfg.swipe_down,
    }
}

pub fn perform(action: Action) {
    perform_with_strategy(action, default_strategy());
}

pub fn perform_with_strategy(action: Action, strategy: u8) {
    let key = match action {
        Action::SpaceLeft => KC_LEFT,
        Action::SpaceRight => KC_RIGHT,
        Action::MissionControl => KC_UP,
        Action::AppExpose => KC_DOWN,
        Action::None => return,
    };
    match strategy {
        1 => ctrl_key(key, CGEventTapLocation::Session, CGEventSourceStateID::HIDSystemState, false),
        2 => ctrl_key(key, CGEventTapLocation::HID, CGEventSourceStateID::CombinedSessionState, false),
        3 => ctrl_key(key, CGEventTapLocation::HID, CGEventSourceStateID::HIDSystemState, true),
        4 => ctrl_key_osascript(key),
        _ => ctrl_key(key, CGEventTapLocation::HID, CGEventSourceStateID::HIDSystemState, false),
    }
}

/// Default is the osascript backend (4): macOS's hotkey handler ignores
/// synthetic Ctrl+Arrow events from unsigned binaries, but accepts them from
/// Apple-signed System Events. MXG_STRATEGY env var overrides (1-3 = direct
/// CGEvent variants, useful if the binary is ever properly signed).
fn default_strategy() -> u8 {
    std::env::var("MXG_STRATEGY").ok().and_then(|s| s.parse().ok()).unwrap_or(4)
}

/// Fallback: let Apple-signed System Events post the keystroke.
fn ctrl_key_osascript(key: CGKeyCode) {
    let script = format!("tell application \"System Events\" to key code {key} using control down");
    let _ = std::process::Command::new("osascript").args(["-e", &script]).spawn();
}

const KC_CONTROL: CGKeyCode = 0x3B;

/// Post Ctrl+<key> as a real key sequence (Ctrl down → key down/up → Ctrl up).
/// These are the default Mission Control shortcuts. Posting the modifier as a
/// real key event (not just an event flag) is needed for the system shortcut
/// handler to reliably pick it up.
fn ctrl_key(
    key: CGKeyCode,
    tap: CGEventTapLocation,
    state: CGEventSourceStateID,
    set_kb_type: bool,
) {
    let Ok(src) = CGEventSource::new(state) else {
        eprintln!("[mx-gestures] failed to create CGEventSource");
        return;
    };
    let seq: [(CGKeyCode, bool, bool); 4] = [
        (KC_CONTROL, true, true),
        (key, true, true),
        (key, false, true),
        (KC_CONTROL, false, false),
    ];
    // Real left-Control presses carry the device-dependent bit 0x1 in
    // addition to the generic Control mask; the system hotkey matcher
    // (Mission Control shortcuts) ignores events without it.
    let ctrl_flags = CGEventFlags::from_bits_retain(
        CGEventFlags::CGEventFlagControl.bits() | 0x1,
    );
    for (kc, down, ctrl_held) in seq {
        let Ok(ev) = CGEvent::new_keyboard_event(src.clone(), kc, down) else {
            eprintln!("[mx-gestures] failed to create key event");
            return;
        };
        ev.set_flags(if ctrl_held {
            ctrl_flags
        } else {
            CGEventFlags::CGEventFlagNull
        });
        if set_kb_type {
            // Match the physical keyboard type; some hotkey handlers filter
            // synthetic events with the default type.
            ev.set_integer_value_field(EventField::KEYBOARD_EVENT_KEYBOARD_TYPE, 40);
        }
        ev.post(tap);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
