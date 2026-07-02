//! Menu-bar UI: monochrome mouse glyph with a status/control menu.
//! Runs on the main thread (macOS requirement); the gesture engine runs in a
//! background thread started by the caller.

use tao::event::Event;
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

use crate::actions;
use crate::install;

enum UserEvent {
    Menu(MenuEvent),
}

/// If running from Downloads/Desktop/a DMG, offer to move the bundle to
/// /Applications (the login item and TCC grants want a stable path).
/// Same UX as the LetsMove pattern; on move, relaunches from the new home.
pub fn offer_move_to_applications() {
    let Ok(exe) = std::env::current_exe() else { return };
    // .../MX Gestures.app/Contents/MacOS/mx-gestures → the .app root
    let Some(app) = exe.ancestors().nth(3).map(std::path::Path::to_path_buf) else { return };
    if app.extension().and_then(|e| e.to_str()) != Some("app")
        || app.starts_with("/Applications")
    {
        return; // not bundled, or already home
    }
    let dest = std::path::Path::new("/Applications").join(app.file_name().unwrap());
    let script = "display dialog \"Move MX Gestures to the Applications folder? It needs a permanent home for login-item and permission tracking.\" buttons {\"Not Now\", \"Move\"} default button \"Move\" with icon note";
    let ok = std::process::Command::new("osascript")
        .args(["-e", script])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("Move"))
        .unwrap_or(false);
    if !ok {
        return;
    }
    let _ = std::fs::remove_dir_all(&dest);
    let copied = std::process::Command::new("ditto")
        .arg(&app)
        .arg(&dest)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if copied {
        let _ = std::process::Command::new("open").arg(&dest).spawn();
        // Old copy removes itself best-effort; ignore failure (e.g. on a DMG).
        let _ = std::fs::remove_dir_all(&app);
        std::process::exit(0);
    }
}

pub fn run_app() -> ! {
    offer_move_to_applications();
    let mut event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    // Accessory = no Dock icon, menu-bar only.
    event_loop.set_activation_policy(ActivationPolicy::Accessory);

    let proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |e| {
        let _ = proxy.send_event(UserEvent::Menu(e));
    }));

    let ax_status = MenuItem::new(ax_label(), true, None);
    let login_toggle =
        CheckMenuItem::new("Start at login", true, install::is_installed(), None);
    let quit = MenuItem::new("Quit mx-gestures", true, None);

    let menu = Menu::new();
    let _ = menu.append_items(&[
        &ax_status,
        &PredefinedMenuItem::separator(),
        &login_toggle,
        &PredefinedMenuItem::separator(),
        &quit,
    ]);

    let mut tray: Option<TrayIcon> = None;

    event_loop.run(move |event, _, control_flow| {
        // Refresh the accessibility line periodically; menus open natively on
        // macOS so there is no reliable "menu will open" hook.
        *control_flow = ControlFlow::WaitUntil(
            std::time::Instant::now() + std::time::Duration::from_secs(3),
        );

        match event {
            Event::NewEvents(tao::event::StartCause::Init) => {
                // Tray must be built after the event loop starts on macOS.
                tray = Some(
                    TrayIconBuilder::new()
                        .with_icon(mouse_icon())
                        .with_icon_as_template(true)
                        .with_tooltip("mx-gestures")
                        .with_menu(Box::new(menu.clone()))
                        .build()
                        .expect("failed to create tray icon"),
                );
            }
            Event::NewEvents(_) => {
                ax_status.set_text(ax_label());
                login_toggle.set_checked(install::is_installed());
            }
            Event::UserEvent(UserEvent::Menu(e)) => {
                if e.id == ax_status.id() {
                    if !actions::accessibility_trusted() {
                        // Registers us in the Accessibility list + system dialog.
                        actions::request_accessibility();
                        let _ = std::process::Command::new("open")
                            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
                            .spawn();
                    }
                } else if e.id == login_toggle.id() {
                    // CheckMenuItem flips its state on click; make reality match.
                    if install::is_installed() {
                        install::uninstall_quiet();
                    } else {
                        install::install_quiet();
                    }
                    login_toggle.set_checked(install::is_installed());
                } else if e.id == quit.id() {
                    crate::cleanup::run_now();
                    drop(tray.take());
                    std::process::exit(0);
                }
            }
            _ => {}
        }
    })
}

fn ax_label() -> String {
    if actions::accessibility_trusted() {
        "✓ Accessibility granted".into()
    } else {
        "⚠ Accessibility missing — click to open settings".into()
    }
}

/// 32×32 monochrome mouse glyph (capsule body outline + scroll line),
/// rendered as a template image so it adapts to light/dark menu bars.
fn mouse_icon() -> tray_icon::Icon {
    const S: usize = 32;
    let mut rgba = vec![0u8; S * S * 4];
    let (cx, cy) = (16.0f32, 16.5f32);
    let (w, h, r) = (15.0f32, 26.0f32, 7.0f32); // body box + corner radius
    for y in 0..S {
        for x in 0..S {
            let px = x as f32 + 0.5 - cx;
            let py = y as f32 + 0.5 - cy;
            // signed distance to rounded rectangle
            let qx = px.abs() - (w / 2.0 - r);
            let qy = py.abs() - (h / 2.0 - r);
            let sd = (qx.max(0.0).hypot(qy.max(0.0))) + qx.max(qy).min(0.0) - r;
            let mut a = (1.7 - sd.abs()).clamp(0.0, 1.0); // outline
            // scroll wheel: short vertical bar in the upper body
            if px.abs() < 1.1 && (-9.0..=-3.5).contains(&py) {
                a = 1.0;
            }
            let i = (y * S + x) * 4;
            rgba[i + 3] = (a * 255.0) as u8; // black + alpha
        }
    }
    tray_icon::Icon::from_rgba(rgba, S as u32, S as u32).expect("icon")
}
