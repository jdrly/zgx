//! The variable-height list scene (see zgx_workload::vlist) in Rune's own
//! virtualized list: `El::virtual_rows` on a growth stack inside a scroll
//! viewport, which builds only the rows in its mounted window, measures them
//! as they are laid out (keeping each height by the row's key) and estimates
//! the rest. Each tick moves it as the scene says:
//!
//! - scroll and fling (and jump between its jumps) through `Tx::wheel`, the
//!   wheel delta Rune's own scroll path takes (`Runtime::wheel_xy`), which
//!   PATCH.md adds to the pinned Rune: its public API has no vertical scroll
//!   offset;
//! - a jump through `Tx::scroll_into_view`, which reveals the row as little as
//!   shows it (nearest), not at the top.
//!
//! `ZGX_MODE=probe` reports the probe rows' laid-out heights from Rune's
//! headless runtime instead (any row's height is the same wherever it sits).
//!
//!     ZGX_MODE=fling ZGX_SECONDS=10 bin/vlist
use std::borrow::Cow;

use rune::text::TextSystem;
use rune::tok::TextStyle;
use rune::{AppSpec, Cx, El, Env, GrowthSpec, Runtime, ScrollSpec, Tx, col, el, row, text};
use zgx_compare_rune::{Ticks, always, rgb};
use zgx_workload::vlist::{self, Mode};

/// The estimate of a row not yet measured: three lines and no image, as the
/// zgx adapter starts its rows.
const ESTIMATE: f32 =
    2.0 * vlist::PAD.1 + vlist::LINE_HEIGHT + vlist::GAP + 3.0 * vlist::LINE_HEIGHT;
/// Where the wheel points: the list's middle.
const WHEEL_AT: (f32, f32) = (vlist::WIDTH / 2.0, vlist::HEIGHT / 2.0);

struct Chat {
    mode: Mode,
    ticks: u64,
}

fn key(i: usize) -> String {
    format!("r{i}")
}

fn style() -> TextStyle {
    TextStyle::new(vlist::FONT_SIZE, 400.0).lh(vlist::LINE_HEIGHT)
}

fn chat_row(i: usize) -> El {
    let mut body = col()
        .w_full()
        .px(vlist::PAD.0)
        .py(vlist::PAD.1)
        .gap(vlist::GAP)
        .fill(rgb(vlist::ROW_COLORS[i % 2]))
        .child(
            row()
                .gap(8.0)
                .h(vlist::LINE_HEIGHT)
                .child(
                    text(vlist::author(i))
                        .font(style())
                        .color(rgb(vlist::AUTHOR)),
                )
                .child(text(vlist::time(i)).font(style()).color(rgb(vlist::MUTED))),
        )
        .child(
            text(vlist::text(i))
                .font(style())
                .color(rgb(vlist::TEXT))
                .wrap()
                .w(vlist::TEXT_WIDTH),
        );
    if vlist::has_image(i) {
        body = body.child(
            el().size(vlist::IMAGE.0, vlist::IMAGE.1)
                .fill(rgb(vlist::IMAGE_COLOR)),
        );
    }
    body
}

fn view(_: &Chat, _: &mut Cx<Chat>) -> El {
    col().w_full().h_full().fill(rgb(vlist::BACKGROUND)).child(
        col()
            .key("list")
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .w_full()
            .h_full()
            .child(
                col()
                    .key("rows")
                    .growth(GrowthSpec::default())
                    .virtual_rows(
                        vlist::ROWS,
                        ESTIMATE,
                        |_: &Chat, i| Cow::Owned(key(i)),
                        |_: &Chat, i| chat_row(i),
                    ),
            ),
    )
}

/// A tick moves the list; it changes nothing the view reads.
fn tick(s: &mut Chat, tx: &mut Tx) -> bool {
    s.ticks += 1;
    match vlist::jump(s.ticks) {
        Some(row) if s.mode == Mode::Jump => tx.scroll_into_view(key(row)),
        // Rune clamps at the end, which a run never reaches (100k rows).
        _ => tx.wheel(WHEEL_AT.0, WHEEL_AT.1, -vlist::delta(s.mode, s.ticks)),
    }
    false
}

/// The probe, headless: reveal each probe row, let it lay out, and keep the
/// height Rune laid it out at.
fn probe() {
    let fonts = zgx_compare_rune::fonts();
    let text_system = TextSystem::new(fonts, (zgx_workload::FONT, zgx_workload::FONT));
    let state = Chat {
        mode: Mode::Probe,
        ticks: 0,
    };
    let env = Env {
        system_dark: true,
        scale: 2.0,
    };
    let mut rt = Runtime::with_text(state, view, (vlist::WIDTH, vlist::HEIGHT), env, text_system);
    rt.frame(0.0);
    let rows = vlist::probe_rows();
    let mut heights = Vec::with_capacity(rows.len());
    for &r in &rows {
        rt.update(|_, tx| tx.scroll_into_view(key(r)));
        for _ in 0..8 {
            rt.frame(1.0 / 60.0);
        }
        rt.settle_all();
        rt.frame(1.0 / 60.0);
        let ([_, _, _, h], _) = rt
            .presented(&key(r))
            .unwrap_or_else(|| panic!("row {r} not laid out"));
        heights.push((r, h));
    }
    let ([_, _, width, _], _) = rt.presented("list").expect("the list");
    eprintln!("{}", vlist::probe_report(&heights, width));
}

fn main() {
    if vlist::Mode::from_env() == Mode::Probe {
        probe();
        return;
    }
    let ticks = Ticks {
        seconds: zgx_workload::Workload::from_env().seconds,
        active: true,
        turn: always,
        tick,
        report: |s| format!("workload_ticks={}", s.ticks),
    };
    zgx_compare_rune::run(
        AppSpec::new(
            "Rune vlist",
            (vlist::WIDTH, vlist::HEIGHT),
            || Chat {
                mode: Mode::from_env(),
                ticks: 0,
            },
            view,
        ),
        Some(ticks),
    );
}
