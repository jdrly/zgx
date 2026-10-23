//! Popups past the window's edges, as AppKit shows menus and popovers.
//!
//! An anchored popup (popover, menu, submenu, context menu, tooltip) is a
//! popup root of its window's scene (`Scene::set_popup`). It stays in that
//! scene, with its input, focus and dismissal. The window's renderer leaves
//! it out, and a borderless, transparent, non-activating child panel shows
//! it instead, drawn by a renderer of its own (`GpuRenderer::set_popup`)
//! that shares the window's device and text caches. The parent window stays
//! key, so keys reach the popup's content through the window's focus. The
//! panel's pointer input goes to the window's host, in window coordinates.
//!
//! Panels are pooled per process and reused hidden: making a window and first
//! ordering it front costs the window server about 100 ms, and a menu must
//! open within a frame. A hidden panel draws nothing and never wakes.
//!
//! Elsewhere, popups stay inside their window (the host gives no screens to
//! show them on, `Ui::set_popup_screens`).
//! todo(windows): owned `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW` popup windows
//! with `WS_EX_LAYERED` alpha, shown with `SW_SHOWNOACTIVATE`.
//! todo(x11): override-redirect ARGB windows, with pointer grabs for dismissal.
//! todo(wayland): `xdg_popup` with `xdg_positioner` (the compositor places and
//! dismisses it), which winit does not expose.
use winit::event::{MouseButton, MouseScrollDelta, TouchPhase};

/// Pointer input over a popup window, in its parent window's coordinates.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) enum PopupInput {
    Moved {
        x: f32,
        y: f32,
    },
    Button {
        x: f32,
        y: f32,
        button: MouseButton,
        pressed: bool,
    },
    Wheel {
        x: f32,
        y: f32,
        delta: MouseScrollDelta,
        phase: TouchPhase,
    },
    /// Out of the popup, and not into its parent window.
    Left,
    /// A press elsewhere in the parent window (its title bar): popups close.
    Dismiss,
}

#[cfg(target_os = "macos")]
pub(crate) use macos::{
    PopupHost, content_origin, popup_screens, prewarm, window_at, window_number,
};

#[cfg(not(target_os = "macos"))]
pub(crate) use other::{
    PopupHost, content_origin, popup_screens, prewarm, window_at, window_number,
};

#[cfg(not(target_os = "macos"))]
mod other {
    use super::PopupInput;
    use zgui::scene::{Rect, Scene};
    /// Popups stay inside their window here (see the module's notes).
    #[derive(Default)]
    pub(crate) struct PopupHost;
    impl PopupHost {
        pub fn new(_: impl Fn(PopupInput) + 'static) -> Self {
            Self
        }
        #[allow(clippy::too_many_arguments)]
        pub fn update(
            &mut self,
            _: &winit::window::Window,
            _: &Scene,
            _: &zgui::semantics::Semantics,
            _: &zgui_gpu::GpuContext,
            _: f32,
            _: &[Rect],
            _: &[Rect],
            _: bool,
            _: Option<&zgui::input::DragState>,
            _: Option<zgui::input::DragEnded>,
        ) -> Result<(), zgui_gpu::GpuError> {
            Ok(())
        }
        pub fn follow(&mut self, _: &winit::window::Window, _: &zgui::input::DragState) {}
        pub fn preview_image(
            &self,
            _: &zgui::input::DragState,
        ) -> Option<crate::native_drag::DragImage> {
            None
        }
        pub fn show_preview(
            &mut self,
            _: &winit::window::Window,
            _: &zgui::input::DragState,
            _: Option<(f32, f32)>,
        ) {
        }
        pub fn hide_all(&mut self, _: &winit::window::Window) {}
    }
    pub(crate) fn popup_screens(_: &winit::window::Window) -> Vec<Rect> {
        Vec::new()
    }
    // todo: the pointer's window across the app's windows (drags between
    // them): Windows' WindowFromPoint; X11's XQueryPointer; Wayland gives a
    // drag's position only to the window under it.
    pub(crate) fn content_origin(_: &winit::window::Window) -> Option<(f64, f64)> {
        None
    }
    pub(crate) fn window_number(_: &winit::window::Window) -> Option<isize> {
        None
    }
    pub(crate) fn window_at(_: f64, _: f64) -> Option<isize> {
        None
    }
    pub(crate) fn prewarm(_: &zgui_gpu::GpuContext, _: f32) {}
}

#[cfg(target_os = "macos")]
mod macos {
    use super::PopupInput;
    use objc2::{
        DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained,
        runtime::AnyObject,
    };
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSBackingStoreType, NSColor, NSEvent, NSEventPhase, NSPanel,
        NSPopUpMenuWindowLevel, NSTrackingArea, NSTrackingAreaOptions, NSView,
        NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState,
        NSVisualEffectView, NSWindow, NSWindowAnimationBehavior, NSWindowCollectionBehavior,
        NSWindowOrderingMode, NSWindowStyleMask,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
    use objc2_quartz_core::{CALayer, CATransaction};
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };
    use winit::{
        dpi::{LogicalPosition, PhysicalPosition},
        event::{MouseButton, MouseScrollDelta, TouchPhase},
    };
    use zgui::{
        input::{DragEnded, DragState},
        scene::{NodeId, NodeKind, Rect, Scene},
        semantics::{Role, Semantics},
    };
    use zgui_gpu::{GpuContext, GpuError, GpuRenderer};

    /// How many panels stay made while hidden: a menu, its submenu and a
    /// tooltip open together without making one. `ZGUI_POPUP_POOL` sets
    /// another number (to measure what each costs).
    fn pool_size() -> usize {
        thread_local! {
            static SIZE: usize = std::env::var("ZGUI_POPUP_POOL")
                .ok()
                .and_then(|size| size.parse().ok())
                .unwrap_or(3);
        }
        SIZE.with(|size| *size)
    }

    /// What hands a popup's input to its parent window's host.
    type Sink = Rc<dyn Fn(PopupInput)>;
    /// Where a panel's input goes: its popup's origin in its parent window,
    /// and the parent's host.
    #[derive(Default)]
    pub(crate) struct Target {
        origin: Cell<(f32, f32)>,
        send: RefCell<Option<Sink>>,
        parent: RefCell<Option<Retained<NSWindow>>>,
    }
    define_class!(
        #[unsafe(super(NSView))]
        #[thread_kind = MainThreadOnly]
        #[name = "ZguiPopupView"]
        #[ivars = Target]
        pub(crate) struct PopupView;
        impl PopupView {
            #[unsafe(method(isFlipped))]
            fn is_flipped(&self) -> bool {
                true
            }
            // The first click on a popup acts, as on a menu.
            #[unsafe(method(acceptsFirstMouse:))]
            fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
                true
            }
            #[unsafe(method(mouseDown:))]
            fn mouse_down(&self, event: &NSEvent) {
                self.button(event, MouseButton::Left, true);
            }
            #[unsafe(method(mouseUp:))]
            fn mouse_up(&self, event: &NSEvent) {
                self.button(event, MouseButton::Left, false);
            }
            #[unsafe(method(rightMouseDown:))]
            fn right_mouse_down(&self, event: &NSEvent) {
                self.button(event, MouseButton::Right, true);
            }
            #[unsafe(method(rightMouseUp:))]
            fn right_mouse_up(&self, event: &NSEvent) {
                self.button(event, MouseButton::Right, false);
            }
            #[unsafe(method(otherMouseDown:))]
            fn other_mouse_down(&self, event: &NSEvent) {
                self.button(event, other_button(event), true);
            }
            #[unsafe(method(otherMouseUp:))]
            fn other_mouse_up(&self, event: &NSEvent) {
                self.button(event, other_button(event), false);
            }
            #[unsafe(method(mouseMoved:))]
            fn mouse_moved(&self, event: &NSEvent) {
                self.moved(event);
            }
            #[unsafe(method(mouseDragged:))]
            fn mouse_dragged(&self, event: &NSEvent) {
                self.moved(event);
            }
            #[unsafe(method(rightMouseDragged:))]
            fn right_mouse_dragged(&self, event: &NSEvent) {
                self.moved(event);
            }
            #[unsafe(method(otherMouseDragged:))]
            fn other_mouse_dragged(&self, event: &NSEvent) {
                self.moved(event);
            }
            #[unsafe(method(mouseExited:))]
            fn mouse_exited(&self, event: &NSEvent) {
                self.exited(event);
            }
            #[unsafe(method(scrollWheel:))]
            fn scroll_wheel(&self, event: &NSEvent) {
                self.wheel(event);
            }
        }
    );
    fn other_button(event: &NSEvent) -> MouseButton {
        match event.buttonNumber() {
            2 => MouseButton::Middle,
            3 => MouseButton::Back,
            4 => MouseButton::Forward,
            n => MouseButton::Other(n as u16),
        }
    }
    impl PopupView {
        fn send(&self, input: PopupInput) {
            let send = self.ivars().send.borrow().clone();
            if let Some(send) = send {
                send(input);
            }
        }
        /// `event`'s point in the parent window's coordinates.
        fn point(&self, event: &NSEvent) -> (f32, f32) {
            let point = self.convertPoint_fromView(event.locationInWindow(), None);
            let (x, y) = self.ivars().origin.get();
            (x + point.x as f32, y + point.y as f32)
        }
        fn button(&self, event: &NSEvent, button: MouseButton, pressed: bool) {
            let (x, y) = self.point(event);
            self.send(PopupInput::Button {
                x,
                y,
                button,
                pressed,
            });
        }
        fn moved(&self, event: &NSEvent) {
            let (x, y) = self.point(event);
            self.send(PopupInput::Moved { x, y });
        }
        fn exited(&self, event: &NSEvent) {
            // Into the parent window, its own events take over (a leave
            // posted after them would clear what they hover).
            let parent = self.ivars().parent.borrow().clone();
            if let Some(parent) = parent {
                let screen = NSEvent::mouseLocation();
                if let Some(content) = parent.contentView() {
                    let frame = parent
                        .convertRectToScreen(content.convertRect_toView(content.bounds(), None));
                    let inside = screen.x >= frame.origin.x
                        && screen.y >= frame.origin.y
                        && screen.x < frame.origin.x + frame.size.width
                        && screen.y < frame.origin.y + frame.size.height;
                    if inside {
                        return;
                    }
                }
            }
            let _ = event;
            self.send(PopupInput::Left);
        }
        fn wheel(&self, event: &NSEvent) {
            let (x, y) = self.point(event);
            let (dx, dy) = (event.scrollingDeltaX(), event.scrollingDeltaY());
            let scale = self
                .window()
                .map_or(1., |window| window.backingScaleFactor());
            // As winit reports the window's own.
            let delta = if event.hasPreciseScrollingDeltas() {
                let delta: PhysicalPosition<f64> = LogicalPosition::new(dx, dy).to_physical(scale);
                MouseScrollDelta::PixelDelta(delta)
            } else {
                MouseScrollDelta::LineDelta(dx as f32, dy as f32)
            };
            let phase = |phase: NSEventPhase| {
                if phase.contains(NSEventPhase::MayBegin) || phase.contains(NSEventPhase::Began) {
                    Some(TouchPhase::Started)
                } else if phase.contains(NSEventPhase::Ended) {
                    Some(TouchPhase::Ended)
                } else if phase.contains(NSEventPhase::Cancelled) {
                    Some(TouchPhase::Cancelled)
                } else {
                    None
                }
            };
            let phase = phase(event.momentumPhase())
                .or_else(|| phase(event.phase()))
                .unwrap_or(TouchPhase::Moved);
            self.send(PopupInput::Wheel { x, y, delta, phase });
        }
    }

    /// What shows behind a popup whose panel is translucent: AppKit's
    /// material for it, cut to the panel's rounded box, as menus and
    /// popovers are made of.
    #[derive(Clone, Copy, PartialEq)]
    struct Material {
        /// The panel's box, in its window's coordinates.
        rect: Rect,
        radius: f32,
        material: NSVisualEffectMaterial,
    }
    impl Material {
        /// For popup root `root`: its panel (what the root holds) is a box
        /// with a translucent fill.
        fn of(scene: &Scene, semantics: &Semantics, root: NodeId) -> Option<Self> {
            let panel = *scene.children(root).first()?;
            let (NodeKind::Quad(quad) | NodeKind::Panel { quad, .. }) = scene.kind(panel) else {
                return None;
            };
            if quad.fill.3 == 255 {
                return None;
            }
            let menu = semantics
                .get(panel)
                .is_some_and(|node| node.role == Role::Menu);
            Some(Self {
                rect: scene.bounds(panel),
                radius: quad.radius,
                material: if menu {
                    NSVisualEffectMaterial::Menu
                } else {
                    NSVisualEffectMaterial::Popover
                },
            })
        }
    }

    /// A popup window: a borderless panel holding AppKit's material (when
    /// its popup wants it) under a view hosting a renderer's layer.
    struct Panel {
        window: Retained<NSPanel>,
        view: Retained<PopupView>,
        effect: Retained<NSVisualEffectView>,
        material: Option<Material>,
        host: Retained<CALayer>,
        renderer: Option<GpuRenderer>,
        /// The scale its renderer was made for.
        scale: f32,
        /// Its frame in its parent window's coordinates, as last placed.
        rect: Option<Rect>,
        /// A drag preview's: from where its preview goes to its frame.
        preview: Option<(f32, f32)>,
        /// Moved by the window server alone (`follow`), to `rect` over its
        /// parent: AppKit's frame is stale until set.
        moved: Option<(Retained<NSWindow>, Retained<NSView>, Rect)>,
    }
    unsafe extern "C" {
        fn CGSMainConnectionID() -> i32;
        fn CGSMoveWindow(connection: i32, window: u32, origin: *const NSPoint) -> i32;
    }
    impl Panel {
        fn new(mtm: MainThreadMarker) -> Self {
            let frame = NSRect::new(NSPoint::new(0., 0.), NSSize::new(1., 1.));
            let window = NSPanel::initWithContentRect_styleMask_backing_defer(
                mtm.alloc::<NSPanel>(),
                frame,
                NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
                NSBackingStoreType::Buffered,
                false,
            );
            // SAFETY: plain property setters on a window of ours.
            unsafe { window.setReleasedWhenClosed(false) };
            window.setOpaque(false);
            window.setBackgroundColor(Some(&NSColor::clearColor()));
            window.setHasShadow(true);
            // Over its parent and every other window, as AppKit's menus.
            window.setLevel(NSPopUpMenuWindowLevel);
            // Shown at once, and only while zgui shows it (it closes popups
            // as the app deactivates).
            window.setAnimationBehavior(NSWindowAnimationBehavior::None);
            window.setHidesOnDeactivate(false);
            window.setCollectionBehavior(
                NSWindowCollectionBehavior::Transient
                    | NSWindowCollectionBehavior::IgnoresCycle
                    | NSWindowCollectionBehavior::FullScreenAuxiliary,
            );
            window.setAcceptsMouseMovedEvents(true);
            window.setBecomesKeyOnlyIfNeeded(true);
            let host = CALayer::new();
            host.setHidden(true);
            let content = NSView::initWithFrame(mtm.alloc::<NSView>(), frame);
            content.setWantsLayer(true);
            let effect =
                NSVisualEffectView::initWithFrame(mtm.alloc::<NSVisualEffectView>(), frame);
            effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
            effect.setState(NSVisualEffectState::Active);
            effect.setWantsLayer(true);
            effect.setHidden(true);
            if let Some(layer) = effect.layer() {
                // SAFETY: CALayer's own property, a string constant.
                unsafe {
                    let continuous = NSString::from_str("continuous");
                    let _: () = msg_send![&*layer, setCornerCurve: &*continuous];
                }
                layer.setMasksToBounds(true);
            }
            content.addSubview(&effect);
            let view: Retained<PopupView> = {
                let view = mtm.alloc::<PopupView>().set_ivars(Target::default());
                // SAFETY: NSView's designated initializer.
                unsafe { msg_send![super(view), initWithFrame: frame] }
            };
            // Layer-hosting: our layer, set before it wants one.
            view.setLayer(Some(&host));
            view.setWantsLayer(true);
            // SAFETY: a tracking area owned by the view it reports to.
            let area = unsafe {
                NSTrackingArea::initWithRect_options_owner_userInfo(
                    mtm.alloc::<NSTrackingArea>(),
                    NSRect::ZERO,
                    NSTrackingAreaOptions::MouseMoved
                        | NSTrackingAreaOptions::MouseEnteredAndExited
                        | NSTrackingAreaOptions::ActiveAlways
                        | NSTrackingAreaOptions::InVisibleRect,
                    Some(&view as &AnyObject),
                    None,
                )
            };
            view.addTrackingArea(&area);
            view.setAutoresizingMask(
                NSAutoresizingMaskOptions::ViewWidthSizable
                    | NSAutoresizingMaskOptions::ViewHeightSizable,
            );
            content.addSubview(&view);
            window.setContentView(Some(&content));
            Self {
                window,
                view,
                effect,
                material: None,
                host,
                renderer: None,
                scale: 0.,
                rect: None,
                preview: None,
                moved: None,
            }
        }
        /// Its renderer, made for `scale` (again for a new one).
        fn renderer(
            &mut self,
            context: &GpuContext,
            scale: f32,
        ) -> Result<&mut GpuRenderer, GpuError> {
            if self.scale != scale {
                self.renderer = None;
            }
            if self.renderer.is_none() {
                self.scale = scale;
                self.host.setContentsScale(scale as f64);
                self.renderer = Some(GpuRenderer::for_overlay_layer(
                    self.host.clone(),
                    context,
                    scale,
                )?);
            }
            Ok(self.renderer.as_mut().expect("made above"))
        }
        /// Show `material` behind its frame at `rect`.
        fn back(&mut self, material: Option<Material>, rect: Rect) {
            if self.material == material && self.rect == Some(rect) {
                return;
            }
            self.material = material;
            match material {
                None => self.effect.setHidden(true),
                Some(material) => {
                    self.effect.setMaterial(material.material);
                    self.effect.setFrame(NSRect::new(
                        NSPoint::new(
                            (material.rect.x - rect.x) as f64,
                            // The container runs bottom-up.
                            (rect.y + rect.height - material.rect.y - material.rect.height) as f64,
                        ),
                        NSSize::new(material.rect.width as f64, material.rect.height as f64),
                    ));
                    if let Some(layer) = self.effect.layer() {
                        layer.setCornerRadius(material.radius as f64);
                    }
                    self.effect.setHidden(false);
                }
            }
        }
        /// Place it at `rect` (its parent's coordinates) over `parent`.
        fn place(&mut self, parent: &NSWindow, content: &NSView, rect: Rect) {
            self.sync();
            if self.rect == Some(rect) {
                return;
            }
            self.rect = Some(rect);
            let local = NSRect::new(
                NSPoint::new(rect.x as f64, rect.y as f64),
                NSSize::new(rect.width as f64, rect.height as f64),
            );
            let screen = parent.convertRectToScreen(content.convertRect_toView(local, None));
            self.window.setFrame_display(screen, false);
            self.view.ivars().origin.set((rect.x, rect.y));
            // The presenting layers fill the host exactly (see the overlay
            // plane's `show_overlay`).
            let bounds = self.view.bounds();
            self.host.setFrame(bounds);
            if let Some(layers) = unsafe { self.host.sublayers() } {
                for layer in layers.iter() {
                    layer.setFrame(bounds);
                }
            }
        }
        /// AppKit learns where the window server has moved it (`follow`).
        fn sync(&mut self) {
            if let Some((parent, content, rect)) = self.moved.take() {
                self.rect = None;
                self.place(&parent, &content, rect);
            }
        }
        fn hide(&mut self) {
            self.sync();
            let parent = self.view.ivars().parent.borrow_mut().take();
            if let Some(parent) = parent {
                parent.removeChildWindow(&self.window);
            }
            self.window.orderOut(None);
            *self.view.ivars().send.borrow_mut() = None;
            // Shown again with its next popup's first frame, never this one's.
            transaction(|| self.host.setHidden(true));
            self.effect.setHidden(true);
            self.material = None;
            self.rect = None;
            if self.preview.take().is_some() {
                PREVIEWS.with(|previews| {
                    previews
                        .borrow_mut()
                        .retain(|number| *number != self.window.windowNumber())
                });
                self.window.setIgnoresMouseEvents(false);
            }
            if let Some(renderer) = &mut self.renderer {
                renderer.trim();
            }
        }
    }
    fn transaction(change: impl FnOnce()) {
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        change();
        CATransaction::commit();
    }

    thread_local! {
        /// Hidden panels, ready to show.
        static POOL_PANELS: RefCell<Vec<Panel>> = const { RefCell::new(Vec::new()) };
        /// Windows showing popups, by window number, and their hosts.
        static PARENTS: RefCell<Vec<(isize, Sink)>> =
            const { RefCell::new(Vec::new()) };
        /// Panels showing drag previews, by window number: the pointer is
        /// over what is under them.
        static PREVIEWS: RefCell<Vec<isize>> = const { RefCell::new(Vec::new()) };
    }
    /// The level drag images show at (`kCGDraggingWindowLevel`), over menus.
    const DRAGGING_LEVEL: isize = 500;
    /// Close a window's popups as it is pressed outside its content (its
    /// title bar), as AppKit closes menus and transient popovers; presses in
    /// its content reach zgui, whose popups close on them. Once per process.
    fn watch_presses() {
        use objc2_app_kit::NSEventMask;
        thread_local! {
            static WATCHING: Cell<bool> = const { Cell::new(false) };
        }
        if WATCHING.replace(true) {
            return;
        }
        let block = block2::RcBlock::new(|event: std::ptr::NonNull<NSEvent>| -> *mut NSEvent {
            // SAFETY: AppKit passes a live event.
            let event_ref = unsafe { event.as_ref() };
            let Some(mtm) = MainThreadMarker::new() else {
                return event.as_ptr();
            };
            let Some(window) = event_ref.window(mtm) else {
                return event.as_ptr();
            };
            let send = PARENTS.with(|parents| {
                parents
                    .borrow()
                    .iter()
                    .find(|(number, _)| *number == window.windowNumber())
                    .map(|(_, send)| send.clone())
            });
            if let (Some(send), Some(content)) = (send, window.contentView()) {
                let point = content.convertPoint_fromView(event_ref.locationInWindow(), None);
                if !content.mouse_inRect(point, content.bounds()) {
                    send(PopupInput::Dismiss);
                }
            }
            event.as_ptr()
        });
        let mask =
            NSEventMask::LeftMouseDown | NSEventMask::RightMouseDown | NSEventMask::OtherMouseDown;
        // SAFETY: the handler returns the event it was given; the monitor
        // lives as long as the app.
        let monitor =
            unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &block) };
        std::mem::forget(monitor);
    }
    thread_local! {
        /// Previews sliding back, by number, until their animation ends.
        static RETURNING: RefCell<Vec<(u64, Panel)>> = const { RefCell::new(Vec::new()) };
        static NEXT_RETURN: Cell<u64> = const { Cell::new(0) };
    }
    /// Slide `panel` to `to` (its parent's coordinates) over AppKit's drag
    /// image duration and ease, then put it back in the pool.
    fn slide_back(panel: Panel, parent: &NSWindow, content: &NSView, to: Rect) {
        let local = NSRect::new(
            NSPoint::new(to.x as f64, to.y as f64),
            NSSize::new(to.width as f64, to.height as f64),
        );
        let target = parent.convertRectToScreen(content.convertRect_toView(local, None));
        let window = panel.window.clone();
        let id = NEXT_RETURN.with(|next| {
            next.set(next.get() + 1);
            next.get()
        });
        RETURNING.with(|returning| returning.borrow_mut().push((id, panel)));
        let changes = block2::RcBlock::new(move |context: *mut AnyObject| {
            // SAFETY: AppKit's animation context, and a window's animator
            // proxy, which animates its frame.
            unsafe {
                let _: () = msg_send![context, setDuration: 0.2f64];
                let ease: Retained<AnyObject> = msg_send![
                    objc2::class!(CAMediaTimingFunction),
                    functionWithName: &*NSString::from_str("easeOut")
                ];
                let _: () = msg_send![context, setTimingFunction: &*ease];
                let animator: Retained<NSWindow> = msg_send![&*window, animator];
                animator.setFrame_display(target, false);
            }
        });
        let done = block2::RcBlock::new(move || {
            let panel = RETURNING.with(|returning| {
                let mut returning = returning.borrow_mut();
                let index = returning.iter().position(|(other, _)| *other == id)?;
                Some(returning.swap_remove(index).1)
            });
            if let Some(panel) = panel {
                release(panel);
            }
        });
        // SAFETY: the class method taking a changes block and a completion
        // block, both copied by AppKit.
        unsafe {
            let _: () = msg_send![
                objc2::class!(NSAnimationContext),
                runAnimationGroup: &*changes,
                completionHandler: &*done
            ];
        }
    }
    fn acquire(mtm: MainThreadMarker) -> Panel {
        POOL_PANELS
            .with(|pool| pool.borrow_mut().pop())
            .unwrap_or_else(|| Panel::new(mtm))
    }
    fn release(mut panel: Panel) {
        panel.hide();
        POOL_PANELS.with(|pool| {
            let mut pool = pool.borrow_mut();
            if pool.len() < pool_size() {
                pool.push(panel);
            }
        });
    }
    /// Make the pool's panels and their renderers ahead of the first popup,
    /// once the first window has shown (so startup does not wait for them).
    /// Each is ordered in once, transparent and off screen, then out: the
    /// window server's first showing of a window is what costs.
    pub(crate) fn prewarm(context: &GpuContext, scale: f32) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let made = POOL_PANELS.with(|pool| pool.borrow().len());
        for _ in made..pool_size() {
            let mut panel = Panel::new(mtm);
            if panel.renderer(context, scale).is_err() {
                return;
            }
            panel.window.setAlphaValue(0.);
            panel
                .window
                .setFrameOrigin(NSPoint::new(-10_000., -10_000.));
            panel.window.orderFront(None);
            panel.window.orderOut(None);
            panel.window.setAlphaValue(1.);
            POOL_PANELS.with(|pool| pool.borrow_mut().push(panel));
        }
    }

    fn parent_of(window: &winit::window::Window) -> Option<(Retained<NSWindow>, Retained<NSView>)> {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let handle = window.window_handle().ok()?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return None;
        };
        // SAFETY: winit's content view, alive while its window is.
        let view = unsafe { Retained::<NSView>::retain(handle.ns_view.as_ptr().cast()) }?;
        Some((view.window()?, view))
    }

    /// Where popups may show: the visible part of each screen (less the
    /// menu bar and Dock), in the window's coordinates. None with
    /// `ZGUI_POPUP_WINDOWS=0`: popups stay inside their window, as elsewhere.
    pub(crate) fn popup_screens(window: &winit::window::Window) -> Vec<Rect> {
        thread_local! {
            static OFF: bool = std::env::var_os("ZGUI_POPUP_WINDOWS").is_some_and(|v| v == "0");
        }
        if OFF.with(|off| *off) {
            return Vec::new();
        }
        let Some((parent, content)) = parent_of(window) else {
            return Vec::new();
        };
        let frame = parent.convertRectToScreen(content.convertRect_toView(content.bounds(), None));
        // Screen space runs bottom-up, the window's top-down.
        let top = frame.origin.y + frame.size.height;
        objc2_app_kit::NSScreen::screens(MainThreadMarker::from(&*parent))
            .iter()
            .map(|screen| {
                let screen = screen.visibleFrame();
                Rect::new(
                    (screen.origin.x - frame.origin.x) as f32,
                    (top - (screen.origin.y + screen.size.height)) as f32,
                    screen.size.width as f32,
                    screen.size.height as f32,
                )
            })
            .collect()
    }
    /// The backing scale of the screen holding `rect`'s centre (the
    /// parent's coordinates): a popup draws at the scale it shows at.
    fn scale_at(parent: &NSWindow, content: &NSView, rect: Rect) -> Option<f32> {
        let local = NSRect::new(
            NSPoint::new(
                (rect.x + rect.width / 2.) as f64,
                (rect.y + rect.height / 2.) as f64,
            ),
            NSSize::new(0., 0.),
        );
        let point = parent
            .convertRectToScreen(content.convertRect_toView(local, None))
            .origin;
        objc2_app_kit::NSScreen::screens(MainThreadMarker::from(parent))
            .iter()
            .find(|screen| {
                let frame = screen.frame();
                (frame.origin.x..frame.origin.x + frame.size.width).contains(&point.x)
                    && (frame.origin.y..frame.origin.y + frame.size.height).contains(&point.y)
            })
            .map(|screen| screen.backingScaleFactor() as f32)
    }

    /// The primary screen's height: global coordinates flip about it
    /// (AppKit's run bottom-up from its bottom, zgui's top-down from its top).
    fn flip_height(mtm: MainThreadMarker) -> f64 {
        objc2_app_kit::NSScreen::screens(mtm)
            .firstObject()
            .map_or(0., |screen| screen.frame().size.height)
    }
    /// The window's content top-left in global coordinates (top-down from
    /// the primary screen's top, in points).
    pub(crate) fn content_origin(window: &winit::window::Window) -> Option<(f64, f64)> {
        let (parent, content) = parent_of(window)?;
        let frame = parent.convertRectToScreen(content.convertRect_toView(content.bounds(), None));
        Some((
            frame.origin.x,
            flip_height(MainThreadMarker::from(&*parent)) - (frame.origin.y + frame.size.height),
        ))
    }
    pub(crate) fn window_number(window: &winit::window::Window) -> Option<isize> {
        parent_of(window).map(|(parent, _)| parent.windowNumber())
    }
    /// The front-most window at global point `(x, y)` (any app's), past
    /// drag previews.
    pub(crate) fn window_at(x: f64, y: f64) -> Option<isize> {
        let mtm = MainThreadMarker::new()?;
        let point = NSPoint::new(x, flip_height(mtm) - y);
        let mut below = 0;
        loop {
            let number =
                NSWindow::windowNumberAtPoint_belowWindowWithWindowNumber(point, below, mtm);
            if number <= 0 {
                return None;
            }
            if !PREVIEWS.with(|previews| previews.borrow().contains(&number)) {
                return Some(number);
            }
            below = number;
        }
    }

    /// A window's shown popups.
    pub(crate) struct PopupHost {
        shown: Vec<(NodeId, Panel)>,
        send: Sink,
    }
    impl PopupHost {
        pub fn new(send: impl Fn(PopupInput) + 'static) -> Self {
            Self {
                shown: Vec::new(),
                send: Rc::new(send),
            }
        }
        /// After the scene's flush: show each visible popup root in a panel
        /// over `window`, draw what changed in it (`damage` and the flush's
        /// `popup_damage`; everything when `full`), and hide the rest.
        #[allow(clippy::too_many_arguments)]
        pub fn update(
            &mut self,
            window: &winit::window::Window,
            scene: &Scene,
            semantics: &Semantics,
            context: &GpuContext,
            scale: f32,
            damage: &[Rect],
            popup_damage: &[Rect],
            full: bool,
            drag: Option<&DragState>,
            ended: Option<DragEnded>,
        ) -> Result<(), GpuError> {
            if self.shown.is_empty() && scene.popups().is_empty() {
                return Ok(());
            }
            let Some(mtm) = MainThreadMarker::new() else {
                return Ok(());
            };
            let Some((parent, content)) = parent_of(window) else {
                return Ok(());
            };
            let wanted: Vec<(NodeId, Rect)> = scene
                .popups()
                .iter()
                .filter_map(|root| scene.popup_bounds(*root).map(|bounds| (*root, bounds)))
                .collect();
            let mut index = 0;
            while index < self.shown.len() {
                if wanted.iter().any(|(root, _)| *root == self.shown[index].0) {
                    index += 1;
                    continue;
                }
                let (root, mut panel) = self.shown.swap_remove(index);
                zgui_gpu::present_log::log(|| format!("popup hide {root:?}"));
                // A drag nothing took: its preview slides back to its source,
                // as AppKit's drag images do.
                let back = ended
                    .filter(|ended| {
                        ended.preview == Some(root) && !ended.accepted && !ended.outside
                    })
                    .filter(|ended| scene.contains(ended.source))
                    .zip(panel.preview.zip(panel.rect));
                match back {
                    Some((ended, ((dx, dy), rect))) => {
                        let source = scene.bounds(ended.source);
                        panel.sync();
                        panel.window.orderFront(None);
                        slide_back(
                            panel,
                            &parent,
                            &content,
                            Rect::new(source.x + dx, source.y + dy, rect.width, rect.height),
                        );
                    }
                    None => release(panel),
                }
            }
            let number = parent.windowNumber();
            PARENTS.with(|parents| {
                let mut parents = parents.borrow_mut();
                parents.retain(|(window, _)| *window != number);
                if !wanted.is_empty() {
                    parents.push((number, self.send.clone()));
                }
            });
            if !wanted.is_empty() {
                watch_presses();
            }
            let damage = [damage, popup_damage].concat();
            for (root, bounds) in wanted {
                let new = !self.shown.iter().any(|(shown, _)| *shown == root);
                if new {
                    zgui_gpu::present_log::log(|| format!("popup show {root:?} at {bounds:?}"));
                    self.shown.push((root, acquire(mtm)));
                }
                let panel = &mut self
                    .shown
                    .iter_mut()
                    .find(|(shown, _)| *shown == root)
                    .expect("pushed above")
                    .1;
                // Its screen's scale, found as it shows.
                let scale = match new {
                    true => scale_at(&parent, &content, bounds).unwrap_or(scale),
                    false => panel.scale,
                };
                let renderer = panel.renderer(context, scale)?;
                renderer.set_popup(root, bounds);
                let rect = renderer.plane_rect().unwrap_or(bounds);
                if new || full {
                    renderer.repaint_all();
                }
                renderer.render(scene, &damage)?;
                // A drag preview's window goes where the drag puts it; its
                // frame is drawn where the scene has it.
                let preview = drag
                    .filter(|drag| drag.preview == Some(root))
                    .and_then(|drag| drag.preview_origin)
                    .map(|(x, y)| {
                        let laid = scene.bounds(root);
                        (rect.x - laid.x, rect.y - laid.y, x, y)
                    });
                panel.preview = preview.map(|(dx, dy, _, _)| (dx, dy));
                let frame = match preview {
                    Some((dx, dy, x, y)) => Rect::new(x + dx, y + dy, rect.width, rect.height),
                    None => rect,
                };
                let moved = panel.rect != Some(frame);
                let material = Material::of(scene, semantics, root).map(|mut material| {
                    material.rect.x += frame.x - rect.x;
                    material.rect.y += frame.y - rect.y;
                    material
                });
                let drawn = panel
                    .renderer
                    .as_ref()
                    .is_some_and(GpuRenderer::frame_pending);
                if !new && !moved && !drawn && material == panel.material {
                    // Nothing new to show: no transaction for the render
                    // server either.
                    continue;
                }
                // The frame and where it shows change together.
                CATransaction::begin();
                CATransaction::setDisableActions(true);
                panel.renderer.as_mut().expect("made above").present()?;
                panel.back(material, frame);
                panel.place(&parent, &content, frame);
                if new {
                    panel.host.setHidden(false);
                }
                CATransaction::commit();
                if new {
                    let ivars = panel.view.ivars();
                    *ivars.send.borrow_mut() = Some(self.send.clone());
                    *ivars.parent.borrow_mut() = Some(parent.clone());
                    // Its first frame reaches the window server before it shows.
                    CATransaction::flush();
                    if panel.preview.is_some() {
                        // Over every window, the pointer passing through it,
                        // as AppKit's drag images.
                        panel.window.setIgnoresMouseEvents(true);
                        panel.window.setLevel(DRAGGING_LEVEL);
                        let number = panel.window.windowNumber();
                        PREVIEWS.with(|previews| previews.borrow_mut().push(number));
                        *ivars.parent.borrow_mut() = None;
                    } else {
                        // SAFETY: both are live windows of this process.
                        unsafe {
                            parent
                                .addChildWindow_ordered(&panel.window, NSWindowOrderingMode::Above)
                        };
                        // A child takes its parent's level; menus' is higher.
                        panel.window.setLevel(NSPopUpMenuWindowLevel);
                    }
                    panel.window.orderFront(None);
                }
                if new || moved {
                    // Its rim (macOS 26 menus have no drop shadow, only a
                    // rim) follows what it shows.
                    panel.window.invalidateShadow();
                }
            }
            Ok(())
        }
        /// Move the drag preview's window to where `drag` puts it, drawing
        /// nothing (between frames, as the pointer moves).
        pub fn follow(&mut self, window: &winit::window::Window, drag: &DragState) {
            let (Some(root), Some((x, y))) = (drag.preview, drag.preview_origin) else {
                return;
            };
            let Some(panel) = self
                .shown
                .iter_mut()
                .find(|(shown, _)| *shown == root)
                .map(|(_, panel)| panel)
            else {
                return;
            };
            let (Some((dx, dy)), Some(rect)) = (panel.preview, panel.rect) else {
                return;
            };
            let Some((parent, content)) = parent_of(window) else {
                return;
            };
            // Only its origin changes: the window server moves it, as AppKit
            // moves its own drag images, without AppKit's bookkeeping for a
            // moved window (a sixth of the cost); AppKit hears where it went
            // as the drag ends (`hide`).
            let local = NSRect::new(
                NSPoint::new((x + dx) as f64, (y + dy) as f64),
                NSSize::new(0., 0.),
            );
            let origin = parent
                .convertRectToScreen(content.convertRect_toView(local, None))
                .origin;
            let top = flip_height(MainThreadMarker::from(&*parent)) - origin.y;
            // SAFETY: the window server's own call, on a window of ours.
            let moved = unsafe {
                CGSMoveWindow(
                    CGSMainConnectionID(),
                    panel.window.windowNumber() as u32,
                    &NSPoint::new(origin.x, top),
                )
            } == 0;
            let frame = Rect::new(x + dx, y + dy, rect.width, rect.height);
            if moved {
                panel.moved = Some((parent.clone(), content.clone(), frame));
            } else {
                panel.place(&parent, &content, frame);
            }
            panel.rect = Some(frame);
        }
        /// The drag preview's last frame, as a drag image with the pointer in
        /// it (to hand the drag to AppKit).
        pub fn preview_image(&self, drag: &DragState) -> Option<crate::native_drag::DragImage> {
            let root = drag.preview?;
            let (_, panel) = self.shown.iter().find(|(shown, _)| *shown == root)?;
            let rect = panel.rect?;
            let (width, height, bgra) = panel.renderer.as_ref()?.shown_pixels()?;
            Some(crate::native_drag::DragImage {
                width,
                height,
                bgra,
                scale: panel.scale,
                grab: (drag.x - rect.x, drag.y - rect.y),
            })
        }
        /// Show the drag preview, or not (AppKit shows the drag's image
        /// meanwhile); shown again where `at` puts its frame (the drag's
        /// pointer, the parent's coordinates, less its grab point).
        pub fn show_preview(
            &mut self,
            window: &winit::window::Window,
            drag: &DragState,
            at: Option<(f32, f32)>,
        ) {
            let Some(root) = drag.preview else {
                return;
            };
            let Some((_, panel)) = self.shown.iter_mut().find(|(shown, _)| *shown == root) else {
                return;
            };
            match at {
                None => panel.window.orderOut(None),
                Some((x, y)) => {
                    let (Some(rect), Some((parent, content))) = (panel.rect, parent_of(window))
                    else {
                        return;
                    };
                    let grab = (drag.x - rect.x, drag.y - rect.y);
                    let frame = Rect::new(x - grab.0, y - grab.1, rect.width, rect.height);
                    panel.place(&parent, &content, frame);
                    panel.window.orderFront(None);
                }
            }
        }
        /// Hide every popup (the window closes or goes away).
        pub fn hide_all(&mut self, _: &winit::window::Window) {
            for (_, panel) in self.shown.drain(..) {
                release(panel);
            }
        }
    }
    impl Drop for PopupHost {
        fn drop(&mut self) {
            for (_, panel) in self.shown.drain(..) {
                release(panel);
            }
        }
    }
}
