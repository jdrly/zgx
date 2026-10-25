//! Windows: an opaque window's swap chain is opaque, so the compositor
//! copies its frames rather than blending them over what lies behind (7 to 14%
//! of its GPU time a frame); a transparent window's keeps its alpha. The swap
//! chain is made at the first frame, after the background is set.
#![cfg(windows)]
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
use winit::platform::windows::{EventLoopBuilderExtWindows, WindowAttributesExtWindows};
use winit::window::{Window, WindowId};
use zgx::scene::{Color, Rect, Scene};
use zgx_gpu::GpuRenderer;

#[derive(Default)]
struct App {
    /// Whether each window is transparent, and whether its swap chain
    /// presents opaque before its first frame and after.
    modes: Vec<(bool, Option<bool>, Option<bool>)>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let mut context = None;
        for transparent in [false, true] {
            // As zgx-desktop makes its windows (`window_info`).
            let attributes = Window::default_attributes()
                .with_title("zgx opaque swap chain test")
                .with_inner_size(winit::dpi::PhysicalSize::new(200, 120))
                .with_transparent(transparent)
                .with_no_redirection_bitmap(true);
            let window = Arc::new(event_loop.create_window(attributes).unwrap());
            let mut renderer = match &context {
                Some(context) => GpuRenderer::for_window_with_context(window, context),
                None => GpuRenderer::for_window(window),
            }
            .unwrap();
            context.get_or_insert_with(|| renderer.context());
            renderer.set_background(if transparent {
                Color(0, 0, 0, 0)
            } else {
                Color(30, 30, 30, 255)
            });
            let before = renderer.presents_opaque();
            let scene = Scene::new(200., 120.);
            renderer
                .render(&scene, &[Rect::new(0., 0., 200., 120.)])
                .unwrap();
            renderer.present().unwrap();
            self.modes
                .push((transparent, before, renderer.presents_opaque()));
        }
        event_loop.exit();
    }
    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

#[test]
fn an_opaque_windows_swap_chain_is_opaque_and_a_transparent_ones_is_not() {
    let mut event_loop = EventLoop::builder().with_any_thread(true).build().unwrap();
    let mut app = App::default();
    event_loop.run_app_on_demand(&mut app).unwrap();
    assert_eq!(
        app.modes,
        [(false, None, Some(true)), (true, None, Some(false))],
        "(transparent, opaque before the first frame, after)"
    );
}
