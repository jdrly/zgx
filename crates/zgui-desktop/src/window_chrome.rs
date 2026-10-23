//! Window chrome from `WindowOptions`: the title bar's style and a frosted
//! glass behind a translucent window. Applied when the window is made, on
//! macOS and Windows; elsewhere windows keep their own title bars and draw no
//! glass.

/// How a window's title bar looks (macOS and Windows; elsewhere the
/// platform's own).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Titlebar {
    /// The platform's title bar.
    #[default]
    Standard,
    /// Transparent, without the title: content runs under it, the window
    /// buttons (the traffic lights) inset over it in a 40-point strip, as
    /// Electron's `hiddenInset`. Mark the strip `View::window_drag` so it
    /// moves the window. On Windows, zgui draws Windows 11's caption buttons
    /// over the content's top right (46 by 32 points each) and Windows takes
    /// them for its own: their tooltips, and snap layouts on maximize.
    Inset,
    /// None: content fills the window and the window buttons are hidden.
    /// The window keeps its rounded corners and shadow; on Windows its edges
    /// resize it (5 points in from them).
    Hidden,
}

/// A blurred desktop behind a translucent window (macOS's behind-window
/// `NSVisualEffectView`, cut to the window's rounded corners; on Windows 11
/// the system backdrop nearest its material, Acrylic or Mica). The window is
/// transparent where zgui paints nothing (or translucent colour).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glass {
    pub material: GlassMaterial,
    /// Only the blur: no tint from the desktop, no saturation boost, a
    /// 60-point gaussian, as gpui's `BlurredView` (for apps laying their own
    /// tint over it).
    pub plain: bool,
}
impl Glass {
    pub fn new(material: GlassMaterial) -> Self {
        Self {
            material,
            plain: false,
        }
    }
    /// The same glass, untinted and unsaturated (see `Glass::plain`).
    pub fn plain(mut self) -> Self {
        self.plain = true;
        self
    }
}
impl Default for Glass {
    fn default() -> Self {
        Self::new(GlassMaterial::Hud)
    }
}
/// The glass's material (`NSVisualEffectMaterial`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GlassMaterial {
    /// A dark, saturated heads-up-display glass.
    #[default]
    Hud,
    /// The colourless material under a window's own background.
    UnderWindowBackground,
    UnderPageBackground,
    WindowBackground,
    ContentBackground,
    Titlebar,
    HeaderView,
    Sidebar,
    Menu,
    Popover,
    Sheet,
    ToolTip,
    FullScreenUi,
    Selection,
}
impl GlassMaterial {
    /// Its `NSVisualEffectMaterial` value.
    pub fn appkit_value(self) -> isize {
        match self {
            Self::Titlebar => 3,
            Self::Selection => 4,
            Self::Menu => 5,
            Self::Popover => 6,
            Self::Sidebar => 7,
            Self::HeaderView => 10,
            Self::Sheet => 11,
            Self::WindowBackground => 12,
            Self::Hud => 13,
            Self::FullScreenUi => 15,
            Self::ToolTip => 17,
            Self::ContentBackground => 18,
            Self::UnderWindowBackground => 21,
            Self::UnderPageBackground => 22,
        }
    }
}

/// Style `window` as `titlebar` and `glass` ask: its corner radius in points
/// (content inset from the window's edge keeps its corners concentric with
/// it), 0 where the platform does not round windows or zgui cannot tell.
pub(crate) fn apply(
    window: &winit::window::Window,
    titlebar: Titlebar,
    glass: Option<Glass>,
) -> f64 {
    #[cfg(target_os = "macos")]
    {
        mac::apply(window, titlebar, glass)
    }
    #[cfg(windows)]
    {
        win::apply(window, titlebar, glass)
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = (window, titlebar, glass);
        0.
    }
}

#[cfg(windows)]
pub(crate) use win::CaptionHost;

#[cfg(windows)]
mod win {
    //! Windows title bars. `Titlebar::Hidden` is winit's frameless window
    //! (its client the whole window, with the system's shadow) with what
    //! the frame gave back: rounded corners on Windows 11 and resize bands
    //! along its edges. `Titlebar::Inset` adds the caption buttons, drawn
    //! by zgui over the content's top right (`CaptionHost`) as Windows 11
    //! draws its own, and reported to Windows as its buttons
    //! (`WM_NCHITTEST`), so it shows the maximize button's snap layouts and
    //! the buttons' tooltips. Windows' own handling of a press on them draws
    //! classic buttons into the window, so the subclass handles them itself.
    use super::{Glass, GlassMaterial, Titlebar};
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
        sync::Arc,
    };
    use windows::Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::{
            Dwm::{
                DWM_SYSTEMBACKDROP_TYPE, DWMSBT_MAINWINDOW, DWMSBT_TABBEDWINDOW,
                DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE,
                DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmExtendFrameIntoClientArea,
                DwmSetWindowAttribute,
            },
            Gdi::ScreenToClient,
        },
        UI::{
            Controls::MARGINS,
            HiDpi::GetDpiForWindow,
            Input::KeyboardAndMouse::{TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT, TrackMouseEvent},
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::{
                GetClientRect, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCLOSE, HTLEFT, HTMAXBUTTON,
                HTMINBUTTON, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, IsZoomed, PostMessageW,
                SC_CLOSE, SC_MAXIMIZE, SC_MINIMIZE, SC_RESTORE, WM_MOUSEMOVE, WM_NCDESTROY,
                WM_NCHITTEST, WM_NCLBUTTONDBLCLK, WM_NCLBUTTONDOWN, WM_NCLBUTTONUP,
                WM_NCMOUSELEAVE, WM_NCMOUSEMOVE, WM_SYSCOMMAND,
            },
        },
    };
    use winit::{
        raw_window_handle::{HasWindowHandle, RawWindowHandle},
        window::Window,
    };
    use zgui::{compose::prelude::*, scene::Color, style::Styled, widgets::Ui};

    /// Windows 11's caption buttons, in logical pixels.
    const BUTTON_WIDTH: f64 = 46.;
    const BUTTON_HEIGHT: f64 = 32.;
    /// Resize bands inside a frameless window's edges, in logical pixels:
    /// it has no frame outside them to take the pointer.
    const RESIZE_BAND: f64 = 5.;
    /// Windows 11's window corner radius, in logical pixels.
    const CORNER_RADIUS: f64 = 8.;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum CaptionButton {
        Minimize,
        Maximize,
        Close,
    }
    impl CaptionButton {
        fn hit(self) -> u32 {
            match self {
                Self::Minimize => HTMINBUTTON,
                Self::Maximize => HTMAXBUTTON,
                Self::Close => HTCLOSE,
            }
        }
        fn from_hit(hit: usize) -> Option<Self> {
            [Self::Minimize, Self::Maximize, Self::Close]
                .into_iter()
                .find(|button| button.hit() as usize == hit)
        }
    }

    /// What the window procedure knows of the caption, shared with its host.
    struct Caption {
        buttons: bool,
        hovered: Cell<Option<CaptionButton>>,
        pressed: Cell<Option<CaptionButton>>,
        tracking: Cell<bool>,
        /// Asks the host for a frame when the buttons' look changed.
        wake: RefCell<Option<Box<dyn Fn()>>>,
    }
    impl Caption {
        fn set(&self, hovered: Option<CaptionButton>, pressed: Option<CaptionButton>) {
            let hover_changed = self.hovered.replace(hovered) != hovered;
            let press_changed = self.pressed.replace(pressed) != pressed;
            if (hover_changed || press_changed)
                && let Some(wake) = &*self.wake.borrow()
            {
                wake();
            }
        }
    }

    fn hwnd(window: &Window) -> Option<HWND> {
        match window.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(handle) => Some(HWND(handle.hwnd.get() as *mut _)),
            _ => None,
        }
    }

    /// The system backdrop nearest a glass material: Acrylic (a blur, as
    /// macOS's materials are) for the transient and heads-up ones, Mica (the
    /// wallpaper's tint) under a window's or page's background, Mica Alt
    /// under title bars and headers.
    fn backdrop(material: GlassMaterial) -> DWM_SYSTEMBACKDROP_TYPE {
        match material {
            GlassMaterial::UnderWindowBackground
            | GlassMaterial::UnderPageBackground
            | GlassMaterial::WindowBackground
            | GlassMaterial::ContentBackground
            | GlassMaterial::Sidebar => DWMSBT_MAINWINDOW,
            GlassMaterial::Titlebar | GlassMaterial::HeaderView => DWMSBT_TABBEDWINDOW,
            GlassMaterial::Hud
            | GlassMaterial::Menu
            | GlassMaterial::Popover
            | GlassMaterial::ToolTip
            | GlassMaterial::Sheet
            | GlassMaterial::FullScreenUi
            | GlassMaterial::Selection => DWMSBT_TRANSIENTWINDOW,
        }
    }

    /// Put the system's backdrop behind the window: the frame, and its
    /// material, extended under the whole client, which shows it where zgui
    /// paints nothing. Windows 11 (22H2) only; elsewhere the window is
    /// transparent alone. `Glass::plain` has no Windows equivalent.
    fn frost(hwnd: HWND, glass: Glass, dark: bool) {
        // From the bottom, over the whole client: extended from the top, the
        // frame brings Windows' own caption buttons into the client, under
        // zgui's (or under nothing, where it paints none).
        let margins = MARGINS {
            cxLeftWidth: 0,
            cxRightWidth: 0,
            cyTopHeight: 0,
            cyBottomHeight: i32::from(u16::MAX),
        };
        let kind = backdrop(glass.material);
        let dark = i32::from(dark);
        // SAFETY: our own window; each attribute is given its own size.
        unsafe {
            let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &dark as *const _ as *const _,
                size_of_val(&dark) as u32,
            );
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_SYSTEMBACKDROP_TYPE,
                &kind as *const _ as *const _,
                size_of_val(&kind) as u32,
            );
        }
    }

    pub(super) fn apply(window: &Window, titlebar: Titlebar, glass: Option<Glass>) -> f64 {
        let Some(hwnd) = hwnd(window) else {
            return 0.;
        };
        if let Some(glass) = glass {
            frost(
                hwnd,
                glass,
                window.theme() != Some(winit::window::Theme::Light),
            );
        }
        if titlebar == Titlebar::Standard {
            return 0.;
        }
        // A frameless window is square unless asked; Windows 10 has no
        // rounded windows and refuses.
        // SAFETY: sets one documented attribute on our own window.
        let rounded = unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &DWMWCP_ROUND as *const _ as *const _,
                size_of_val(&DWMWCP_ROUND) as u32,
            )
        }
        .is_ok();
        let caption = Rc::new(Caption {
            buttons: titlebar == Titlebar::Inset,
            hovered: Cell::new(None),
            pressed: Cell::new(None),
            tracking: Cell::new(false),
            wake: RefCell::new(None),
        });
        // SAFETY: the subclass runs on this (the window's) thread; its
        // reference is released when the window is destroyed.
        let installed = unsafe {
            SetWindowSubclass(
                hwnd,
                Some(subclass),
                SUBCLASS_ID,
                Rc::into_raw(caption.clone()) as usize,
            )
        }
        .as_bool();
        if installed {
            CAPTIONS.with(|captions| captions.borrow_mut().push((hwnd.0 as usize, caption)));
        }
        if rounded { CORNER_RADIUS } else { 0. }
    }

    const SUBCLASS_ID: usize = 0x7a67_7569; // "zgui"
    thread_local! {
        /// Each subclassed window's caption, for its host to find.
        static CAPTIONS: RefCell<Vec<(usize, Rc<Caption>)>> = const { RefCell::new(Vec::new()) };
    }

    fn scale(hwnd: HWND) -> f64 {
        // SAFETY: reads our own window's DPI.
        f64::from(unsafe { GetDpiForWindow(hwnd) }.max(96)) / 96.
    }

    /// What lies at screen point (`x`, `y`) of the window's client, if not
    /// the content: a resize band or a caption button.
    fn hit_test(hwnd: HWND, caption: &Caption, x: i32, y: i32) -> Option<u32> {
        let mut point = POINT { x, y };
        let mut client = RECT::default();
        // SAFETY: our own window; both write into locals.
        unsafe {
            if !ScreenToClient(hwnd, &mut point).as_bool()
                || GetClientRect(hwnd, &mut client).is_err()
            {
                return None;
            }
        }
        let scale = scale(hwnd);
        // SAFETY: reads our own window's state.
        let maximized = unsafe { IsZoomed(hwnd) }.as_bool();
        let band = if maximized {
            0
        } else {
            (RESIZE_BAND * scale).round() as i32
        };
        let (left, right) = (point.x < band, point.x >= client.right - band);
        let (top, bottom) = (point.y < band, point.y >= client.bottom - band);
        // The caption buttons keep all but the window's outermost pixels.
        if caption.buttons && point.y >= band.min(1) && f64::from(point.y) < BUTTON_HEIGHT * scale {
            let width = (BUTTON_WIDTH * scale).round() as i32;
            let from_right = client.right - 1 - point.x;
            if (band.min(1)..3 * width).contains(&from_right) {
                return Some(match from_right / width {
                    0 => HTCLOSE,
                    1 => HTMAXBUTTON,
                    _ => HTMINBUTTON,
                });
            }
        }
        Some(match (top, bottom, left, right) {
            (true, _, true, _) => HTTOPLEFT,
            (true, _, _, true) => HTTOPRIGHT,
            (_, true, true, _) => HTBOTTOMLEFT,
            (_, true, _, true) => HTBOTTOMRIGHT,
            (true, ..) => HTTOP,
            (_, true, ..) => HTBOTTOM,
            (.., true, _) => HTLEFT,
            (.., true) => HTRIGHT,
            _ => return None,
        })
    }

    unsafe extern "system" fn subclass(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        data: usize,
    ) -> LRESULT {
        // SAFETY: `data` is the `Rc` `apply` leaked for this subclass, alive
        // until `WM_NCDESTROY` below takes it back.
        let caption = unsafe { &*(data as *const Caption) };
        let point = || {
            (
                (lparam.0 & 0xffff) as i16 as i32,
                ((lparam.0 >> 16) & 0xffff) as i16 as i32,
            )
        };
        match message {
            WM_NCHITTEST => {
                let (x, y) = point();
                if let Some(hit) = hit_test(hwnd, caption, x, y) {
                    return LRESULT(hit as isize);
                }
            }
            WM_NCMOUSEMOVE => {
                if !caption.tracking.replace(true) {
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE | TME_NONCLIENT,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    // SAFETY: tracks our own window.
                    let _ = unsafe { TrackMouseEvent(&mut track) };
                }
                let hovered = CaptionButton::from_hit(wparam.0);
                let pressed = caption.pressed.get().filter(|_| hovered.is_some());
                caption.set(hovered, pressed);
            }
            WM_NCMOUSELEAVE => {
                caption.tracking.set(false);
                caption.set(None, None);
            }
            WM_MOUSEMOVE => caption.set(None, None),
            WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK => {
                if let Some(button) = CaptionButton::from_hit(wparam.0) {
                    caption.set(Some(button), Some(button));
                    return LRESULT(0);
                }
            }
            WM_NCLBUTTONUP => {
                if let Some(button) = CaptionButton::from_hit(wparam.0) {
                    let pressed = caption.pressed.get() == Some(button);
                    caption.set(Some(button), None);
                    if pressed {
                        let command = match button {
                            CaptionButton::Minimize => SC_MINIMIZE,
                            // SAFETY: reads our own window's state.
                            CaptionButton::Maximize if unsafe { IsZoomed(hwnd) }.as_bool() => {
                                SC_RESTORE
                            }
                            CaptionButton::Maximize => SC_MAXIMIZE,
                            CaptionButton::Close => SC_CLOSE,
                        };
                        // The system command animates and asks winit, as
                        // the frame's own buttons do.
                        // SAFETY: posts to our own window.
                        let _ = unsafe {
                            PostMessageW(
                                Some(hwnd),
                                WM_SYSCOMMAND,
                                WPARAM(command as usize),
                                LPARAM(0),
                            )
                        };
                    }
                    return LRESULT(0);
                }
            }
            WM_NCDESTROY => {
                // SAFETY: removes this subclass and takes back its reference.
                unsafe {
                    let _ = RemoveWindowSubclass(hwnd, Some(subclass), SUBCLASS_ID);
                    drop(Rc::from_raw(data as *const Caption));
                }
                CAPTIONS.with(|captions| {
                    captions
                        .borrow_mut()
                        .retain(|(window, _)| *window != hwnd.0 as usize)
                });
            }
            _ => {}
        }
        // SAFETY: forwards to the next procedure in the chain (winit's).
        unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
    }

    /// The caption buttons of a window with `Titlebar::Inset`, drawn over
    /// its content: hovered, pressed, the window's state and appearance
    /// follow the window procedure, a frame after they change.
    pub(crate) struct CaptionHost {
        caption: Rc<Caption>,
        window: std::sync::Weak<Window>,
        hovered: zgui::reactive::Signal<Option<CaptionButton>>,
        pressed: zgui::reactive::Signal<Option<CaptionButton>>,
        maximized: zgui::reactive::Signal<bool>,
        active: zgui::reactive::Signal<bool>,
        dark: zgui::reactive::Signal<bool>,
        view: Option<zgui::compose::ViewHandle>,
    }
    impl CaptionHost {
        /// For a window `apply` gave caption buttons; `wake` asks for a frame.
        pub(crate) fn new(
            window: &Arc<Window>,
            ui: &Ui,
            wake: impl Fn() + 'static,
        ) -> Option<Self> {
            let hwnd = hwnd(window)?.0 as usize;
            let caption = CAPTIONS.with(|captions| {
                captions
                    .borrow()
                    .iter()
                    .find(|(window, _)| *window == hwnd)
                    .map(|(_, caption)| caption.clone())
            })?;
            if !caption.buttons {
                return None;
            }
            *caption.wake.borrow_mut() = Some(Box::new(wake));
            Some(Self {
                caption,
                window: Arc::downgrade(window),
                hovered: ui.signal(None),
                pressed: ui.signal(None),
                maximized: ui.signal(false),
                active: ui.signal(true),
                dark: ui.signal(true),
                view: None,
            })
        }
        /// Before a frame: bring the buttons up to date, and back if the
        /// content was rendered again over them.
        pub(crate) fn sync(&mut self, ui: &mut Ui) {
            if let Some(window) = self.window.upgrade() {
                self.maximized.set(window.is_maximized());
                self.active.set(window.has_focus());
                self.dark
                    .set(window.theme() != Some(winit::window::Theme::Light));
            }
            self.hovered.set(self.caption.hovered.get());
            self.pressed.set(self.caption.pressed.get());
            if !self.view.as_ref().is_some_and(|view| view.is_mounted()) {
                self.view = Some(ui.mount(self.buttons()));
            }
        }
        fn buttons(&self) -> View {
            // Windows 11's icon font; Windows 10's has the same glyphs.
            let font = if std::path::Path::new(r"C:\Windows\Fonts\SegoeIcons.ttf").exists() {
                "Segoe Fluent Icons"
            } else {
                "Segoe MDL2 Assets"
            };
            let button = |kind: CaptionButton| {
                let (hovered, pressed, dark) = (
                    self.hovered.clone(),
                    self.pressed.clone(),
                    self.dark.clone(),
                );
                let (glyph_hovered, glyph_dark, active, maximized) = (
                    self.hovered.clone(),
                    self.dark.clone(),
                    self.active.clone(),
                    self.maximized.clone(),
                );
                div()
                    .size(BUTTON_WIDTH as f32, BUTTON_HEIGHT as f32)
                    .items_center()
                    .justify_center()
                    .reactive_style(move || {
                        let hover = hovered.get() == Some(kind);
                        let down = hover && pressed.get() == Some(kind);
                        // WinUI's caption button brushes.
                        let fill = match (kind, hover, down, dark.get()) {
                            (_, false, ..) => Color(0, 0, 0, 0),
                            (CaptionButton::Close, _, false, _) => Color(196, 43, 28, 255),
                            (CaptionButton::Close, _, true, _) => Color(196, 43, 28, 230),
                            (_, _, false, true) => Color(255, 255, 255, 15),
                            (_, _, true, true) => Color(255, 255, 255, 10),
                            (_, _, false, false) => Color(0, 0, 0, 9),
                            (_, _, true, false) => Color(0, 0, 0, 6),
                        };
                        Styles::new().bg(fill)
                    })
                    .child(
                        text_signal(move || {
                            match kind {
                                CaptionButton::Minimize => "\u{e921}",
                                CaptionButton::Maximize if maximized.get() => "\u{e923}",
                                CaptionButton::Maximize => "\u{e922}",
                                CaptionButton::Close => "\u{e8bb}",
                            }
                            .to_owned()
                        })
                        .font_family(font)
                        .text_size(10.)
                        .reactive_style(move || {
                            let red =
                                kind == CaptionButton::Close && glyph_hovered.get() == Some(kind);
                            let color = match (red, glyph_dark.get(), active.get()) {
                                (true, ..) => Color(255, 255, 255, 255),
                                (_, true, true) => Color(255, 255, 255, 255),
                                (_, true, false) => Color(255, 255, 255, 93),
                                (_, false, true) => Color(0, 0, 0, 228),
                                (_, false, false) => Color(0, 0, 0, 92),
                            };
                            Styles::new().text_color(color)
                        }),
                    )
            };
            row()
                .absolute()
                .top(0.)
                .right(0.)
                .pointer_events(false)
                .child(button(CaptionButton::Minimize))
                .child(button(CaptionButton::Maximize))
                .child(button(CaptionButton::Close))
        }
    }
}

/// A double click in a window drag region: what a double click on the title
/// bar does. macOS follows the system setting (zoom, minimize or nothing);
/// elsewhere the window toggles maximized.
pub(crate) fn double_click(window: &winit::window::Window) {
    #[cfg(target_os = "macos")]
    mac::double_click(window);
    #[cfg(not(target_os = "macos"))]
    window.set_maximized(!window.is_maximized());
}

#[cfg(target_os = "macos")]
mod mac {
    use super::{Glass, Titlebar};
    use objc2::{
        DefinedClass, MainThreadOnly, define_class, msg_send,
        rc::Retained,
        runtime::{AnyClass, AnyObject},
        sel,
    };
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSView, NSVisualEffectBlendingMode, NSVisualEffectMaterial,
        NSVisualEffectState, NSVisualEffectView, NSWindow, NSWindowStyleMask,
        NSWindowTitleVisibility,
    };
    use objc2_foundation::NSString;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    pub(super) fn ns_window(window: &winit::window::Window) -> Option<Retained<NSWindow>> {
        let handle = window.window_handle().ok()?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return None;
        };
        // SAFETY: winit's view, alive while its window is; the main thread.
        let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
        view.window()
    }

    pub(super) fn apply(
        window: &winit::window::Window,
        titlebar: Titlebar,
        glass: Option<Glass>,
    ) -> f64 {
        let Some(window) = ns_window(window) else {
            return 0.;
        };
        match titlebar {
            Titlebar::Standard => {}
            Titlebar::Inset => {
                content_under_titlebar(&window);
                inset_buttons(&window);
            }
            Titlebar::Hidden => {
                content_under_titlebar(&window);
                // NSWindowCloseButton, MiniaturizeButton, ZoomButton.
                for button in [0_isize, 1, 2] {
                    unsafe {
                        let button: *mut AnyObject =
                            msg_send![&*window, standardWindowButton: button];
                        if let Some(button) = button.as_ref() {
                            let _: () = msg_send![button, setHidden: true];
                        }
                    }
                }
            }
        }
        if let Some(glass) = glass {
            frost(&window, glass);
        }
        radius(&window)
    }

    /// Content under a transparent, untitled title bar with no separator.
    fn content_under_titlebar(window: &NSWindow) {
        window.setStyleMask(window.styleMask() | NSWindowStyleMask::FullSizeContentView);
        window.setTitlebarAppearsTransparent(true);
        window.setTitleVisibility(NSWindowTitleVisibility::Hidden);
        // NSTitlebarSeparatorStyleNone
        unsafe {
            let _: () = msg_send![window, setTitlebarSeparatorStyle: 1_isize];
        }
    }

    /// An empty compact unified toolbar: it centres the traffic lights in a
    /// 40-point strip.
    fn inset_buttons(window: &NSWindow) {
        let Some(class) = AnyClass::get(c"NSToolbar") else {
            return;
        };
        unsafe {
            let id = NSString::from_str("zgui.titlebar.inset");
            let toolbar: Retained<AnyObject> =
                msg_send![msg_send![class, alloc], initWithIdentifier: &*id];
            let _: () = msg_send![&*toolbar, setShowsBaselineSeparator: false];
            let _: () = msg_send![window, setToolbar: &*toolbar];
            // NSWindowToolbarStyleUnifiedCompact
            let _: () = msg_send![window, setToolbarStyle: 4_isize];
        }
        toolbar_passes_through();
    }

    /// The empty toolbar is only there to centre the traffic lights: presses
    /// in it go to the content under it. Otherwise it keeps a press's drag
    /// and release events (zgui would start a drag and never see it end).
    /// The traffic lights are in the titlebar view, not the toolbar's, and
    /// stay clickable.
    fn toolbar_passes_through() {
        use objc2::runtime::Sel;
        use objc2_foundation::NSPoint;
        extern "C-unwind" fn nothing(
            _this: *mut AnyObject,
            _cmd: Sel,
            _point: NSPoint,
        ) -> *mut AnyObject {
            std::ptr::null_mut()
        }
        let Some(class) = AnyClass::get(c"NSToolbarView") else {
            return;
        };
        unsafe {
            let imp: objc2::runtime::Imp = std::mem::transmute(
                nothing as extern "C-unwind" fn(*mut AnyObject, Sel, NSPoint) -> *mut AnyObject,
            );
            objc2::ffi::class_replaceMethod(
                class as *const AnyClass as *mut AnyClass,
                sel!(hitTest:),
                imp,
                c"@@:{CGPoint=dd}".as_ptr(),
            );
        }
    }

    /// The window's corner radius: it varies by macOS version and toolbar
    /// style.
    pub(super) fn radius(window: &NSWindow) -> f64 {
        if window.class().responds_to(sel!(_cornerRadius)) {
            unsafe { msg_send![window, _cornerRadius] }
        } else {
            10.
        }
    }

    pub(crate) struct GlassState {
        plain: bool,
    }
    define_class!(
        /// A behind-window effect view that, plain, strips the layers AppKit
        /// builds for it each time it builds them (gpui's `BlurredView`).
        #[unsafe(super(NSVisualEffectView, NSView))]
        #[thread_kind = MainThreadOnly]
        #[name = "ZguiWindowGlass"]
        #[ivars = GlassState]
        pub(crate) struct GlassView;
        impl GlassView {
            #[unsafe(method(updateLayer))]
            fn update_layer(&self) {
                let _: () = unsafe { msg_send![super(self), updateLayer] };
                if self.ivars().plain
                    && let Some(layer) = self.layer()
                {
                    unsafe { strip_layer(&layer) };
                }
            }
        }
    );

    /// Put a behind-window effect view under the (transparent) zgui view.
    /// Unlike the private CGS window blur, it can be clipped to the window's
    /// rounded shape, so neither blur nor shadow is left square.
    fn frost(window: &NSWindow, glass: Glass) {
        let Some(content) = window.contentView() else {
            return;
        };
        let Some(frame) = (unsafe { content.superview() }) else {
            return;
        };
        let mtm = window.mtm();
        let effect: Retained<GlassView> = unsafe {
            let object = mtm
                .alloc::<GlassView>()
                .set_ivars(GlassState { plain: glass.plain });
            msg_send![super(object), initWithFrame: frame.bounds()]
        };
        effect.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        // Behind-window blending, always active.
        effect.setMaterial(NSVisualEffectMaterial(glass.material.appkit_value()));
        effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        effect.setState(NSVisualEffectState::Active);
        effect.setWantsLayer(true);
        if let Some(layer) = effect.layer() {
            layer.setCornerRadius(radius(window));
            unsafe {
                let continuous = NSString::from_str("continuous");
                let _: () = msg_send![&*layer, setCornerCurve: &*continuous];
            }
            layer.setMasksToBounds(true);
            if glass.plain {
                unsafe { strip_layer(&layer) };
            }
        }
        frame.addSubview_positioned_relativeTo(
            &effect,
            objc2_app_kit::NSWindowOrderingMode::Below,
            Some(&content),
        );
        window.invalidateShadow();
    }

    /// No desktop tint (`CAChameleonLayer` hidden), a 60-point gaussian and
    /// no saturation boost, through the effect's layer tree.
    unsafe fn strip_layer(layer: &AnyObject) {
        unsafe {
            let none: *const AnyObject = std::ptr::null();
            let _: () = msg_send![layer, setBackgroundColor: none];
            let class_name: Retained<NSString> = msg_send![layer, className];
            if class_name.to_string() == "CAChameleonLayer" {
                let _: () = msg_send![layer, setHidden: true];
                return;
            }
            let filters: *mut AnyObject = msg_send![layer, filters];
            if let Some(filters) = filters.as_ref() {
                let count: usize = msg_send![filters, count];
                let mut kept: Vec<*mut AnyObject> = Vec::with_capacity(count);
                for i in 0..count {
                    let filter: *mut AnyObject = msg_send![filters, objectAtIndex: i];
                    let Some(filter_ref) = filter.as_ref() else {
                        continue;
                    };
                    let description: Retained<NSString> = msg_send![filter_ref, description];
                    let description = description.to_string();
                    if description.contains("Saturat") {
                        continue;
                    }
                    if description.contains("Blur") {
                        let radius = objc2_foundation::NSNumber::new_f64(60.);
                        let key = NSString::from_str("inputRadius");
                        let _: () = msg_send![filter_ref, setValue: &*radius, forKey: &*key];
                    }
                    kept.push(filter);
                }
                if let Some(array_class) = AnyClass::get(c"NSArray") {
                    let array: Retained<AnyObject> = msg_send![
                        array_class,
                        arrayWithObjects: kept.as_ptr(),
                        count: kept.len()
                    ];
                    let _: () = msg_send![layer, setFilters: &*array];
                }
            }
            let sublayers: *mut AnyObject = msg_send![layer, sublayers];
            if let Some(sublayers) = sublayers.as_ref() {
                let count: usize = msg_send![sublayers, count];
                for i in 0..count {
                    let sublayer: *mut AnyObject = msg_send![sublayers, objectAtIndex: i];
                    if let Some(sublayer) = sublayer.as_ref() {
                        strip_layer(sublayer);
                    }
                }
            }
        }
    }

    /// The system's title bar double click (Desktop & Dock settings).
    pub(super) fn double_click(window: &winit::window::Window) {
        let Some(window) = ns_window(window) else {
            return;
        };
        let action = unsafe {
            let defaults: *mut AnyObject =
                msg_send![objc2::class!(NSUserDefaults), standardUserDefaults];
            let Some(defaults) = defaults.as_ref() else {
                return;
            };
            let key = NSString::from_str("AppleActionOnDoubleClick");
            let action: *mut NSString = msg_send![defaults, stringForKey: &*key];
            match action.as_ref() {
                Some(action) => action.to_string(),
                None => {
                    // Before the setting had choices: minimize or zoom.
                    let key = NSString::from_str("AppleMiniaturizeOnDoubleClick");
                    let minimize: bool = msg_send![defaults, boolForKey: &*key];
                    if minimize { "Minimize" } else { "Maximize" }.to_owned()
                }
            }
        };
        match action.as_str() {
            "Minimize" => window.miniaturize(None),
            "None" => {}
            // "Maximize" (zoom), "Fill" and anything newer: zoom.
            _ => window.zoom(None),
        }
    }
}
