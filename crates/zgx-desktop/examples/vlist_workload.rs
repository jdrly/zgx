//! The variable-height list comparison scene (see zgx_workload::vlist),
//! shared with the GPUI Fast and MyGo adapters: 100,000 chat rows of wrapped
//! text in zgx's measured virtual list, which measures each row as it is
//! laid out and estimates the rest, moved by the scene's scroll, fling or
//! jump each tick. Run with ZGX_MODE=fling ZGX_SECONDS=10.
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};
use zgx::compose::prelude::*;
use zgx_desktop::{Application, WindowOptions};
use zgx_workload::{
    PERIOD,
    vlist::{self, Mode},
};

fn chat_row(index: usize) -> View {
    let mut body = column()
        .w_full()
        .px(vlist::PAD.0)
        .py(vlist::PAD.1)
        .gap(vlist::GAP)
        .bg(rgb(vlist::ROW_COLORS[index % 2]))
        .child(
            row()
                .gap(8.)
                .h(vlist::LINE_HEIGHT)
                .child(text(vlist::author(index)).text_color(rgb(vlist::AUTHOR)))
                .child(text(vlist::time(index)).text_color(rgb(vlist::MUTED))),
        )
        .child(text(vlist::text(index)).w(vlist::TEXT_WIDTH));
    if vlist::has_image(index) {
        body = body.child(
            div()
                .size(vlist::IMAGE.0, vlist::IMAGE.1)
                .bg(rgb(vlist::IMAGE_COLOR)),
        );
    }
    body
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = Mode::from_env();
    let seconds: f64 = std::env::var("ZGX_SECONDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30.);
    Application::new()
        .window(WindowOptions {
            title: "zgx vlist".into(),
            width: vlist::WIDTH as f64,
            height: vlist::HEIGHT as f64,
            resizable: false,
            ..Default::default()
        })
        .run(move |cx| {
            let ticks = Rc::new(Cell::new(0_u64));
            // Rows start at the estimate of a three-line row with no image.
            let heights = VariableHeights::new(
                &cx.ui.runtime,
                vlist::ROWS,
                2. * vlist::PAD.1 + vlist::LINE_HEIGHT + vlist::GAP + 3. * vlist::LINE_HEIGHT,
            );
            let offset = cx.ui.signal(0_f32);
            let list = measured_virtual_list(
                offset.clone(),
                heights.clone(),
                2,
                |i| i,
                |_, index, _| chat_row(index),
            )
            .w_full()
            .h_full();
            cx.render(
                column()
                    .w_full()
                    .h_full()
                    .bg(rgb(vlist::BACKGROUND))
                    .font_family(vlist::FONT)
                    .text_size(vlist::FONT_SIZE)
                    .line_height(vlist::LINE_HEIGHT)
                    .text_color(rgb(vlist::TEXT))
                    .child(list),
            );
            cx.on_closed({
                let ticks = ticks.clone();
                move || eprintln!("workload_ticks={}", ticks.get())
            });
            let window = cx.window.clone();
            cx.tasks.spawn(async move {
                let start = Instant::now();
                let end = (seconds > 0.).then(|| start + Duration::from_secs_f64(seconds));
                let mut next = start + PERIOD;
                loop {
                    zgx::timer::sleep_until(end.map_or(next, |end| end.min(next))).await;
                    if end.is_some_and(|end| Instant::now() >= end) {
                        window.close();
                        break;
                    }
                    let tick = ticks.get() + 1;
                    ticks.set(tick);
                    match vlist::jump(tick) {
                        Some(row) if mode == Mode::Jump => {
                            offset.set(heights.row_offset(row));
                        }
                        _ => {
                            let max = (heights.content_height() - vlist::HEIGHT).max(0.);
                            let moved = offset.get() + vlist::delta(mode, tick);
                            // At the end, back to the top (100k rows take minutes to reach).
                            offset.set(if moved > max { 0. } else { moved });
                        }
                    }
                    next += PERIOD;
                    let now = Instant::now();
                    if next <= now {
                        next = now + PERIOD;
                    }
                }
            });
        })
}
