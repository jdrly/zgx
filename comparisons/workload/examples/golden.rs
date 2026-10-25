//! Every string and number the workloads give an adapter, over a run of
//! ticks, one per line: the golden file other languages' ports of this
//! crate check themselves against (comparisons/mygo/workload).
//!
//!     cargo run -p zgx-workload --example golden > comparisons/mygo/workload/testdata/golden.txt
use zgx_workload::{animation, heavy, row_label, vlist, Mode, Workload};

fn main() {
    let mut lab = Workload {
        text: String::new(),
        scroll_offset: 0.,
        frames: 0,
        mode: Mode::Both,
        seconds: 0.,
    };
    for tick in 0..1500 {
        lab.tick();
        if tick % 37 == 0 {
            println!("lab {tick} {:?} {:?}", lab.row_range(), lab.visible_text());
        }
    }
    for row in [0, 1, 99, 12_345, 99_999] {
        println!("label {row} {}", row_label(row));
    }
    let mut work = heavy::Heavy {
        frames: 0,
        live: 0,
        scroll: 0.,
        log: Default::default(),
        mode: Mode::Both,
        seconds: 0.,
    };
    for _ in 0..heavy::LOG_LINES {
        work.tick();
    }
    for tick in 0..900 {
        work.tick();
        let live = work.live;
        if tick % 29 != 0 {
            continue;
        }
        println!(
            "heavy {tick} live {live} scroll {} rows {:?}",
            work.scroll,
            work.row_range()
        );
        println!("log {:?}", work.visible_log());
        for tile in 0..heavy::TILES {
            println!(
                "tile {tile} {} {}",
                heavy::tile_label(tile),
                heavy::tile_value(tile, live)
            );
        }
        for card in [0, 7, 19, 29] {
            let bars: Vec<String> = (0..heavy::BARS)
                .map(|b| format!("{:.3}", heavy::bar_height(card, b, live)))
                .collect();
            println!(
                "card {card} {} {} {:.3} {:?} {}",
                heavy::card_title(card),
                heavy::card_value(card, live),
                heavy::card_progress(card, live),
                heavy::status(card, live),
                bars.join(",")
            );
        }
        for row in work.row_range().step_by(7) {
            println!(
                "row {row} {} {} {} {} {:?} {:06x}",
                heavy::row_id(row),
                heavy::row_worker(row),
                heavy::row_latency(row, live),
                heavy::row_throughput(row, live),
                heavy::status(row, live),
                heavy::row_color(row)
            );
        }
    }
    for frame in 0..240 {
        let seconds = frame as f32 / 120.;
        let dots: Vec<String> = (0..animation::COLUMNS * animation::ROWS)
            .step_by(5)
            .map(|i| format!("{:.4}", animation::dot_opacity(i, seconds)))
            .collect();
        let count = animation::SHIMMER.chars().count();
        let shimmer: Vec<String> = (0..count)
            .step_by(3)
            .map(|i| animation::shimmer_alpha(i, count, seconds).to_string())
            .collect();
        println!("animate {frame} {} {}", dots.join(","), shimmer.join(","));
    }
    for row in (0..40).chain([999, 54_321, 99_999]) {
        println!(
            "vlist {row} {} {} {} {} {:?}",
            vlist::lines(row),
            vlist::has_image(row),
            vlist::author(row),
            vlist::time(row),
            vlist::text(row)
        );
    }
    for tick in 0..200 {
        println!(
            "vtick {tick} {} {} {} {:?}",
            vlist::delta(vlist::Mode::Scroll, tick),
            vlist::delta(vlist::Mode::Fling, tick),
            vlist::delta(vlist::Mode::Jump, tick),
            vlist::jump(tick)
        );
    }
}
