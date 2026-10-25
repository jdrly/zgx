//! The display-rate animation scene (see zgx_workload::animation) in Rune:
//! every dot's opacity and every shimmer character's alpha change on every
//! frame. They are Rune's high-frequency values (`Live`, bound with
//! `live_opacity`), set in the app's poll at the frame's time, which asks
//! for the next frame (`Tx::wake_after(0)`): no view re-run, no layout.
//!
//!     ZGX_SECONDS=10 bin/animate
use std::time::{Duration, Instant};

use rune::tok::TextStyle;
use rune::{AppSpec, Color, Cx, El, Live, Tx, el, row, text};
use zgx_compare_rune::rgb;
use zgx_workload::{Workload, animation::*};

struct Scene {
    dots: Vec<Live<f32>>,
    chars: Vec<(String, Live<f32>)>,
    seconds: f64,
    start: Option<Instant>,
    frames: u64,
}

fn view(s: &Scene, _: &mut Cx<Scene>) -> El {
    let mut root = el().w_full().h_full().fill(rgb(BACKGROUND));
    for (i, opacity) in s.dots.iter().enumerate() {
        let (x, y) = dot_position(i);
        root = root.child(
            el().absolute(x, y)
                .size(DOT, DOT)
                .radius(DOT / 2.0)
                .fill(rgb(DOT_COLOR))
                .live_opacity(opacity),
        );
    }
    let mut line = row().absolute(SHIMMER_ORIGIN.0, SHIMMER_ORIGIN.1);
    for (ch, alpha) in &s.chars {
        line = line.child(
            text(ch.as_str())
                .font(TextStyle::new(18.0, 400.0))
                .color(Color::WHITE)
                .live_opacity(alpha),
        );
    }
    root.child(line)
}

fn poll(s: &mut Scene, clock: f64, tx: &mut Tx) -> bool {
    let start = *s.start.get_or_insert_with(Instant::now);
    if s.seconds > 0.0 && start.elapsed().as_secs_f64() >= s.seconds {
        eprintln!(
            "animation_frames={} fps={:.1}",
            s.frames,
            s.frames as f64 / start.elapsed().as_secs_f64()
        );
        tx.quit();
        return false;
    }
    s.frames += 1;
    let t = clock as f32;
    for (i, dot) in s.dots.iter().enumerate() {
        dot.set(dot_opacity(i, t));
    }
    let count = s.chars.len();
    for (i, (_, alpha)) in s.chars.iter().enumerate() {
        alpha.set(shimmer_alpha(i, count, t) as f32 / 255.0);
    }
    tx.wake_after(Duration::ZERO);
    false
}

fn main() {
    let spec = AppSpec::<Scene> {
        poll: Some(poll),
        ..AppSpec::new(
            "Rune animation",
            (960.0, 720.0),
            || Scene {
                dots: (0..COLUMNS * ROWS)
                    .map(|i| Live::new(dot_opacity(i, 0.0)))
                    .collect(),
                chars: SHIMMER
                    .chars()
                    .map(|c| (c.to_string(), Live::new(90.0 / 255.0)))
                    .collect(),
                seconds: Workload::from_env().seconds,
                start: None,
                frames: 0,
            },
            view,
        )
    };
    zgx_compare_rune::run(spec, None);
}
