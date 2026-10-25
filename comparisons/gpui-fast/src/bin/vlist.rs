//! The variable-height list (see zgx_workload::vlist): 100,000 chat rows of
//! wrapped text in GPUI's `list` with a `ListState` (not `uniform_list`),
//! which lays rows out to measure them as they come into view, moved by the
//! scene's scroll, fling or jump each tick.
use gpui::{prelude::*, *};
use std::time::Instant;
use zgx_workload::{PERIOD, vlist};

struct Chat {
    list: ListState,
}
impl Render for Chat {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(vlist::BACKGROUND))
            .font_family(vlist::FONT)
            .text_size(px(vlist::FONT_SIZE))
            .line_height(px(vlist::LINE_HEIGHT))
            .text_color(rgb(vlist::TEXT))
            .child(
                list(self.list.clone(), |index, _, _| {
                    row(index).into_any_element()
                })
                .size_full(),
            )
    }
}

fn row(index: usize) -> Div {
    let mut body = div()
        .flex()
        .flex_col()
        .w_full()
        .px(px(vlist::PAD.0))
        .py(px(vlist::PAD.1))
        .gap(px(vlist::GAP))
        .bg(rgb(vlist::ROW_COLORS[index % 2]))
        .child(
            div()
                .flex()
                .gap(px(8.))
                .h(px(vlist::LINE_HEIGHT))
                .child(
                    div()
                        .text_color(rgb(vlist::AUTHOR))
                        .child(vlist::author(index)),
                )
                .child(
                    div()
                        .text_color(rgb(vlist::MUTED))
                        .child(vlist::time(index)),
                ),
        )
        .child(div().w(px(vlist::TEXT_WIDTH)).child(vlist::text(index)));
    if vlist::has_image(index) {
        body = body.child(
            div()
                .w(px(vlist::IMAGE.0))
                .h(px(vlist::IMAGE.1))
                .bg(rgb(vlist::IMAGE_COLOR)),
        );
    }
    body
}

fn main() {
    let mode = vlist::Mode::from_env();
    let seconds: f64 = std::env::var("ZGX_SECONDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30.);
    gpui_platform::application().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(vlist::WIDTH), px(vlist::HEIGHT)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                is_resizable: false,
                ..Default::default()
            },
            |_, cx| {
                cx.new(|cx: &mut Context<Chat>| {
                    let list = ListState::new(vlist::ROWS, ListAlignment::Top, px(200.));
                    let state = list.clone();
                    cx.spawn(async move |chat, cx| {
                        let start = Instant::now();
                        let mut deadline = start + PERIOD;
                        let mut tick = 0_u64;
                        loop {
                            cx.background_executor()
                                .timer(deadline.saturating_duration_since(Instant::now()))
                                .await;
                            if seconds > 0. && start.elapsed().as_secs_f64() >= seconds {
                                eprintln!("workload_ticks={tick}");
                                let _ = cx.update(|cx| cx.quit());
                                break;
                            }
                            tick += 1;
                            match vlist::jump(tick) {
                                Some(row) if mode == vlist::Mode::Jump => {
                                    state.scroll_to(ListOffset {
                                        item_ix: row,
                                        offset_in_item: px(0.),
                                    })
                                }
                                _ => {
                                    // At the end, back to the top (100k rows take minutes to reach).
                                    if state.logical_scroll_top().item_ix + 64 >= vlist::ROWS {
                                        state.scroll_to(ListOffset::default());
                                    } else {
                                        state.scroll_by(px(vlist::delta(mode, tick)));
                                    }
                                }
                            }
                            if chat.update(cx, |_, cx| cx.notify()).is_err() {
                                break;
                            }
                            deadline += PERIOD;
                            if deadline <= Instant::now() {
                                deadline = Instant::now() + PERIOD;
                            }
                        }
                    })
                    .detach();
                    Chat { list }
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
