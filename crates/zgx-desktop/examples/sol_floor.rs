//! Scratch (not shipped): heavy's measured floor, in stream mode. The same
//! pixels as zgx draws, through the same direct Metal submit, from a
//! program that knows heavy's layout: one full frame captured from zgx,
//! then each tick only the values that change are written into its
//! instances (bars, progress fills, pill colours, and the glyphs of texts
//! whose string changed, shaped through the word cache) and submitted.
//!
//! MODE=floor (default) or MODE=zgx; FRAMES=n; DUMP=path writes the last
//! frame's pixels, to compare the two.
use zgx::scene::{Color, NodeKind, Rect};
use zgx::widgets::Ui;
use zgx_gpu::GpuRenderer;
use zgx_workload::heavy::*;

#[path = "support/heavy_view.rs"]
mod heavy_view;

unsafe extern "C" {
    fn thread_selfcounts(kind: i32, buf: *mut u64, size: usize) -> i32;
}
fn instr() -> u64 {
    let mut b = [0u64; 2];
    unsafe { thread_selfcounts(1, b.as_mut_ptr(), 16) };
    b[0]
}
fn env<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
/// Panels' 1-pixel border: what they hold is laid out inside it.
const IN: f32 = 1.;
fn rgb(value: u32) -> Color {
    Color((value >> 16) as u8, (value >> 8) as u8, value as u8, 255)
}

/// What a text slot shows, worked out from the model each tick.
#[derive(Clone, Copy)]
enum Value {
    Tile(usize),
    Card(usize),
    Latency(usize),
    Throughput(usize),
    Status(usize),
    Log,
}

fn main() {
    let frames: usize = env("FRAMES", 1200);
    let warmup = 120.min(frames / 2);
    let mode: String = env("MODE", "floor".to_string());
    let scale = 2.;
    let mut gpu = GpuRenderer::new((WIDTH * scale) as u32, (HEIGHT * scale) as u32).expect("GPU");
    gpu.set_scale_factor(scale);
    let mut ui = Ui::new(WIDTH, HEIGHT);
    gpu.install_text(&mut ui.scene.borrow_mut());
    // SAFETY: before any other thread reads the environment.
    unsafe { std::env::set_var("ZGX_MODE", "stream") };
    let mut work = Heavy::from_env();
    let live = ui.signal(0_u64);
    let scroll = ui.signal(0_f32);
    let log = ui.signal(work.visible_log());
    ui.mount(heavy_view::dashboard(
        live.clone(),
        scroll.clone(),
        log.clone(),
    ));
    ui.prepare_frame();
    let report = ui.scene.borrow_mut().flush();
    // Phases: model (the app's strings and numbers), text (shaping, layout,
    // glyphs), patch (rectangles and colours) and submit; for zgx, its own.
    let mut phase = [0u64; 4];
    let k = |v: u64| v as f64 / (frames - warmup) as f64 / 1e3;
    if mode == "zgx" {
        gpu.render(&ui.scene.borrow(), &report.damage).unwrap();
        let runtime = ui.runtime.clone();
        for frame in 1..frames {
            let i0 = instr();
            work.tick();
            runtime.batch(|| {
                live.set(work.live);
                log.set(work.visible_log());
            });
            let i1 = instr();
            ui.prepare_frame();
            let i2 = instr();
            let report = ui.scene.borrow_mut().flush();
            let i3 = instr();
            gpu.render(&ui.scene.borrow(), &report.damage).unwrap();
            let i4 = instr();
            if frame >= warmup {
                for (p, d) in phase.iter_mut().zip([i1 - i0, i2 - i1, i3 - i2, i4 - i3]) {
                    *p += d;
                }
            }
        }
        if std::env::var_os("FLOOR_DEBUG").is_some() {
            let f = gpu.floor_capture(&ui.scene.borrow()).unwrap();
            let (cx, cy) = card_position(0);
            for i in f.find(|r, _| {
                r[0] >= cx && r[0] < cx + 20. && r[1] >= cy + 44. && r[1] < cy + CARD.1
            }) {
                println!("zgx {i}: {}", f.debug(i));
            }
        }
        println!(
            "zgx heavy stream: K instr/frame: update {:.1} prepare {:.1} flush {:.1} render {:.1} = {:.1}",
            k(phase[0]),
            k(phase[1]),
            k(phase[2]),
            k(phase[3]),
            k(phase.iter().sum())
        );
    } else {
        let mut floor = gpu.floor_capture(&ui.scene.borrow()).expect("capture");
        // The texts that change, found by where heavy puts them.
        let near = |a: f32, b: f32| (a - b).abs() < 0.01;
        let mut texts: Vec<(Rect, zgx::text_layout::FontStyle, f32, Value)> = Vec::new();
        let rows = row_range(0.);
        for item in ui.scene.borrow().paint_items() {
            let NodeKind::Text { font_size, .. } = item.kind else {
                continue;
            };
            let (x, y) = (item.bounds.x, item.bounds.y);
            let at = |(ox, oy): (f32, f32)| near(x, ox) && near(y, oy);
            let mut value = (0..TILES)
                .find(|i| {
                    let p = tile_position(*i);
                    at((p.0 + IN + TILE_VALUE.0, p.1 + IN + TILE_VALUE.1))
                })
                .map(Value::Tile)
                .or_else(|| {
                    (0..CARDS)
                        .find(|i| {
                            let p = card_position(*i);
                            at((p.0 + IN + CARD_VALUE.0, p.1 + IN + CARD_VALUE.1))
                        })
                        .map(Value::Card)
                });
            for row in rows.clone() {
                let top = TABLE_ORIGIN.1 + IN + TABLE_HEADER + row as f32 * TABLE_ROW;
                let left = TABLE_ORIGIN.0 + IN;
                if at((left + COLUMNS[3].0, top + CELL_TOP)) {
                    value = Some(Value::Latency(row));
                } else if at((left + COLUMNS[4].0, top + CELL_TOP)) {
                    value = Some(Value::Throughput(row));
                } else if at((left + PILL.0 + PILL_TEXT.0, top + PILL.1 + PILL_TEXT.1)) {
                    value = Some(Value::Status(row));
                }
            }
            if at((
                LOG_ORIGIN.0 + IN + LOG_TEXT.0,
                LOG_ORIGIN.1 + IN + LOG_TEXT.1,
            )) {
                value = Some(Value::Log);
            }
            if let Some(value) = value {
                texts.push((item.bounds, item.font.clone(), *font_size, value));
            }
        }
        let mut slots = Vec::new();
        for (bounds, font, size, value) in &texts {
            let cap = match value {
                Value::Log => 720,
                _ => 24,
            };
            if let Some(slot) = floor.text_slot(*bounds, font, *size, cap, scale) {
                slots.push((slot, *value, String::new()));
            }
        }
        floor.finish();
        // Bars, progress fills and pills, by place and colour.
        let accent = zgx_gpu::floor::Floor::color(rgb(ACCENT));
        let mut bars = Vec::new();
        let mut fills = Vec::new();
        for card in 0..CARDS {
            let (cx, cy) = card_position(card);
            let (cx, cy) = (cx + IN, cy + IN);
            for bar in 0..BARS {
                let x = cx + bar_x(bar);
                let found = floor.find(|r, c| {
                    c == accent
                        && near(r[0], x)
                        && near(r[2], BAR_WIDTH)
                        && near(r[1] + r[3], cy + SPARK_BASE)
                });
                assert_eq!(found.len(), 1);
                bars.push((card, bar, found[0]));
            }
            let found = floor.find(|r, c| {
                c == accent && near(r[0], cx + PROGRESS.0) && near(r[1], cy + PROGRESS.1)
            });
            assert_eq!(found.len(), 1);
            fills.push((card, found[0]));
        }
        let mut pills = Vec::new();
        for row in rows.clone() {
            let top = TABLE_ORIGIN.1 + IN + TABLE_HEADER + row as f32 * TABLE_ROW;
            let found = floor.find(|r, _| {
                near(r[0], TABLE_ORIGIN.0 + IN + PILL.0)
                    && near(r[1], top + PILL.1)
                    && near(r[2], PILL.2)
                    && near(r[3], PILL.3)
            });
            if let Some(index) = found.first() {
                pills.push((row, *index, 0_u32));
            }
        }
        for &(_, _, index) in &bars {
            floor.moves(index);
        }
        for &(_, index) in &fills {
            floor.moves(index);
        }
        println!(
            "floor: {} text slots, {} bars, {} fills, {} pills",
            slots.len(),
            bars.len(),
            fills.len(),
            pills.len()
        );
        let mut strings: Vec<String> = Vec::new();
        let mut log_text = 0;
        for frame in 1..frames {
            let i0 = instr();
            if std::env::var_os("NOPATCH").is_some() {
                floor.submit(&mut gpu);
                continue;
            }
            work.tick();
            let t = work.live;
            strings.clear();
            for (_, value, _) in &slots {
                strings.push(match *value {
                    Value::Tile(i) => tile_value(i, t),
                    Value::Card(i) => card_value(i, t),
                    Value::Latency(row) => row_latency(row, t),
                    Value::Throughput(row) => row_throughput(row, t),
                    Value::Status(row) => status(row, t).0.to_owned(),
                    Value::Log => work.visible_log(),
                });
            }
            let i1 = instr();
            for ((slot, value, shown), string) in slots.iter_mut().zip(&strings) {
                if shown != string {
                    let l0 = instr();
                    floor.set_text(&mut gpu, *slot, string);
                    if matches!(value, Value::Log) && frame >= warmup {
                        log_text += instr() - l0;
                    }
                    shown.clone_from(string);
                }
            }
            let i2 = instr();
            for &(card, bar, index) in &bars {
                let height = bar_height(card, bar, t);
                let mut r = floor.rect(index);
                r[1] = (card_position(card).1 + IN) + (SPARK_BASE - height);
                r[3] = height;
                floor.set_rect(index, r);
            }
            for &(card, index) in &fills {
                let mut r = floor.rect(index);
                r[2] = card_progress(card, t);
                floor.set_rect(index, r);
            }
            for (row, index, shown) in &mut pills {
                let color = status(*row, t).1;
                if *shown != color {
                    floor.set_color(*index, rgb(color));
                    *shown = color;
                }
            }
            let i3 = instr();
            floor.submit(&mut gpu);
            let i4 = instr();
            if frame >= warmup {
                for (p, d) in phase.iter_mut().zip([i1 - i0, i2 - i1, i3 - i2, i4 - i3]) {
                    *p += d;
                }
            }
        }
        if std::env::var_os("FLOOR_DEBUG").is_some() {
            let (cx, cy) = card_position(0);
            for i in floor.find(|r, _| {
                r[0] >= cx && r[0] < cx + 20. && r[1] >= cy + 44. && r[1] < cy + CARD.1
            }) {
                println!("floor {i}: {}", floor.debug(i));
            }
        }
        println!("of text, the log's: {:.1}", k(log_text));
        println!(
            "floor heavy stream: K instr/frame: model {:.1} text {:.1} patch {:.1} submit {:.1} = {:.1}",
            k(phase[0]),
            k(phase[1]),
            k(phase[2]),
            k(phase[3]),
            k(phase.iter().sum())
        );
    }
    if let Ok(path) = std::env::var("DUMP") {
        std::fs::write(path, gpu.readback().unwrap()).unwrap();
    }
}
