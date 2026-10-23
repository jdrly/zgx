//! Measures drags between windows (macOS): a tab dragged from one window's
//! strip onto another's, then one torn off onto the desktop, by synthetic
//! AppKit events posted to this process alone (the user's pointer is never
//! moved). Prints the retired instructions each pointer move costs while
//! the drag is over the other window, and what each window holds after.
//! Last, text dragged out of every window is handed to AppKit, which ends
//! the session at once (no real button is down) and the preview slides back.
//!
//! winit hears moves outside a window only while a mouse button is down,
//! which it asks the hardware (`NSEvent.pressedMouseButtons`): the probe
//! answers that for its synthetic presses, in this process only.
use std::{cell::Cell, rc::Rc};
use zgui::{compose::prelude::*, reactive::Signal};
use zgui_desktop::{Application, WindowBounds, WindowContext, WindowOptions};

#[derive(Clone)]
struct TabDrag(String, Rc<Cell<bool>>);

thread_local! {
    static STRIPS: std::cell::RefCell<Vec<(String, Signal<Vec<String>>)>> = Default::default();
}

fn strip(cx: &mut WindowContext, name: &str, tabs: Vec<&str>) {
    let tabs = cx
        .ui
        .signal(tabs.into_iter().map(String::from).collect::<Vec<_>>());
    STRIPS.with(|strips| strips.borrow_mut().push((name.into(), tabs.clone())));
    let ids = tabs.clone();
    let accept = tabs.clone();
    let window = cx.window.clone();
    let export = div()
        .id("export")
        .w(120.)
        .h(28.)
        .rounded(8.)
        .bg(rgba(0x3d7ef733))
        .child(text("Some text"))
        .on_drag(|| String::from("Some text"))
        .drag_preview(|text: &String| {
            div()
                .w(120.)
                .h(28.)
                .rounded(8.)
                .bg(rgb(0x3d7ef7))
                .child(zgui::compose::text(text))
        })
        .drag_export(|text: &String| vec![zgui::input::DragItem::Text(text.clone())])
        .on_drag_end(|_: &String, accepted| println!("EXPORT_END accepted={accepted}"));
    cx.render(
        column()
            .w_full()
            .h_full()
            .child(
                row()
                    .w_full()
                    .h(40.)
                    .px(10.)
                    .child(
                        keyed(
                            move || ids.get(),
                            move |title: String, _| {
                                let removed = tabs.clone();
                                let torn = tabs.clone();
                                let window = window.clone();
                                let closing = window.clone();
                                let id = title.clone();
                                div()
                                    .id(title.clone())
                                    .w(120.)
                                    .h(32.)
                                    .rounded(8.)
                                    .bg(rgba(0x80808030))
                                    .child(text(&title))
                                    .on_drag(move || TabDrag(id.clone(), Rc::default()))
                                    .drag_preview(move |drag: &TabDrag| {
                                        div()
                                            .w(120.)
                                            .h(32.)
                                            .rounded(8.)
                                            .bg(rgb(0x3d7ef7))
                                            .child(text(&drag.0))
                                    })
                                    .on_drag_end(move |drag: &TabDrag, accepted| {
                                        if accepted && !drag.1.get() {
                                            removed
                                                .update(|tabs| tabs.retain(|tab| *tab != drag.0));
                                            if removed.with_untracked(Vec::is_empty) {
                                                window.close();
                                            }
                                        }
                                    })
                                    .on_drag_outside(move |drag: &TabDrag, _| {
                                        println!("TORN {}", drag.0);
                                        torn.update(|tabs| tabs.retain(|tab| *tab != drag.0));
                                        if torn.with_untracked(Vec::is_empty) {
                                            closing.close();
                                        }
                                    })
                            },
                        )
                        .flex_row()
                        .gap(4.),
                    )
                    .child(div().flex_grow(1.).h_full())
                    .on_drop(move |drag: &TabDrag, _| {
                        if accept.with_untracked(|tabs| tabs.contains(&drag.0)) {
                            drag.1.set(true);
                        } else {
                            accept.update(|tabs| tabs.push(drag.0.clone()));
                        }
                    }),
            )
            .child(div().p(10.).child(export)),
    );
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = |title: &str, x| WindowOptions {
        title: title.into(),
        width: 420.,
        height: 200.,
        bounds: Some(WindowBounds::new(840, 400).at(x, 300)),
        ..Default::default()
    };
    let b = options("B", 1500);
    Application::new().window(options("A", 400)).run(move |cx| {
        strip(cx, "A", vec!["a1", "a2"]);
        cx.windows.open(b, |cx| strip(cx, "B", vec!["b1"]));
        #[cfg(target_os = "macos")]
        {
            let ui = cx.ui.downgrade();
            cx.tasks.spawn(async move {
                macos::run(ui).await;
                std::process::exit(0);
            });
        }
    })
}

#[cfg(target_os = "macos")]
fn tabs() -> Vec<(String, Vec<String>)> {
    STRIPS.with(|strips| {
        strips
            .borrow()
            .iter()
            .map(|(name, tabs)| (name.clone(), tabs.with_untracked(Clone::clone)))
            .collect()
    })
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use objc2::{MainThreadMarker, rc::Retained};
    use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType, NSWindow};
    use objc2_foundation::NSPoint;
    use std::time::Duration;
    use zgui::widgets::WeakUi;
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
        static HOLD: std::cell::Cell<bool> = std::cell::Cell::new(std::env::args().any(|arg| arg == "--hold"));
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
    /// The left of `other`'s content less `window`'s, in points.
    fn offset(window: &NSWindow, other: &NSWindow) -> (f64, f64) {
        let a = window.convertRectToScreen(window.contentView().unwrap().frame());
        let b = other.convertRectToScreen(other.contentView().unwrap().frame());
        (
            b.origin.x - a.origin.x,
            (a.origin.y + a.size.height) - (b.origin.y + b.size.height),
        )
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
            // `--hold`: a pause near the end of the first drag, to look.
            if step == steps - 10 && HOLD.with(|hold| hold.replace(false)) {
                println!("HOLD");
                sleep(2500).await;
            }
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
    pub async fn run(_ui: WeakUi) {
        sleep(3000).await;
        hold_buttons();
        let (a, b) = (window("A").unwrap(), window("B").unwrap());
        println!("BEFORE {:?}", tabs());
        // a1 (its centre, 70 points in) onto B's strip, right of b1.
        let (dx, dy) = offset(&a, &b);
        let (spent, moves) = drag(&a, (70., 26.), (dx + 300., dy + 20.), 120, 40).await;
        println!(
            "CROSS_WINDOW moves={moves} instructions_per_move={}",
            spent / moves as u64
        );
        println!("AFTER_DROP {:?}", tabs());
        // A drag within A, for comparison: a2 along its own strip.
        let (spent, moves) = drag(&a, (70., 26.), (330., 26.), 120, 40).await;
        println!(
            "SAME_WINDOW moves={moves} instructions_per_move={}",
            spent / moves as u64
        );
        // Text dragged out to the desktop: handed to AppKit.
        drag(&b, (70., 64.), (200., 700.), 60, 59).await;
        sleep(800).await;
        // a2 torn off onto the desktop, far below both windows.
        drag(&a, (70., 26.), (200., 700.), 60, 59).await;
        sleep(600).await;
        let titles: Vec<String> = {
            let mtm = MainThreadMarker::new().unwrap();
            NSApplication::sharedApplication(mtm)
                .windows()
                .iter()
                .filter(|window| window.isVisible())
                .map(|window| window.title().to_string())
                .collect()
        };
        println!("AFTER_TEAR {:?} windows={titles:?}", tabs());
    }
}
