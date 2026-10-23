//! Measures drags (macOS) by synthetic AppKit events posted to this
//! process alone (the user's pointer is never moved), printing the retired
//! instructions each pointer move costs: a chip dragged along its window and
//! released where nothing takes it (its preview sliding back), a tab dragged
//! onto another window's strip, then a tab torn off (released outside both
//! windows, it opens in a window of its own).
//!
//! Nothing that carries an export leaves the windows: AppKit's session
//! follows the real pointer, and with no real button down it drops at once
//! on whatever app is under it (Finder makes a text clipping).
//!
//! winit hears moves outside a window only while a mouse button is down,
//! which it asks the hardware (`NSEvent.pressedMouseButtons`): the probe
//! answers that for its synthetic presses, in this process only.
//!
//! On Windows `scripts/windows_drag_smoke.py` drives it from outside, with
//! real input, onto a test-owned drop target (`drop_target_probe`).
//! `DRAG_PROBE_ANIMATE=1` moves a bar every display frame.
use zgui::compose::prelude::*;
use zgui_desktop::{Application, WindowBounds, WindowOptions};

/// A tab: it carries nothing to other apps, so it can be torn off.
#[derive(Clone)]
struct Tab(&'static str);

fn tab(name: &'static str) -> View {
    div()
        .id(name)
        .w(120.)
        .h(28.)
        .rounded(8.)
        .bg(rgba(0x80808040))
        .items_center()
        .justify_center()
        .child(text(name))
        .on_drag(move || Tab(name))
        .drag_preview(|tab: &Tab| {
            div()
                .w(120.)
                .h(28.)
                .rounded(8.)
                .bg(rgb(0x808080))
                .items_center()
                .justify_center()
                .child(zgui::compose::text(tab.0))
        })
        .on_drag_end(|tab: &Tab, accepted| println!("TAB_END {} accepted={accepted}", tab.0))
        .on_drag_outside(|tab: &Tab, _| println!("TORN {}", tab.0))
}

fn options(title: &str, x: i32) -> WindowOptions {
    WindowOptions {
        title: title.into(),
        width: 420.,
        height: 200.,
        bounds: Some(WindowBounds::new(840, 400).at(x, 300)),
        ..Default::default()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(options("Drag", 400))
        .run(move |cx| {
            cx.windows.open(options("Other", 1400), |cx| {
                let strip = div()
                    .w_full()
                    .h(60.)
                    .bg(rgba(0x3d7ef720))
                    .on_drop(|tab: &Tab, _| println!("DROPPED {}", tab.0));
                cx.render(column().w_full().h_full().p(10.).child(strip));
            });
            let chip = div()
                .id("chip")
                .w(120.)
                .h(28.)
                .rounded(8.)
                .bg(rgba(0x3d7ef733))
                .items_center()
                .justify_center()
                .child(text("Some text"))
                .on_drag(|| String::from("Some text"))
                .drag_preview(|text: &String| {
                    div()
                        .w(120.)
                        .h(28.)
                        .rounded(8.)
                        .bg(rgb(0x3d7ef7))
                        .items_center()
                        .justify_center()
                        .child(zgui::compose::text(text))
                })
                .drag_export(|text: &String| vec![zgui::input::DragItem::Text(text.clone())])
                .on_drag_end(|_: &String, accepted| println!("DRAG_END accepted={accepted}"));
            // `DRAG_PROBE_ANIMATE=1`: a bar moving every display frame, to
            // see that the window goes on animating while the platform runs
            // the drag (Windows' `DoDragDrop` loop).
            let clock = cx.ui.signal(0_f32);
            if std::env::var_os("DRAG_PROBE_ANIMATE").is_some() {
                let (clock, source) = (clock.clone(), cx.frames.clone());
                let start = std::time::Instant::now();
                cx.tasks.spawn(async move {
                    loop {
                        source.next().await;
                        clock.set(start.elapsed().as_secs_f32());
                    }
                });
            }
            let bar = div()
                .w(40.)
                .h(8.)
                .bg(rgb(0x3d7ef7))
                .reactive_style(move || zgui::style::Styles::new().ml((clock.get() * 200.) % 300.));
            cx.render(
                column()
                    .w_full()
                    .h_full()
                    .p(10.)
                    .gap(10.)
                    .child(chip)
                    .child(tab("tab"))
                    .child(bar),
            );
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
    use objc2_foundation::NSPoint;
    use std::time::Duration;
    use zgui_gpu::present_log::now_ns;

    unsafe extern "C" {
        fn thread_selfcounts(kind: i32, buffer: *mut u64, size: usize) -> i32;
    }
    /// The main thread's retired instructions so far (the kernel's
    /// per-thread counters, as xnu's own tests read them): the work a move
    /// costs the app, apart from the system's threads in the process.
    fn instructions() -> u64 {
        let mut counts = [0u64; 2];
        // SAFETY: kind 1 fills instructions then cycles.
        unsafe { thread_selfcounts(1, counts.as_mut_ptr(), 16) };
        counts[0]
    }
    thread_local! {
        static PRESSED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
        static POSTING: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    }
    extern "C-unwind" fn pressed_buttons(
        _: *mut objc2::runtime::AnyObject,
        _: objc2::runtime::Sel,
    ) -> usize {
        PRESSED.with(|pressed| pressed.get())
    }
    /// `+[NSEvent pressedMouseButtons]` answers the probe's presses.
    fn hold_buttons() {
        let class = objc2::runtime::AnyClass::get(c"NSEvent").unwrap();
        // SAFETY: a class method returning NSUInteger, replaced in this
        // process by one of the same signature.
        unsafe {
            let imp: objc2::runtime::Imp = std::mem::transmute(
                pressed_buttons
                    as extern "C-unwind" fn(
                        *mut objc2::runtime::AnyObject,
                        objc2::runtime::Sel,
                    ) -> usize,
            );
            objc2::ffi::class_replaceMethod(
                class.metaclass() as *const _ as *mut _,
                objc2::sel!(pressedMouseButtons),
                imp,
                c"Q@:".as_ptr(),
            );
        }
    }
    async fn sleep(ms: u64) {
        zgui::timer::sleep(Duration::from_millis(ms)).await;
    }
    fn window(title: &str) -> Option<Retained<NSWindow>> {
        let mtm = MainThreadMarker::new().unwrap();
        NSApplication::sharedApplication(mtm)
            .windows()
            .iter()
            .find(|window| window.isVisible() && window.title().to_string() == title)
    }
    fn post(window: &NSWindow, kind: NSEventType, x: f64, y: f64) {
        let mtm = MainThreadMarker::new().unwrap();
        let height = window.contentView().unwrap().frame().size.height;
        let event = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            kind,
            NSPoint::new(x, height - y),
            NSEventModifierFlags::empty(),
            now_ns() as f64 / 1e9,
            window.windowNumber(),
            None,
            0,
            1,
            if kind == NSEventType::LeftMouseUp { 0. } else { 1. },
        )
        .unwrap();
        NSApplication::sharedApplication(mtm).postEvent_atStart(&event, false);
    }
    /// Drag from `(x, y)` in `window` to `(to_x, to_y)` in `steps` moves,
    /// then release; returns the instructions of the moves from `measure_from`.
    async fn drag(
        window: &NSWindow,
        (x, y): (f64, f64),
        (to_x, to_y): (f64, f64),
        steps: usize,
        measure_from: usize,
    ) -> (u64, usize) {
        PRESSED.with(|pressed| pressed.set(1));
        post(window, NSEventType::LeftMouseDown, x, y);
        sleep(50).await;
        let mut start = 0;
        for step in 1..=steps {
            if step == measure_from {
                sleep(100).await;
                POSTING.with(|posting| posting.set(0));
                start = instructions();
            }
            let t = step as f64 / steps as f64;
            let before = instructions();
            post(
                window,
                NSEventType::LeftMouseDragged,
                x + (to_x - x) * t,
                y + (to_y - y) * t,
            );
            POSTING.with(|posting| posting.set(posting.get() + instructions() - before));
            sleep(8).await;
        }
        // Making and posting the synthetic events is the probe's own work.
        let spent = instructions() - start - POSTING.with(|posting| posting.replace(0));
        // As long again, the pointer held still: what the drag costs
        // anyway (frames, timers) is not the moves'.
        sleep(100).await;
        let still = instructions();
        for _ in measure_from..=steps {
            sleep(8).await;
        }
        let spent = spent.saturating_sub(instructions() - still);
        sleep(100).await;
        post(window, NSEventType::LeftMouseUp, to_x, to_y);
        sleep(50).await;
        PRESSED.with(|pressed| pressed.set(0));
        sleep(350).await;
        (spent, steps + 1 - measure_from)
    }
    /// The left of `other`'s content less `window`'s, in points.
    fn offset(window: &NSWindow, other: &NSWindow) -> (f64, f64) {
        let a = window.convertRectToScreen(window.contentView().unwrap().frame());
        let b = other.convertRectToScreen(other.contentView().unwrap().frame());
        (
            b.origin.x - a.origin.x,
            (a.origin.y + a.size.height) - (b.origin.y + b.size.height),
        )
    }
    pub async fn run() {
        sleep(3000).await;
        hold_buttons();
        let window = window("Drag").unwrap();
        let other = self::window("Other").unwrap();
        // Hover first, as a real pointer does.
        post(&window, NSEventType::MouseMoved, 60., 24.);
        sleep(200).await;
        for _ in 0..3 {
            let (spent, moves) = drag(&window, (60., 24.), (330., 150.), 120, 40).await;
            println!(
                "SAME_WINDOW moves={moves} instructions_per_move={}",
                spent / moves as u64
            );
        }
        // The tab (its centre 72 points down) onto the other window's strip:
        // measured once it is over that window.
        let (dx, dy) = offset(&window, &other);
        for _ in 0..3 {
            let (spent, moves) = drag(&window, (60., 62.), (dx + 200., dy + 40.), 120, 90).await;
            println!(
                "CROSS_WINDOW moves={moves} instructions_per_move={}",
                spent / moves as u64
            );
        }
        // Torn off: released below both windows.
        drag(&window, (60., 62.), (200., 400.), 60, 59).await;
        sleep(600).await;
        let windows = {
            let mtm = MainThreadMarker::new().unwrap();
            NSApplication::sharedApplication(mtm)
                .windows()
                .iter()
                .filter(|window| window.isVisible())
                .count()
        };
        println!("WINDOWS {windows}");
        println!("DONE");
    }
}
