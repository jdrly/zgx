//! A window whose leaves alone change (bars resizing, values changing) is
//! drawn by patching those items into its last whole frame (`render_kept`):
//! each such frame is pixel for pixel what drawing the scene afresh draws.
use zgx::{compose::prelude::*, reactive::Signal, scene::Color, widgets::Ui};
use zgx_gpu::{GpuRenderer, text::FontData};

const GEIST: &[u8] = include_bytes!("fonts/GeistVariable-subset.ttf");
const SIZE: (f32, f32) = (480., 260.);

fn view(live: Signal<u32>, mounted: Signal<u32>) -> View {
    let rows = live.clone();
    let card = move |index: u32| {
        let (value, bars, fill) = (live.clone(), live.clone(), live.clone());
        div()
            .absolute()
            .ml(8. + 116. * index as f32)
            .mt(8.)
            .size(108., 120.)
            .rounded(6.)
            .border(1.)
            .border_color(Color(40, 50, 64, 255))
            .bg(Color(22, 28, 38, 255))
            .child(
                text_signal(move || format!("{:.1} ms", 3.7 * (value.get() + index) as f32))
                    .text_size(15.)
                    .absolute()
                    .ml(8.)
                    .mt(6.)
                    .size(92., 20.),
            )
            .child(
                div()
                    .absolute()
                    .ml(8.)
                    .mt(32.)
                    .h(4.)
                    .rounded(2.)
                    .bg(Color(94, 177, 255, 255))
                    .reactive_style(move || {
                        Styles::new().w(8. + ((fill.get() * 7 + index) % 13) as f32 * 6.3)
                    }),
            )
            .children((0..12).map(move |bar| {
                let bars = bars.clone();
                div()
                    .absolute()
                    .ml(8. + 7.5 * bar as f32)
                    .w(5.)
                    .bg(Color(94, 177, 255, 255))
                    .reactive_style(move || {
                        let height = 2. + ((bars.get() + bar + index) % 9) as f32 * 7.25;
                        Styles::new().mt(110. - height).h(height)
                    })
            }))
    };
    let row = move |row: u32| {
        let (status, label, latency) = (rows.clone(), rows.clone(), rows.clone());
        div()
            .absolute()
            .ml(8.)
            .mt(140. + 22. * row as f32)
            .size(464., 22.)
            .bg(if row.is_multiple_of(2) {
                Color(22, 28, 38, 255)
            } else {
                Color(25, 32, 43, 255)
            })
            .child(
                text(format!("worker-{row:03}"))
                    .text_size(12.)
                    .absolute()
                    .ml(6.)
                    .mt(3.)
                    .size(120., 16.),
            )
            .child(
                div()
                    .absolute()
                    .ml(140.)
                    .mt(3.)
                    .size(60., 16.)
                    .rounded(8.)
                    .reactive_style(move || {
                        let down = (status.get() / 4 + row).is_multiple_of(3);
                        Styles::new().bg(if down {
                            Color(138, 34, 48, 255)
                        } else {
                            Color(29, 107, 74, 255)
                        })
                    })
                    .child(
                        text_signal(move || {
                            ["down", "healthy", "healthy"][((label.get() / 4 + row) % 3) as usize]
                                .into()
                        })
                        .text_size(10.)
                        .absolute()
                        .ml(8.)
                        .mt(1.)
                        .size(48., 14.),
                    ),
            )
            .child(
                text_signal(move || format!("{} ms", (latency.get() * 13 + row * 7) % 997))
                    .text_size(12.)
                    .absolute()
                    .ml(220.)
                    .mt(3.)
                    .size(80., 16.),
            )
    };
    div()
        .size(SIZE.0, SIZE.1)
        .bg(Color(14, 18, 24, 255))
        .text_color(Color(229, 237, 247, 255))
        .font_family("Geist")
        .children((0..4).map(card))
        .children((0..5).map(row))
        // Mounted again whenever `mounted` changes: a change no patch makes.
        .child(switch(
            move || mounted.get(),
            |which, _| {
                div()
                    .absolute()
                    .ml(8. + 40. * which as f32)
                    .mt(252.)
                    .size(100., 6.)
                    .bg(Color(255, 196, 61, 255))
            },
        ))
}

struct Window {
    ui: Ui,
    gpu: GpuRenderer,
    live: Signal<u32>,
    mounted: Signal<u32>,
}
impl Window {
    fn new(live: u32) -> Self {
        Self::mounted(live, 0)
    }
    fn mounted(live: u32, mounted: u32) -> Self {
        let gpu = GpuRenderer::new((SIZE.0 * 2.) as u32, (SIZE.1 * 2.) as u32).unwrap();
        let mut gpu = gpu;
        gpu.set_scale_factor(2.);
        FontData::new(GEIST.to_vec())
            .unwrap()
            .install(&mut gpu.text_system().borrow_mut());
        let mut ui = Ui::new(SIZE.0, SIZE.1);
        gpu.install_text(&mut ui.scene.borrow_mut());
        let state = ui.signal(live);
        let which = ui.signal(mounted);
        ui.mount(view(state.clone(), which.clone()));
        Window {
            ui,
            gpu,
            live: state,
            mounted: which,
        }
    }
    fn draw(&mut self) -> zgx_gpu::GpuStats {
        self.ui.prepare_frame();
        let report = self.ui.scene.borrow_mut().flush();
        self.gpu
            .render(&self.ui.scene.borrow(), &report.damage)
            .unwrap()
    }
}

#[test]
fn a_frame_patched_into_the_kept_window_is_the_frame_drawn_afresh() {
    let mut window = Window::new(1);
    window.draw();
    let mut patched = 0;
    for live in 2..14 {
        window.live.set(live);
        let stats = window.draw();
        patched += usize::from(stats.patched_items > 0);
        let mut fresh = Window::new(live);
        fresh.draw();
        assert!(
            window.gpu.readback().unwrap() == fresh.gpu.readback().unwrap(),
            "frame {live} differs from the scene drawn afresh (patched {})",
            stats.patched_items
        );
    }
    // Every frame but those where a pill (a box holding a text) changes
    // colour patches; drawn whole then, the next patches again.
    assert!(patched >= 8, "{patched} frames patched");
}

/// A window that lost its kept frame to a change no patch makes (a node
/// mounted) before patching from it keeps one again once a few frames in a
/// row repaint leaves alone: patching goes on, each frame as drawn afresh.
#[test]
fn a_window_keeps_a_frame_again_after_a_change_no_patch_makes() {
    let mut window = Window::new(1);
    window.draw();
    window.mounted.set(1);
    window.live.set(2);
    window.draw();
    let mut patched = 0;
    for live in 3..14 {
        window.live.set(live);
        let stats = window.draw();
        patched += usize::from(stats.patched_items > 0);
        let mut fresh = Window::mounted(live, 1);
        fresh.draw();
        assert!(
            window.gpu.readback().unwrap() == fresh.gpu.readback().unwrap(),
            "frame {live} differs from the scene drawn afresh (patched {})",
            stats.patched_items
        );
    }
    assert!(patched >= 5, "{patched} frames patched");
}

/// Boxes a frame patches alone (`patch_box`): a flat colour, translucent,
/// edge-faded, bordered and rounded, and one sliding across a clip's edge
/// (inside it, cut by it, then outside), each frame as drawn afresh.
fn boxes(live: Signal<u32>) -> View {
    let shade = live.clone();
    let tint = live.clone();
    let slide = live.clone();
    let border = live.clone();
    div()
        .size(SIZE.0, SIZE.1)
        .bg(Color(14, 18, 24, 255))
        .child(
            div()
                .absolute()
                .ml(10.)
                .mt(10.)
                .size(60., 40.)
                .reactive_style(move || {
                    Styles::new().bg(Color((shade.get() * 37 % 256) as u8, 90, 140, 255))
                }),
        )
        .child(
            div()
                .absolute()
                .ml(80.)
                .mt(10.)
                .size(60., 40.)
                .rounded(9.)
                .opacity(0.6)
                .reactive_style(move || {
                    Styles::new().bg(Color(200, (tint.get() * 53 % 256) as u8, 60, 255))
                }),
        )
        .child(
            div()
                .absolute()
                .ml(150.)
                .mt(10.)
                .size(70., 44.)
                .edge_fade(6.)
                .bg(Color(90, 200, 120, 255))
                .reactive_style(move || Styles::new().w(40. + (border.get() % 5) as f32 * 7.5)),
        )
        .child(
            div()
                .absolute()
                .ml(230.)
                .mt(10.)
                .size(90., 60.)
                .rounded(4.)
                .border(2.)
                .border_color(Color(250, 250, 250, 255))
                .bg(Color(40, 60, 90, 255))
                .reactive_style({
                    let live = live.clone();
                    move || Styles::new().h(30. + (live.get() % 4) as f32 * 9.5)
                }),
        )
        .child(
            div()
                .absolute()
                .ml(20.)
                .mt(100.)
                .size(200., 60.)
                .overflow_hidden()
                .bg(Color(30, 30, 30, 255))
                .child(
                    div()
                        .absolute()
                        .mt(10.)
                        .size(50., 30.)
                        .bg(Color(255, 196, 61, 255))
                        .reactive_style(move || {
                            Styles::new().ml(-70. + (slide.get() % 12) as f32 * 30.5)
                        }),
                ),
        )
}

#[test]
fn a_box_patched_alone_is_the_box_drawn_afresh() {
    let window = |live: u32| {
        let mut gpu = GpuRenderer::new((SIZE.0 * 2.) as u32, (SIZE.1 * 2.) as u32).unwrap();
        gpu.set_scale_factor(2.);
        let mut ui = Ui::new(SIZE.0, SIZE.1);
        gpu.install_text(&mut ui.scene.borrow_mut());
        let state = ui.signal(live);
        ui.mount(boxes(state.clone()));
        (ui, gpu, state)
    };
    let draw = |ui: &mut Ui, gpu: &mut GpuRenderer| {
        ui.prepare_frame();
        let report = ui.scene.borrow_mut().flush();
        gpu.render(&ui.scene.borrow(), &report.damage).unwrap()
    };
    let (mut ui, mut gpu, live) = window(1);
    draw(&mut ui, &mut gpu);
    let mut patched = 0;
    for value in 2..26 {
        live.set(value);
        let stats = draw(&mut ui, &mut gpu);
        patched += usize::from(stats.patched_items > 0);
        let (mut fresh, mut fresh_gpu, _) = window(value);
        draw(&mut fresh, &mut fresh_gpu);
        assert!(
            gpu.readback().unwrap() == fresh_gpu.readback().unwrap(),
            "frame {value} differs from the boxes drawn afresh (patched {})",
            stats.patched_items
        );
    }
    assert!(patched >= 16, "{patched} frames patched");
}
