//! Frames presented through a window's own surfaces (`LayerPresent`), each
//! brought up to date from the one shown around what a frame draws: the
//! regions a frame draws are drawn whole, never through a stencil of the
//! damage inside them (which left the rest of each region as the surface
//! held it frames before: stale patches in windows).
//!
//! Its own binary without the test harness: layer presentation runs on the
//! main thread, which the harness keeps for itself.
#[cfg(target_os = "macos")]
use zgx::{compose::prelude::*, reactive::Signal, scene::Color, widgets::Ui};
#[cfg(target_os = "macos")]
use zgx_gpu::GpuRenderer;

#[cfg(target_os = "macos")]
const SIZE: (f32, f32) = (640., 400.);

#[cfg(target_os = "macos")]
/// Cells all over the window, a few changing each tick: damage scattered
/// over most of it, which a frame draws through a stencil of it.
fn view(live: Signal<u32>) -> View {
    div()
        .size(SIZE.0, SIZE.1)
        .bg(Color(14, 18, 24, 255))
        .children((0..48u32).map(move |cell| {
            let (bar, fill) = (live.clone(), live.clone());
            let (x, y) = ((cell % 8) as f32 * 80., (cell / 8) as f32 * 66.);
            div()
                .absolute()
                .ml(x + 4.)
                .mt(y + 4.)
                .size(72., 58.)
                .bg(Color(30, 40, 54, 255))
                .child(
                    div()
                        .absolute()
                        .ml(6.)
                        .w(10.)
                        .reactive_style(move || {
                            let on = (bar.get() + cell * 5) % 7 == 0;
                            let height = if on { 40. } else { 12. + (cell % 5) as f32 };
                            Styles::new().mt(52. - height).h(height)
                        })
                        .bg(Color(94, 177, 255, 255)),
                )
                .child(
                    div()
                        .absolute()
                        .ml(24.)
                        .mt(20.)
                        .size(30., 16.)
                        .reactive_style(move || {
                            let on = (fill.get() + cell * 3) % 11 == 0;
                            Styles::new().bg(if on {
                                Color(200, 80, 60, 255)
                            } else {
                                Color(40, 120, 80, 255)
                            })
                        }),
                )
        }))
}

#[cfg(target_os = "macos")]
struct Window {
    ui: Ui,
    gpu: GpuRenderer,
    live: Signal<u32>,
}
#[cfg(target_os = "macos")]
impl Window {
    fn new(live: u32) -> Self {
        let host = objc2_quartz_core::CALayer::new();
        let mut gpu =
            GpuRenderer::debug_for_layer(host, (SIZE.0 * 2.) as u32, (SIZE.1 * 2.) as u32).unwrap();
        gpu.set_scale_factor(2.);
        let mut ui = Ui::new(SIZE.0, SIZE.1);
        gpu.install_text(&mut ui.scene.borrow_mut());
        let state = ui.signal(live);
        ui.mount(view(state.clone()));
        Window {
            ui,
            gpu,
            live: state,
        }
    }
    fn show(&mut self) -> Vec<u8> {
        self.ui.prepare_frame();
        let damage = self.ui.scene.borrow_mut().flush().damage;
        self.gpu.render(&self.ui.scene.borrow(), &damage).unwrap();
        self.gpu.present().unwrap();
        self.gpu.shown_pixels().expect("a surface shown").2
    }
}

#[cfg(target_os = "macos")]
fn scattered_damage_presented_through_surfaces_draws_whole_regions() {
    // The same frames through surfaces, one renderer allowed to draw
    // through a stencil of the damage and one not.
    let (mut stenciled, mut plain) = (Window::new(1), Window::new(1));
    plain.gpu.debug_stencil(false);
    assert!(stenciled.show() == plain.show(), "the first frame differs");
    for live in 2..16 {
        stenciled.live.set(live);
        plain.live.set(live);
        assert!(
            stenciled.show() == plain.show(),
            "frame {live}: the surface shown differs from one drawn without a stencil"
        );
    }
    // Presenting through surfaces, no frame is drawn through a stencil:
    // each surface is brought up to date around the regions a frame draws,
    // whole. (A headless renderer, drawing into one target, does stamp.)
    assert_eq!(stenciled.gpu.debug_stamped_frames(), 0, "a frame stamped");
}

fn main() {
    #[cfg(target_os = "macos")]
    {
        scattered_damage_presented_through_surfaces_draws_whole_regions();
        println!("layer_frames: scattered damage through surfaces draws whole regions");
    }
}
