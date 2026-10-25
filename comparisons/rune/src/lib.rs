//! What the Rune adapters share: their window, colours, the bundled font,
//! and the workload's clock, which every adapter of every framework runs the
//! same way (comparisons/README.md).
use std::time::{Duration, Instant};

use rune::{App, AppHandle, AppSpec, Chrome, Color, Tx, WindowOptions};
pub use zgx_workload::PERIOD;

/// A 0xrrggbb colour (Rune's colours are linear light).
pub fn rgb(c: u32) -> Color {
    rgba(c, 1.0)
}

pub fn rgba(c: u32, a: f32) -> Color {
    let channel = |shift: u32| rune::color::srgb_decode(((c >> shift) & 0xff) as f32 / 255.0);
    Color {
        r: channel(16),
        g: channel(8),
        b: channel(0),
        a,
    }
}

/// The font file the runner staged (`ZGX_FONT_FILE`), reported as the runner
/// checks it, as `AppSpec::fonts` takes it: the app's faces for its whole
/// run.
pub fn fonts() -> &'static [&'static [u8]] {
    match zgx_workload::font_file() {
        Some(bytes) => Box::leak(vec![&*Box::leak(bytes.into_boxed_slice())].into_boxed_slice()),
        None => &[],
    }
}

/// The scene's spec in a standard titled window of its size, not resizable,
/// with every family (sans and mono) the bundled font, and its workload's
/// ticks once the window is up (`ticks`, none for `None`). Translucency
/// composites in sRGB (`App::srgb_blend`, "matching CSS and Core Animation"),
/// as the other frameworks blend, rather than Rune's default linear light: a
/// surface format and a shader flag, no extra pass.
pub fn run<S: 'static>(spec: AppSpec<S>, ticks: Option<Ticks<S>>) {
    let spec = AppSpec {
        fonts: fonts(),
        families: (zgx_workload::FONT, zgx_workload::FONT),
        ..spec
    };
    let app = App::new(spec)
        .srgb_blend(true)
        .chrome(Chrome {
            full_size_content: false,
            ..Chrome::DEFAULT
        })
        .window_options(WindowOptions {
            resizable: false,
            ..Default::default()
        });
    let handle = app.handle();
    let mut ticks = ticks;
    app.on_ready(move |_| {
        if let Some(ticks) = ticks.take() {
            let handle = handle.clone();
            std::thread::spawn(move || ticks.run(handle));
        }
    })
    .run();
}

/// The workload's clock: a tick once a `PERIOD` by the wall clock while
/// active, until `seconds` (never for 0), then `report` and quit. It runs on
/// a timer thread, as Rune's `AppHandle::update` is for ("a timer or feed"):
/// each tick runs on the UI thread as it comes, and the timer waits for it,
/// as the Rust adapters' loops wait for theirs, so a busy UI thread drops
/// ticks rather than queueing them. `tick` returns whether it changed state
/// the view reads; a tick whose `turn` is false (heavy's ZGX_PARITY) is
/// skipped.
pub struct Ticks<S> {
    pub seconds: f64,
    pub active: bool,
    pub turn: fn() -> bool,
    pub tick: fn(&mut S, &mut Tx) -> bool,
    pub report: fn(&S) -> String,
}

impl<S: 'static> Ticks<S> {
    fn run(self, handle: AppHandle<S>) {
        let Ticks {
            seconds,
            active,
            turn,
            tick,
            report,
        } = self;
        let start = Instant::now();
        let end = (seconds > 0.0).then(|| start + Duration::from_secs_f64(seconds));
        let finish = || {
            handle.update_if(move |s, tx| {
                eprintln!("{}", report(s));
                tx.quit();
                false
            })
        };
        if !active {
            if let Some(end) = end {
                std::thread::sleep(end.saturating_duration_since(Instant::now()));
                finish();
            }
            return;
        }
        let (done, ran) = std::sync::mpsc::sync_channel::<()>(1);
        let mut next = start + PERIOD;
        loop {
            let wake = end.map_or(next, |end| end.min(next));
            std::thread::sleep(wake.saturating_duration_since(Instant::now()));
            if end.is_some_and(|end| Instant::now() >= end) {
                finish();
                return;
            }
            if turn() {
                let done = done.clone();
                handle.update_if(move |s, tx| {
                    let changed = tick(s, tx);
                    let _ = done.send(());
                    changed
                });
                if ran.recv().is_err() {
                    return;
                }
            }
            next += PERIOD;
            let now = Instant::now();
            if next <= now {
                next = now + PERIOD;
            }
        }
    }
}

/// Always: a scene with no turns to take.
pub fn always() -> bool {
    true
}
