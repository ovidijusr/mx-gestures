mod actions;
mod config;
mod gesture;
mod hidpp;

use config::Config;
use gesture::Tracker;
use hidpp::{CID_GESTURE, Receiver};
use std::thread;
use std::time::Duration;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let verbose = args.iter().any(|a| a == "--verbose" || a == "-v");

    match args.first().map(String::as_str) {
        Some("install") => return install::install(),
        Some("uninstall") => return install::uninstall(),
        Some("reset") => {
            // Clear divert + rawXY so the button returns to stock behavior.
            match reset(verbose) {
                Ok(()) => println!("gesture button restored to stock behavior"),
                Err(e) => {
                    eprintln!("reset failed: {e}");
                    std::process::exit(1);
                }
            }
            return;
        }
        // Post one action's key events directly (debugging aid), e.g.
        //   mx-gestures fire mission_control
        Some("fire") => {
            let action = match args.get(1).map(String::as_str) {
                Some("space_left") => config::Action::SpaceLeft,
                Some("space_right") => config::Action::SpaceRight,
                Some("mission_control") => config::Action::MissionControl,
                Some("app_expose") => config::Action::AppExpose,
                other => {
                    eprintln!(
                        "usage: mx-gestures fire <space_left|space_right|mission_control|app_expose> (got {other:?})"
                    );
                    std::process::exit(2);
                }
            };
            match args.get(2).and_then(|s| s.parse::<u8>().ok()) {
                Some(s) => actions::perform_with_strategy(action, s),
                None => actions::perform(action),
            }
            return;
        }
        Some("--verbose") | Some("-v") | None => {}
        Some(other) => {
            eprintln!("usage: mx-gestures [--verbose|install|uninstall]");
            eprintln!("unknown argument: {other}");
            std::process::exit(2);
        }
    }

    let cfg = Config::load();
    if verbose {
        eprintln!("[mx-gestures] config: {cfg:?}");
    }
    if !actions::accessibility_trusted() {
        eprintln!(
            "[mx-gestures] WARNING: Accessibility permission not granted — gestures will be \
             detected but actions won't fire.\n  Grant it in System Settings → Privacy & \
             Security → Accessibility."
        );
    }

    // Outer resilience loop: receiver unplugged / mouse asleep / transient errors.
    loop {
        match run(&cfg, verbose) {
            Ok(()) => {}
            Err(e) => {
                if verbose {
                    eprintln!("[mx-gestures] {e}; retrying in 3s");
                }
            }
        }
        thread::sleep(Duration::from_secs(3));
    }
}

/// Un-divert the gesture button (both button and rawXY flags cleared).
fn reset(verbose: bool) -> hidpp::Result<()> {
    let api = hidapi::HidApi::new()?;
    let mut rx = Receiver::open(&api, verbose)?;
    for idx in 1..=6u8 {
        let reprog = match rx.get_feature_index(idx, hidpp::FEAT_REPROG_CONTROLS_V4) {
            Ok(fi) if fi != 0 => fi,
            _ => continue,
        };
        if rx.dump_controls(idx, reprog)?.contains(&CID_GESTURE) {
            rx.set_cid_reporting(
                idx,
                reprog,
                CID_GESTURE,
                hidpp::FLAG_DIVERT_VALID | hidpp::FLAG_RAW_XY_VALID,
            )?;
            return Ok(());
        }
    }
    Err(hidpp::Error::NotFound("device with gesture button"))
}

/// Find the mouse, divert the gesture button, pump events. Returns Err on
/// device loss so the outer loop can re-discover.
fn run(cfg: &Config, verbose: bool) -> hidpp::Result<()> {
    let api = hidapi::HidApi::new()?;
    let mut rx = Receiver::open(&api, verbose)?;

    // Discover the paired device that has the gesture button.
    let mut found: Option<(u8, u8)> = None; // (device_idx, reprog_feat_idx)
    for idx in 1..=6u8 {
        let reprog = match rx.get_feature_index(idx, hidpp::FEAT_REPROG_CONTROLS_V4) {
            Ok(fi) if fi != 0 => fi,
            _ => continue,
        };
        let cids = rx.dump_controls(idx, reprog)?;
        if cids.contains(&CID_GESTURE) {
            if verbose {
                let name = rx.device_name(idx).unwrap_or_default();
                eprintln!("[mx-gestures] device {idx}: {name} (reprog feature idx {reprog})");
            }
            found = Some((idx, reprog));
            break;
        }
    }
    let Some((dev, reprog)) = found else {
        return Err(hidpp::Error::NotFound(
            "paired device with a gesture button (mouse asleep or not paired?)",
        ));
    };

    // divert = button events come to us; rawXY = while the button is held the
    // device reroutes raw pointer motion to us (firmware gates it per-hold).
    let divert = |rx: &mut Receiver| {
        rx.set_cid_reporting(
            dev,
            reprog,
            CID_GESTURE,
            hidpp::FLAG_DIVERT
                | hidpp::FLAG_DIVERT_VALID
                | hidpp::FLAG_RAW_XY
                | hidpp::FLAG_RAW_XY_VALID,
        )
    };
    divert(&mut rx)?;
    eprintln!("[mx-gestures] gesture button diverted; ready");

    // Un-divert on SIGINT/SIGTERM so a stopped daemon never leaves the
    // button diverted with nobody listening.
    cleanup::arm(move || {
        if let Ok(api) = hidapi::HidApi::new() {
            if let Ok(mut rx) = Receiver::open(&api, false) {
                let _ = rx.set_cid_reporting(
                    dev,
                    reprog,
                    CID_GESTURE,
                    hidpp::FLAG_DIVERT_VALID | hidpp::FLAG_RAW_XY_VALID,
                );
            }
        }
    });

    let mut tracker = Tracker::default();
    loop {
        let Some(ev) = rx.next_event(500)? else { continue };
        if ev.device_idx != dev {
            continue;
        }
        // Receiver-level "device connection" notification (0x41): fires on
        // wake/reconnect, which resets diversion state on the device.
        if ev.feat_idx == 0x41 {
            if verbose {
                eprintln!("[mx-gestures] device reconnected; re-diverting");
            }
            // Give the link a moment before talking to the device.
            thread::sleep(Duration::from_millis(200));
            divert(&mut rx)?;
            continue;
        }
        if ev.feat_idx != reprog {
            continue;
        }
        match ev.event_id {
            // divertedButtonsEvent: params hold up to 4 currently-pressed CIDs.
            0x0 => {
                let pressed = ev
                    .params
                    .chunks(2)
                    .take(4)
                    .any(|c| u16::from_be_bytes([c[0], c[1]]) == CID_GESTURE);
                if pressed && !tracker.held {
                    tracker.press();
                    if verbose {
                        eprintln!("[gesture] press");
                    }
                } else if !pressed && tracker.held {
                    let g = tracker.release(cfg);
                    let action = actions::resolve(g, cfg);
                    if verbose {
                        eprintln!("[gesture] release → {g:?} → {action:?}");
                    }
                    actions::perform(action);
                }
            }
            // divertedRawMouseXYEvent: dx, dy as big-endian i16.
            0x1 => {
                let dx = i16::from_be_bytes([ev.params[0], ev.params[1]]);
                let dy = i16::from_be_bytes([ev.params[2], ev.params[3]]);
                tracker.motion(dx, dy);
                if verbose {
                    eprintln!("[gesture] motion dx={dx} dy={dy}");
                }
            }
            _ => {}
        }
    }
}

mod cleanup {
    use std::sync::Mutex;

    static HANDLER: Mutex<Option<Box<dyn FnOnce() + Send>>> = Mutex::new(None);

    /// Run `f` on SIGINT/SIGTERM, then exit. Re-arming replaces the handler.
    pub fn arm<F: FnOnce() + Send + 'static>(f: F) {
        *HANDLER.lock().unwrap() = Some(Box::new(f));
        unsafe {
            signal(2, handle as usize); // SIGINT
            signal(15, handle as usize); // SIGTERM
        }
    }

    extern "C" fn handle(_sig: i32) {
        // Not async-signal-safe, but we're exiting anyway — worst case the
        // un-divert write fails and a power cycle of the mouse clears it.
        if let Some(f) = HANDLER.lock().unwrap().take() {
            f();
        }
        std::process::exit(0);
    }

    unsafe extern "C" {
        fn signal(signum: i32, handler: usize) -> usize;
    }
}

mod install {
    use std::path::PathBuf;
    use std::process::Command;

    const LABEL: &str = "lt.ovi.mx-gestures";

    fn plist_path() -> PathBuf {
        let home = std::env::var("HOME").expect("HOME not set");
        PathBuf::from(home).join(format!("Library/LaunchAgents/{LABEL}.plist"))
    }

    pub fn install() {
        let exe = std::env::current_exe()
            .expect("cannot resolve own path")
            .canonicalize()
            .expect("cannot canonicalize own path");
        let log = "/tmp/mx-gestures.log";
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array><string>{}</string></array>
    <key>RunAtLoad</key><true/>
    <key>KeepAlive</key><true/>
    <key>StandardOutPath</key><string>{log}</string>
    <key>StandardErrorPath</key><string>{log}</string>
</dict>
</plist>
"#,
            exe.display()
        );
        let path = plist_path();
        std::fs::write(&path, plist).expect("failed to write LaunchAgent plist");
        // bootout first so re-install picks up a changed binary path
        let _ = Command::new("launchctl")
            .args(["bootout", &format!("gui/{}", uid()), path.to_str().unwrap()])
            .output();
        let st = Command::new("launchctl")
            .args(["bootstrap", &format!("gui/{}", uid()), path.to_str().unwrap()])
            .status()
            .expect("failed to run launchctl");
        if st.success() {
            println!("installed and started {LABEL}");
            println!("  binary: {}", exe.display());
            println!("  logs:   {log}");
            println!("If actions don't fire, add the binary to System Settings → Privacy & Security → Accessibility.");
        } else {
            eprintln!("launchctl bootstrap failed (status {st})");
            std::process::exit(1);
        }
    }

    pub fn uninstall() {
        let path = plist_path();
        let _ = Command::new("launchctl")
            .args(["bootout", &format!("gui/{}", uid()), path.to_str().unwrap()])
            .output();
        match std::fs::remove_file(&path) {
            Ok(()) => println!("uninstalled {LABEL}"),
            Err(e) => eprintln!("could not remove {}: {e}", path.display()),
        }
    }

    fn uid() -> u32 {
        unsafe { libc_getuid() }
    }
    unsafe extern "C" {
        #[link_name = "getuid"]
        fn libc_getuid() -> u32;
    }
}
