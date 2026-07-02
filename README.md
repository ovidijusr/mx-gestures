# MX Gestures

Menu-bar app that turns the **Logitech MX Master gesture (palm) button** into macOS trackpad-style gestures — no Logi Options needed. Lightweight Rust, one ~500 KB binary.

| Hold palm button + | Action (configurable) |
|---|---|
| drag left / right | switch Spaces |
| drag up | Mission Control |
| drag down | App Exposé |
| plain click | Mission Control |

The cursor freezes while the button is held — that's the gesture in progress. A monochrome mouse icon in the menu bar shows permission status and lets you toggle *Start at login* or quit.

Tested on an MX Master 2S over a Unifying receiver; other MX Master models advertising the same HID++ gesture control (CID `0x00C3`) should work.

## Install

### From a release

1. Download the `.app` zip from [Releases](../../releases), unzip, move **MX Gestures.app** to `/Applications`.
2. The app isn't notarized, so the first launch needs one of:
   - right-click the app → **Open** → **Open**, or
   - `xattr -d com.apple.quarantine "/Applications/MX Gestures.app"`
3. Launch it. A mouse icon appears in the menu bar and macOS asks for **Accessibility** — the app registers itself in the list, just flip the toggle on (System Settings → Privacy & Security → Accessibility).
4. On the first gesture, approve the **Automation** ("control System Events") prompt.
5. Optional: click the menu-bar icon → check **Start at login**.

### From source

```sh
git clone https://github.com/ovidijusr/mx-gestures && cd mx-gestures
./make-app.sh        # builds, bundles and signs /Applications/MX Gestures.app
open "/Applications/MX Gestures.app"
```

`make-app.sh` signs with your first Apple codesigning identity if you have one (permissions then survive rebuilds), otherwise ad-hoc. Override with `MXG_SIGN_IDENTITY="Developer ID Application: ..."`.

## Requirements

- MX Master mouse connected via **Logitech Unifying USB receiver** (plain Bluetooth won't work — macOS owns that connection and blocks HID++ from userspace)
- Logi Options / Options+ **not** running (it fights over the button)
- Default Mission Control shortcuts (Ctrl+Arrows) enabled in System Settings (they are by default)

## Configuration

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

## CLI

The bundled binary is also a CLI (`/Applications/MX Gestures.app/Contents/MacOS/mx-gestures`):

```
mx-gestures              # menu-bar app + gesture engine (what the bundle runs)
mx-gestures --headless   # engine only, no menu-bar icon
mx-gestures --verbose    # log every HID++ packet and gesture
mx-gestures install      # write + start the launchd agent (menu toggle does this too)
mx-gestures uninstall    # remove the launchd agent
mx-gestures reset        # restore the gesture button to stock behavior
mx-gestures fire <a>     # post one action directly (debugging)
```

Logs: `/tmp/mx-gestures.log`. If the button ever feels stuck, `mx-gestures reset` or just power-cycle the mouse.

## How it works

The gesture button is invisible to macOS as a normal button. The app speaks Logitech's **HID++ 2.0** protocol to the Unifying receiver (vendor HID interface, usage page `0xFF00`) and *diverts* control `0x00C3` with the rawXY flag (feature `0x1B04`, Reprog Controls v4). While the button is held the mouse reroutes raw motion to the app instead of the cursor; on release the accumulated deltas are classified and the matching Mission Control shortcut is sent.

Keystrokes go through Apple-signed System Events (`osascript`) because macOS's hotkey handler silently ignores synthetic Ctrl+Arrow CGEvents from non-notarized binaries — verified empirically; direct CGEvent posting at any tap location/flag combination never triggers Mission Control. `MXG_STRATEGY=0..3` selects the direct CGEvent variants if your build is signed in a way macOS accepts.

Protocol behavior cross-referenced from [logiops](https://github.com/PixlOne/logiops) and [Solaar](https://github.com/pwr-Solaar/Solaar) (behavior only; no code copied).

## License

MIT
