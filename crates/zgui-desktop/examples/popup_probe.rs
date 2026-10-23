//! Measures popup windows (macOS): a menu hanging past the window's bottom
//! edge, opened, hovered and chosen from by synthetic AppKit events posted
//! to this process alone (the user's pointer is never moved).
//!
//! Prints the time from the posted click to the popup window showing, then
//! idles twice (before any popup, after one opened and closed) for an
//! outside sampler: `READY` marks launch, `IDLE_A`/`IDLE_B` each idle span.
//! `ZGUI_POPUP_POOL=n` sets how many popup windows are made ahead.
use zgui::compose::prelude::*;
use zgui_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions {
            title: "Popup probe".into(),
            width: 420.,
            height: 220.,
            bounds: Some(zgui_desktop::WindowBounds::new(840, 440).at(400, 300)),
            ..Default::default()
        })
        .run(|cx| {
            let open = cx.ui.signal(false);
            let chosen = cx.ui.signal(String::new());
            let anchor = button()
                .id("file")
                .h(28.)
                .w(120.)
                .child(text("File"))
                .on_click({
                    let open = open.clone();
                    move || {
                        open.set(true);
                    }
                });
            let items = (1..=8).map(|n| {
                let chosen = chosen.clone();
                menu_item(format!("Item {n}"))
                    .id(format!("item-{n}"))
                    .h(24.)
                    .on_click(move || {
                        chosen.set(format!("Item {n}"));
                    })
            });
            let view = cx.render(
                column().w_full().h_full().p(16.).justify_end().child(
                    menu("File", open.clone(), anchor)
                        .w(200.)
                        .p(5.)
                        .children(items),
                ),
            );
            #[cfg(target_os = "macos")]
            {
                let window = cx.window.clone();
                let ui = cx.ui.downgrade();
                cx.tasks.spawn(async move {
                    macos::run(ui, view, open, chosen).await;
                    window.close();
                });
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = (view, open, chosen);
                eprintln!("popup_probe: macOS only");
                cx.window.close();
            }
        })
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::{MainThreadMarker, rc::Retained};
    use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType, NSWindow};
    use objc2_foundation::NSPoint;
    use std::time::Duration;
    use zgui::{compose::ViewHandle, reactive::Signal, scene::Rect, widgets::WeakUi};
    use zgui_gpu::present_log::now_ns;

    async fn sleep(ms: u64) {
        zgui::timer::sleep(Duration::from_millis(ms)).await;
    }
    fn seconds() -> f64 {
        now_ns() as f64 / 1e9
    }
    /// The app's windows: its own (titled) and the shown popups.
    fn windows() -> (Retained<NSWindow>, Vec<Retained<NSWindow>>) {
        let mtm = MainThreadMarker::new().unwrap();
        let mut main = None;
        let mut popups = Vec::new();
        for window in NSApplication::sharedApplication(mtm).windows().iter() {
            if !window.isVisible() {
                continue;
            }
            if window.title().to_string() == "Popup probe" {
                main = Some(window);
            } else {
                popups.push(window);
            }
        }
        (main.expect("probe window"), popups)
    }
    /// Post a mouse event at `(x, y)` in `window`'s content (top-down).
    fn post(window: &NSWindow, kind: NSEventType, x: f64, y: f64) {
        let mtm = MainThreadMarker::new().unwrap();
        let height = window.contentView().unwrap().frame().size.height;
        let event = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            kind,
            NSPoint::new(x, height - y),
            NSEventModifierFlags::empty(),
            seconds(),
            window.windowNumber(),
            None,
            0,
            1,
            if kind == NSEventType::LeftMouseDown { 1. } else { 0. },
        )
        .unwrap();
        NSApplication::sharedApplication(mtm).postEvent_atStart(&event, false);
    }
    fn center(rect: Rect) -> (f64, f64) {
        (
            (rect.x + rect.width / 2.) as f64,
            (rect.y + rect.height / 2.) as f64,
        )
    }
    pub async fn run(ui: WeakUi, view: ViewHandle, open: Signal<bool>, chosen: Signal<String>) {
        let start = seconds();
        sleep(3000).await;
        println!("READY {:.3}", seconds());
        let bounds = |id: &str| {
            let ui = ui.upgrade().unwrap();
            let node = view.find(id).unwrap();
            ui.scene.borrow().bounds(node)
        };
        // Idle before any popup (from 18 s: the driver's startup memory goes
        // within 15 s, see docs).
        let until = |at: f64| sleep(((start + at - seconds()) * 1000.).max(0.) as u64);
        until(18.).await;
        println!("IDLE_A {:.3}", seconds());
        until(30.).await;
        let mut latencies = Vec::new();
        for round in 0..5 {
            let (main, _) = windows();
            let (x, y) = center(bounds("file"));
            let clicked = seconds();
            post(&main, NSEventType::LeftMouseDown, x, y);
            post(&main, NSEventType::LeftMouseUp, x, y);
            let mut shown = None;
            for _ in 0..500 {
                sleep(1).await;
                if !windows().1.is_empty() {
                    shown = Some(seconds());
                    break;
                }
            }
            let Some(shown) = shown else {
                println!("OPEN_FAILED round={round} open={}", open.get());
                return;
            };
            latencies.push((shown - clicked) * 1000.);
            // Hover down the items, then choose the sixth.
            let popup = windows().1.remove(0);
            let frame = popup.frame();
            let origin = {
                let parent = main.convertRectToScreen(main.contentView().unwrap().frame());
                (
                    frame.origin.x - parent.origin.x,
                    (parent.origin.y + parent.size.height) - (frame.origin.y + frame.size.height),
                )
            };
            for n in 1..=8 {
                let (x, y) = center(bounds(&format!("item-{n}")));
                post(&popup, NSEventType::MouseMoved, x - origin.0, y - origin.1);
                sleep(30).await;
            }
            let (x, y) = center(bounds("item-6"));
            post(
                &popup,
                NSEventType::LeftMouseDown,
                x - origin.0,
                y - origin.1,
            );
            post(&popup, NSEventType::LeftMouseUp, x - origin.0, y - origin.1);
            sleep(300).await;
            println!(
                "ROUND {round} chosen={:?} open={} popups={}",
                chosen.get(),
                open.get(),
                windows().1.len()
            );
            chosen.set(String::new());
            sleep(300).await;
        }
        println!(
            "OPEN_LATENCY_MS {}",
            latencies
                .iter()
                .map(|ms| format!("{ms:.2}"))
                .collect::<Vec<_>>()
                .join(" ")
        );
        until(52.).await;
        println!("IDLE_B {:.3}", seconds());
        until(63.).await;
    }
}
