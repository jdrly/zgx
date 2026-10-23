//! Checks the app's windows as AppKit keeps them (macOS), with events posted
//! to this process alone: the Window menu and the windows it lists, a second
//! window cascaded from the first, a frame saved under its autosave name,
//! and what ⌘` cycles through. ⌘` itself is a system hotkey AppKit hears
//! from the window server, not a key event the app could post itself: the
//! probe sends one anyway and prints whether the key window changed.
use zgui::compose::prelude::*;
use zgui_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = |title: &str| WindowOptions {
        title: title.into(),
        width: 420.,
        height: 200.,
        ..Default::default()
    };
    let second = options("Second");
    Application::new().window(options("First")).run(move |cx| {
        cx.render(div().p(20.).child(text("First")));
        cx.windows.open(second, |cx| {
            cx.render(div().p(20.).child(text("Second")));
        });
        #[cfg(target_os = "macos")]
        cx.tasks.spawn(async move {
            macos::run().await;
            std::process::exit(0);
        });
    })
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::{MainThreadMarker, rc::Retained};
    use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType, NSWindow};
    use objc2_foundation::{NSPoint, NSString};
    use std::time::Duration;

    async fn sleep(ms: u64) {
        zgui::timer::sleep(Duration::from_millis(ms)).await;
    }
    fn app() -> Retained<NSApplication> {
        NSApplication::sharedApplication(MainThreadMarker::new().unwrap())
    }
    fn window(title: &str) -> Option<Retained<NSWindow>> {
        app()
            .windows()
            .iter()
            .find(|window| window.isVisible() && window.title().to_string() == title)
    }
    fn key_title() -> String {
        app()
            .keyWindow()
            .map_or_else(|| "none".into(), |window| window.title().to_string())
    }
    /// ⌘` as the keyboard would send it, to the key window.
    fn command_backtick() {
        let Some(key) = app().keyWindow() else {
            return;
        };
        let event = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            NSEventType::KeyDown,
            NSPoint::new(0., 0.),
            NSEventModifierFlags::Command,
            0.,
            key.windowNumber(),
            None,
            &NSString::from_str("`"),
            &NSString::from_str("`"),
            false,
            50,
        )
        .unwrap();
        app().sendEvent(&event);
    }
    pub async fn run() {
        sleep(2000).await;
        let app = app();
        let menu = app.windowsMenu().map(|menu| {
            menu.itemArray()
                .iter()
                .map(|item| item.title().to_string())
                .collect::<Vec<_>>()
        });
        println!("WINDOW_MENU {menu:?}");
        let (first, second) = (window("First").unwrap(), window("Second").unwrap());
        let (a, b) = (first.frame(), second.frame());
        println!(
            "CASCADE dx={} dy={}",
            b.origin.x - a.origin.x,
            (a.origin.y + a.size.height) - (b.origin.y + b.size.height)
        );
        // Moved, its frame is saved under its name.
        first.setFrameAutosaveName(&NSString::from_str("zgui window_probe"));
        let _ = second;
        first.setFrameOrigin(NSPoint::new(a.origin.x + 40., a.origin.y - 40.));
        sleep(300).await;
        // SAFETY: NSUserDefaults' class method and string getter.
        let saved: Option<Retained<NSString>> = unsafe {
            let defaults: Retained<objc2::runtime::AnyObject> =
                objc2::msg_send![objc2::class!(NSUserDefaults), standardUserDefaults];
            objc2::msg_send![
                &*defaults,
                stringForKey: &*NSString::from_str("NSWindow Frame zgui window_probe")
            ]
        };
        let saved = saved.map(|frame| frame.to_string());
        println!("AUTOSAVE {saved:?}");
        // SAFETY: as above; the probe leaves no default behind.
        unsafe {
            let defaults: Retained<objc2::runtime::AnyObject> =
                objc2::msg_send![objc2::class!(NSUserDefaults), standardUserDefaults];
            let _: () = objc2::msg_send![
                &*defaults,
                removeObjectForKey: &*NSString::from_str("NSWindow Frame zgui window_probe")
            ];
        }
        // What ⌘` cycles through: windows AppKit lists that can take the
        // keyboard.
        println!(
            "CYCLABLE {:?}",
            [&first, &second].map(|window| (
                window.title().to_string(),
                window.canBecomeKeyWindow(),
                window.isExcludedFromWindowsMenu()
            ))
        );
        first.makeKeyAndOrderFront(None);
        sleep(300).await;
        let before = key_title();
        command_backtick();
        sleep(300).await;
        println!("CYCLE before={before} after={}", key_title());
    }
}
