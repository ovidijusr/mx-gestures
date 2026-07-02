# mx-gestures

Lightweight macOS daemon that turns the **Logitech MX Master 2S gesture (palm) button** into trackpad-style gestures — no Logi Options needed.

| Gesture (hold palm button +) | Default action |
|---|---|
| drag left | Space left (Ctrl+←) |
| drag right | Space right (Ctrl+→) |
| drag up | Mission Control (Ctrl+↑) |
| drag down | App Exposé (Ctrl+↓) |
| plain click | Mission Control |

## How it works

The gesture button is invisible to macOS as a normal button. This tool speaks Logitech's **HID++ 2.0** protocol to the Unifying receiver (vendor HID interface, usage page `0xFF00`) and *diverts* control `0x00C3` (feature `0x1B04`, Reprog Controls v4). While the button is held it also diverts raw XY motion, so the cursor freezes and the deltas come to us instead — same mechanism `logiops` uses on Linux. On release the accumulated movement is classified and a synthetic Ctrl+Arrow key event triggers the matching Mission Control action.

## Requirements

- MX Master (2S) connected via **Unifying USB receiver** (not plain Bluetooth)
- Logi Options / Options+ **not** running (it would fight over the button)
- The default Mission Control shortcuts (Ctrl+Arrows) enabled in System Settings (they are by default)

### Permissions (two one-time grants)

Key events are sent through Apple-signed System Events (`osascript`) because macOS's
hotkey handler silently ignores synthetic Ctrl+Arrow CGEvents from unsigned binaries —
verified empirically; direct posting at HID/Session tap with every flag combination
never triggers Mission Control, while the same keystroke via System Events does.

1. **Automation** — on first gesture macOS prompts "mx-gestures wants to control System
   Events"; click Allow.
2. **Accessibility** — System Settings → Privacy & Security → Accessibility → **+** →
   ⌘⇧G → path to the `mx-gestures` binary (without this, gestures fail with
   `osascript is not allowed to send keystrokes (1002)` in the log).

Note: TCC tracks the unsigned binary by path/hash — after a rebuild you may need to
re-toggle the Accessibility entry. `MXG_STRATEGY=0..3` selects direct CGEvent posting
instead (lower latency), which may work if you properly codesign the binary.

## Usage

```sh
cargo build --release
./target/release/mx-gestures --verbose   # foreground, prints every event
./target/release/mx-gestures install     # writes + starts a LaunchAgent (lt.ovi.mx-gestures)
./target/release/mx-gestures uninstall
```

## Config

`~/.config/mx-gestures/config.toml` (all optional):

```toml
tap_max_distance = 40     # raw counts; below this a press is a "tap"
axis_ratio = 1.2          # dominant axis must beat the other by this factor
invert_horizontal = false # true = natural-scrolling direction for spaces

swipe_left = "space_left"       # space_left | space_right | mission_control | app_expose | none
swipe_right = "space_right"
swipe_up = "mission_control"
swipe_down = "app_expose"
tap = "mission_control"
```

## Credits / prior art

Protocol behavior cross-referenced from [logiops](https://github.com/PixlOne/logiops) and [Solaar](https://github.com/pwr-Solaar/Solaar) (behavior only; no code copied).
