//! The heavy dashboard (see zgx_workload::heavy) in Rune, as the zgx, GPUI
//! Fast and MyGo adapters draw it: tiles, service cards with a value, a
//! progress bar and a sparkline each, a table showing the rows in view (and
//! two each side, keyed by row) and a streaming log. Each tick changes the
//! state and the view re-runs; Rune diffs it against the retained tree. A
//! tick applies at once (`Tx::instant`), as live data does, rather than on
//! Rune's default layout and colour springs.
//!
//!     ZGX_MODE=both ZGX_SECONDS=10 bin/heavy
use rune::tok::TextStyle;
use rune::{AppSpec, Cx, El, Tx, el, text};
use zgx_compare_rune::{Ticks, rgb};
use zgx_workload::{Mode, heavy::*};

struct Dashboard {
    work: Heavy,
}

fn at(e: El, (x, y): (f32, f32), w: f32, h: f32) -> El {
    e.absolute(x, y).size(w, h)
}
/// `at` inside a panel: CSS (which the scene's geometry follows, as zgx, GPUI
/// and MyGo lay it out) places an absolute child inside its parent's border,
/// but a Rune border takes no layout space, so the child moves in by it.
fn inside(e: El, (x, y): (f32, f32), w: f32, h: f32) -> El {
    at(e, (x + BORDER_WIDTH, y + BORDER_WIDTH), w, h)
}
const BORDER_WIDTH: f32 = 1.0;
fn panel(e: El, radius: f32) -> El {
    e.radius(radius)
        .border(BORDER_WIDTH, rgb(BORDER))
        .fill(rgb(PANEL))
}
fn label(s: impl Into<String>, size: f32, line: f32) -> El {
    text(s)
        .font(TextStyle::new(size, 400.0).lh(line))
        .color(rgb(TEXT))
}

fn view(d: &Dashboard, _: &mut Cx<Dashboard>) -> El {
    let live = d.work.live;
    let mut root = el().size(WIDTH, HEIGHT).fill(rgb(BACKGROUND)).child(at(
        label(TITLE, 18.0, 24.0),
        TITLE_ORIGIN,
        600.0,
        24.0,
    ));
    for i in 0..TILES {
        root = root.child(
            panel(at(el(), tile_position(i), TILE.0, TILE.1), 8.0)
                .child(inside(
                    label(tile_label(i), 11.0, 14.0).color(rgb(MUTED)),
                    TILE_LABEL,
                    176.0,
                    14.0,
                ))
                .child(inside(
                    label(tile_value(i, live), 20.0, 26.0),
                    TILE_VALUE,
                    176.0,
                    26.0,
                )),
        );
    }
    for i in 0..CARDS {
        let mut card = panel(at(el(), card_position(i), CARD.0, CARD.1), 6.0)
            .child(inside(
                label(card_title(i), 11.0, 14.0).color(rgb(MUTED)),
                CARD_TITLE,
                104.0,
                14.0,
            ))
            .child(inside(
                label(card_value(i, live), 16.0, 20.0),
                CARD_VALUE,
                104.0,
                20.0,
            ))
            .child(
                inside(el(), (PROGRESS.0, PROGRESS.1), PROGRESS.2, PROGRESS.3)
                    .radius(2.0)
                    .fill(rgb(TRACK)),
            )
            .child(
                inside(
                    el(),
                    (PROGRESS.0, PROGRESS.1),
                    card_progress(i, live),
                    PROGRESS.3,
                )
                .radius(2.0)
                .fill(rgb(ACCENT)),
            );
        for bar in 0..BARS {
            let h = bar_height(i, bar, live);
            card = card
                .child(inside(el(), (bar_x(bar), SPARK_BASE - h), BAR_WIDTH, h).fill(rgb(ACCENT)));
        }
        root = root.child(card);
    }
    root.child(table(d))
        .child(panel(at(el(), LOG_ORIGIN, LOG.0, LOG.1), 8.0).child(inside(
            label(d.work.visible_log(), 11.0, LOG_LINE_HEIGHT).color(rgb(MUTED)),
            LOG_TEXT,
            LOG.0 - 24.0,
            LOG_LINES as f32 * LOG_LINE_HEIGHT,
        )))
}

fn table(d: &Dashboard) -> El {
    let (live, scroll) = (d.work.live, d.work.scroll);
    let mut header = inside(el(), (0.0, 0.0), TABLE.0, TABLE_HEADER).fill(rgb(HEADER));
    for (x, title) in COLUMNS {
        header = header.child(at(
            label(title, 11.0, 14.0).color(rgb(MUTED)),
            (x, 7.0),
            80.0,
            14.0,
        ));
    }
    let mut body = inside(el(), (0.0, TABLE_HEADER), TABLE.0, TABLE_VIEWPORT).clip_rounded(0.0);
    for row in d.work.row_range() {
        let (status, color) = status(row, live);
        let cell = |s: String, column: usize, w: f32| {
            at(label(s, 12.0, 16.0), (COLUMNS[column].0, CELL_TOP), w, 16.0)
        };
        body = body.child(
            at(
                el().key(row_id(row)),
                (0.0, row as f32 * TABLE_ROW - scroll),
                TABLE.0,
                TABLE_ROW,
            )
            .fill(rgb(row_color(row)))
            .child(cell(row_id(row), 0, 70.0).color(rgb(MUTED)))
            .child(cell(row_worker(row), 1, 150.0))
            .child(
                at(el(), (PILL.0, PILL.1), PILL.2, PILL.3)
                    .radius(8.0)
                    .fill(rgb(color))
                    .child(at(label(status, 10.0, 14.0), PILL_TEXT, 56.0, 14.0)),
            )
            .child(cell(row_latency(row, live), 3, 70.0))
            .child(cell(row_throughput(row, live), 4, 80.0)),
        );
    }
    panel(at(el(), TABLE_ORIGIN, TABLE.0, TABLE.1), 8.0)
        .clip_rounded(8.0)
        .child(header)
        .child(body)
}

fn tick(d: &mut Dashboard, tx: &mut Tx) -> bool {
    d.work.tick();
    tx.instant();
    true
}

fn main() {
    let work = Heavy::from_env();
    let ticks = Ticks {
        seconds: work.seconds,
        active: work.mode != Mode::Idle,
        turn: Heavy::turn,
        tick,
        report: |d| format!("workload_ticks={}", d.work.frames),
    };
    zgx_compare_rune::run(
        AppSpec::new(
            "Rune heavy",
            (WIDTH, HEIGHT),
            || Dashboard {
                work: Heavy::from_env(),
            },
            view,
        ),
        Some(ticks),
    );
}
