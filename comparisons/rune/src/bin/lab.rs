//! The performance lab (see zgx_workload::Workload) in Rune, as the zgx, GPUI
//! Fast and MyGo adapters draw it: a header, three buttons over a list of
//! capacity four, the streaming pane and the rows in view of a 100,000-row
//! list (and two each side), placed by the scroll offset. Each tick changes
//! the state and applies at once (`Tx::instant`).
//!
//!     ZGX_MODE=both ZGX_SECONDS=10 bin/lab
use rune::tok::TextStyle;
use rune::{AppSpec, Cx, El, Tx, el, text};
use zgx_compare_rune::{Ticks, always, rgb};
use zgx_workload::{Mode, ROW_HEIGHT, Workload, row_label};

const SURFACE: u32 = 0x192230;
const STRIPE: u32 = 0x1d2838;
const BUTTON: u32 = 0x293b50;
const FG: u32 = 0xe5edf7;

struct Lab {
    work: Workload,
    items: Vec<i32>,
    done: i32,
}

fn at(e: El, x: f32, y: f32, w: f32, h: f32) -> El {
    e.absolute(x, y).size(w, h)
}
fn label(s: impl Into<String>) -> El {
    text(s)
        .font(TextStyle::new(14.0, 400.0).lh(20.0))
        .color(rgb(FG))
}

impl Lab {
    /// Three items, stopping at the fourth slot's limit (done -1), the
    /// earlier writes kept.
    fn append(&mut self) {
        self.done = 0;
        for value in [1, 2, 3] {
            if self.items.len() == 4 {
                self.done = -1;
                return;
            }
            self.items.push(value);
        }
    }
    /// Two equal writes to the first item.
    fn write(&mut self) {
        self.done = match self.items.first_mut() {
            Some(item) => {
                *item = 99;
                *item = 99;
                0
            }
            None => -1,
        };
    }
}

fn view(s: &Lab, _: &mut Cx<Lab>) -> El {
    let count = s.items.len().to_string();
    let button = |key: &'static str, label_text: String, x: f32| {
        at(el().key(key), x, 55.0, 50.0, 32.0)
            .fill(rgb(BUTTON))
            .child(at(label(label_text), 12.0, 6.0, 38.0, 20.0))
    };
    let mut list = at(el(), 20.0, 290.0, 920.0, zgx_workload::VIEWPORT_HEIGHT)
        .fill(rgb(SURFACE))
        .clip_rounded(0.0);
    for index in s.work.row_range() {
        list = list.child(
            at(
                el().key(index.to_string()),
                0.0,
                index as f32 * ROW_HEIGHT - s.work.scroll_offset,
                920.0,
                ROW_HEIGHT,
            )
            .fill(rgb(if index % 2 == 0 { SURFACE } else { STRIPE }))
            .child(at(label(row_label(index)), 8.0, 4.0, 904.0, 20.0)),
        );
    }
    el().size(zgx_workload::WIDTH, zgx_workload::HEIGHT)
        .fill(rgb(0x10141c))
        .child(at(
            text("zgx performance lab")
                .font(TextStyle::new(20.0, 400.0).lh(28.0))
                .color(rgb(FG)),
            20.0,
            20.0,
            920.0,
            28.0,
        ))
        .child(button("plus", "+".into(), 20.0).on_press(Lab::append))
        .child(button("equals", "=".into(), 80.0).on_press(Lab::write))
        .child(button("done", s.done.to_string(), 140.0).on_press(|s: &mut Lab| s.done = 999))
        .child(at(label(count.clone()), 220.0, 62.0, 50.0, 24.0))
        .child(at(label("stable"), 280.0, 62.0, 80.0, 24.0))
        .child(at(label(count), 370.0, 62.0, 50.0, 24.0))
        .child(
            at(el(), 20.0, 110.0, 920.0, 160.0)
                .fill(rgb(SURFACE))
                .clip_rounded(0.0)
                .child(at(label(s.work.visible_text()), 8.0, 8.0, 904.0, 144.0)),
        )
        .child(list)
}

fn tick(s: &mut Lab, tx: &mut Tx) -> bool {
    s.work.tick();
    tx.instant();
    true
}

fn main() {
    let work = Workload::from_env();
    let ticks = Ticks {
        seconds: work.seconds,
        active: work.mode != Mode::Idle,
        turn: always,
        tick,
        report: |s| format!("workload_ticks={}", s.work.frames),
    };
    zgx_compare_rune::run(
        AppSpec::new(
            "Rune performance lab",
            (zgx_workload::WIDTH, zgx_workload::HEIGHT),
            || Lab {
                work: Workload::from_env(),
                items: Vec::with_capacity(4),
                done: 0,
            },
            view,
        ),
        Some(ticks),
    );
}
