//! Popup windows on Windows: owned, never-activated tool windows, each drawn
//! through a DirectComposition visual of its own (see the module's notes).
//!
//! A popup's window is `WS_POPUP` with `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW
//! | WS_EX_NOREDIRECTIONBITMAP`, owned by the window it belongs to (above it,
//! hidden and minimized with it, never on the taskbar). It answers
//! `WM_MOUSEACTIVATE` with `MA_NOACTIVATE`, so the parent keeps the keyboard
//! and keys reach the popup's content through the window's focus. Its pointer
//! input goes to the parent's host, in the parent's coordinates. Its frames
//! are an overlay renderer's (`GpuRenderer::for_overlay_visual`) drawing the
//! popup root alone, in a swap chain with alpha. A menu or popover that is
//! one rounded panel gets DWM's rounded corners, rim and shadow, as Windows
//! 11's menus do (`corner_of`).
//!
//! The parent window is subclassed while it has popups: an owned window does
//! not follow its owner, so popups move with it (`WM_WINDOWPOSCHANGED`), and
//! close as the app deactivates (`WM_ACTIVATEAPP`) and on a press in its
//! frame or title bar (`WM_NCLBUTTONDOWN`, which zgui never hears).
//!
//! Windows are pooled per thread and reused hidden, made only once the
//! pointer reaches something that opens a popup (`warm`): an app that never
//! opens one makes none. Without Direct3D 12 (the Vulkan fallback) there is
//! no visual to draw into, and popups stay inside their window.
use super::PopupInput;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::{
            DirectComposition::{
                DCompositionCreateDevice2, IDCompositionDevice, IDCompositionTarget,
                IDCompositionVisual,
            },
            Dwm::{
                DWM_WINDOW_CORNER_PREFERENCE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
                DWMWCP_ROUND, DWMWCP_ROUNDSMALL, DwmSetWindowAttribute,
            },
            Gdi::{
                ClientToScreen, EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR,
                MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint, ScreenToClient,
            },
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI},
            Input::KeyboardAndMouse::{
                GetCapture, ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
            },
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::{
                CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow,
                GWLP_HWNDPARENT, GWLP_USERDATA, GetClientRect, GetCursorPos, GetWindowLongPtrW,
                HTCLIENT, HWND_TOP, IDC_ARROW, LoadCursorW, MA_NOACTIVATE, RegisterClassExW,
                SPI_SETWORKAREA, SW_HIDE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER,
                SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SetCursor, SetWindowLongPtrW,
                SetWindowPos, ShowWindow, WINDOWPOS, WM_ACTIVATEAPP, WM_DISPLAYCHANGE,
                WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDBLCLK, WM_MBUTTONDOWN,
                WM_MBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL,
                WM_NCDESTROY, WM_NCHITTEST, WM_NCLBUTTONDOWN, WM_NCMBUTTONDOWN, WM_NCRBUTTONDOWN,
                WM_RBUTTONDBLCLK, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR, WM_SETTINGCHANGE,
                WM_WINDOWPOSCHANGED, WM_XBUTTONDBLCLK, WM_XBUTTONDOWN, WM_XBUTTONUP, WNDCLASSEXW,
                WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_POPUP,
            },
        },
    },
    core::{BOOL, w},
};
use winit::event::{MouseButton, MouseScrollDelta, TouchPhase};
use zgui::{
    scene::{NodeId, NodeKind, Rect, Scene},
    semantics::Semantics,
};
use zgui_gpu::{GpuContext, GpuError, GpuRenderer};

/// The pointer left the window (TrackMouseEvent's; not in the bindings' module).
const WM_MOUSELEAVE: u32 = 0x02a3;
const SUBCLASS_ID: usize = 0x7a67_7570; // "zgup"
/// A mouse message's buttons still down (`MK_LBUTTON | MK_RBUTTON |
/// MK_MBUTTON | MK_XBUTTON1 | MK_XBUTTON2`).
const BUTTONS: usize = 0x01 | 0x02 | 0x10 | 0x20 | 0x40;

/// How many windows stay made while hidden: a menu, its submenu and a
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

/// Where a popup window's input goes: its popup's origin in its parent
/// window (logical pixels), the scale it draws at, and the parent's host.
#[derive(Default)]
struct Target {
    origin: Cell<(f32, f32)>,
    scale: Cell<f32>,
    send: RefCell<Option<Sink>>,
    parent: Cell<usize>,
    tracking: Cell<bool>,
}
impl Target {
    fn send(&self, input: PopupInput) {
        let send = self.send.borrow().clone();
        if let Some(send) = send {
            send(input);
        }
    }
    /// Client point `(x, y)` (physical pixels) in the parent's coordinates.
    fn point(&self, (x, y): (i32, i32)) -> (f32, f32) {
        let (left, top) = self.origin.get();
        let scale = self.scale.get().max(0.5);
        (left + x as f32 / scale, top + y as f32 / scale)
    }
}

fn client_point(lparam: LPARAM) -> (i32, i32) {
    (
        (lparam.0 & 0xffff) as i16 as i32,
        ((lparam.0 >> 16) & 0xffff) as i16 as i32,
    )
}

unsafe extern "system" fn popup_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        // Clicks act without taking the keyboard from the parent.
        WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
        WM_NCHITTEST => return LRESULT(HTCLIENT as isize),
        WM_SETCURSOR => {
            // SAFETY: a system cursor, set while over our window.
            unsafe {
                if let Ok(cursor) = LoadCursorW(None, IDC_ARROW) {
                    SetCursor(Some(cursor));
                }
            }
            return LRESULT(1);
        }
        _ => {}
    }
    // SAFETY: our window's slot holds its `Target` (`Panel::new`), alive
    // until the window is destroyed (`Panel::drop` clears it first).
    let target = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const Target;
    if let Some(target) = unsafe { target.as_ref() }
        && pointer(hwnd, target, message, wparam, lparam)
    {
        return LRESULT(0);
    }
    // SAFETY: the default procedure, for our own window.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

/// A popup window's pointer message, sent to its parent's host: whether it
/// was one.
fn pointer(hwnd: HWND, target: &Target, message: u32, wparam: WPARAM, lparam: LPARAM) -> bool {
    let button = |button: MouseButton, pressed: bool| {
        let (x, y) = target.point(client_point(lparam));
        // A press holds the pointer, as a window's does: dragged out of the
        // popup, its moves still come here.
        // SAFETY: capture for our own window, on its thread.
        unsafe {
            if pressed {
                SetCapture(hwnd);
            } else if wparam.0 & BUTTONS == 0 && GetCapture() == hwnd {
                let _ = ReleaseCapture();
            }
        }
        target.send(PopupInput::Button {
            x,
            y,
            button,
            pressed,
        });
    };
    let extra = || match (wparam.0 >> 16) & 0xffff {
        1 => MouseButton::Back,
        _ => MouseButton::Forward,
    };
    match message {
        WM_MOUSEMOVE => {
            if !target.tracking.replace(true) {
                let mut track = TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                // SAFETY: tracks our own window.
                let _ = unsafe { TrackMouseEvent(&mut track) };
            }
            let (x, y) = target.point(client_point(lparam));
            target.send(PopupInput::Moved { x, y });
        }
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => button(MouseButton::Left, true),
        WM_LBUTTONUP => button(MouseButton::Left, false),
        WM_RBUTTONDOWN | WM_RBUTTONDBLCLK => button(MouseButton::Right, true),
        WM_RBUTTONUP => button(MouseButton::Right, false),
        WM_MBUTTONDOWN | WM_MBUTTONDBLCLK => button(MouseButton::Middle, true),
        WM_MBUTTONUP => button(MouseButton::Middle, false),
        WM_XBUTTONDOWN | WM_XBUTTONDBLCLK => button(extra(), true),
        WM_XBUTTONUP => button(extra(), false),
        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            // A wheel's point is on the screen.
            let (x, y) = client_point(lparam);
            let mut point = POINT { x, y };
            // SAFETY: converts into our own window's client.
            let _ = unsafe { ScreenToClient(hwnd, &mut point) };
            let (x, y) = target.point((point.x, point.y));
            // As winit reports the window's own.
            let notches = ((wparam.0 >> 16) & 0xffff) as i16 as f32 / 120.;
            let delta = if message == WM_MOUSEWHEEL {
                MouseScrollDelta::LineDelta(0., notches)
            } else {
                MouseScrollDelta::LineDelta(-notches, 0.)
            };
            target.send(PopupInput::Wheel {
                x,
                y,
                delta,
                phase: TouchPhase::Moved,
            });
        }
        WM_MOUSELEAVE => {
            target.tracking.set(false);
            // Into the parent window, its own messages take over (a leave
            // sent after them would clear what they hover).
            let parent = HWND(target.parent.get() as *mut _);
            if !parent.is_invalid() && over_client(parent) {
                return true;
            }
            target.send(PopupInput::Left);
        }
        _ => return false,
    }
    true
}

/// Whether the pointer is over `hwnd`'s client area.
fn over_client(hwnd: HWND) -> bool {
    let mut point = POINT::default();
    let mut client = RECT::default();
    // SAFETY: reads the pointer and a window of ours into locals.
    unsafe {
        if GetCursorPos(&mut point).is_err()
            || !ScreenToClient(hwnd, &mut point).as_bool()
            || GetClientRect(hwnd, &mut client).is_err()
        {
            return false;
        }
    }
    point.x >= client.left
        && point.x < client.right
        && point.y >= client.top
        && point.y < client.bottom
}

thread_local! {
    /// DirectComposition's device for every popup window of the thread.
    static DEVICE: RefCell<Option<IDCompositionDevice>> = const { RefCell::new(None) };
}
fn device() -> Result<IDCompositionDevice, String> {
    DEVICE.with(|device| {
        if let Some(device) = &*device.borrow() {
            return Ok(device.clone());
        }
        // SAFETY: a device of our own, used on this thread.
        let made: IDCompositionDevice =
            unsafe { DCompositionCreateDevice2(None) }.map_err(|e| e.to_string())?;
        *device.borrow_mut() = Some(made.clone());
        Ok(made)
    })
}

fn class() -> windows::core::PCWSTR {
    thread_local! {
        static REGISTERED: Cell<bool> = const { Cell::new(false) };
    }
    let name = w!("ZguiPopup");
    if !REGISTERED.replace(true) {
        // SAFETY: registers a class of this module's procedure.
        unsafe {
            let instance = GetModuleHandleW(None).unwrap_or_default();
            let class = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(popup_proc),
                hInstance: instance.into(),
                lpszClassName: name,
                ..Default::default()
            };
            RegisterClassExW(&class);
        }
    }
    name
}

/// A popup window: its composition and the renderer drawing into it.
struct Panel {
    hwnd: HWND,
    target: Rc<Target>,
    _composition: IDCompositionTarget,
    visual: IDCompositionVisual,
    device: IDCompositionDevice,
    renderer: Option<GpuRenderer>,
    /// The scale its renderer was made for.
    scale: f32,
    /// Its box in its parent window's coordinates, as last placed.
    rect: Option<Rect>,
    /// Where it is on the screen (physical pixels), as last placed.
    screen: Option<(i32, i32, i32, i32)>,
    /// Its popup root's layer revision it last drew.
    revision: Option<u64>,
    /// How DWM rounds it, as last set.
    corner: Option<DWM_WINDOW_CORNER_PREFERENCE>,
}

/// How DWM frames popup root `root`'s window at `rect`: as Windows 11 frames
/// its menus, with rounded corners, a rim and a shadow, when what it shows is
/// one rounded panel filling it; otherwise not at all, zgui drawing all of it
/// (DWM's shadow would double one of zgui's, past the panel).
///
/// Not with a menu's acrylic: DWM draws a never-activated window's system
/// backdrop as a flat colour, and a popup must not take activation (its
/// window keeps the keyboard).
fn corner_of(scene: &Scene, root: NodeId, rect: Rect) -> DWM_WINDOW_CORNER_PREFERENCE {
    let Some(&panel) = scene.children(root).first() else {
        return DWMWCP_DONOTROUND;
    };
    let (NodeKind::Quad(quad) | NodeKind::Panel { quad, .. }) = scene.kind(panel) else {
        return DWMWCP_DONOTROUND;
    };
    let bounds = scene.bounds(panel);
    if (bounds.width - rect.width).abs() > 1. || (bounds.height - rect.height).abs() > 1. {
        return DWMWCP_DONOTROUND;
    }
    // DWM's two radii: 8 and 4 pixels at 100%.
    match quad.radius {
        r if r >= 6. => DWMWCP_ROUND,
        r if r > 0. => DWMWCP_ROUNDSMALL,
        _ => DWMWCP_DONOTROUND,
    }
}
impl Panel {
    fn new() -> Result<Self, String> {
        let device = device()?;
        // SAFETY: a window of our class and the composition of it, on this
        // thread; its target slot is cleared before the window goes.
        unsafe {
            let hwnd = CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP,
                class(),
                w!(""),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(GetModuleHandleW(None).unwrap_or_default().into()),
                None,
            )
            .map_err(|e| e.to_string())?;
            let target = Rc::new(Target::default());
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Rc::as_ptr(&target) as isize);
            let made = (|| {
                let composition = device.CreateTargetForHwnd(hwnd, true)?;
                let visual = device.CreateVisual()?;
                composition.SetRoot(&visual)?;
                Ok::<_, windows::core::Error>((composition, visual))
            })();
            let (composition, visual) = match made {
                Ok(made) => made,
                Err(error) => {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                    let _ = DestroyWindow(hwnd);
                    return Err(error.to_string());
                }
            };
            Ok(Self {
                hwnd,
                target,
                _composition: composition,
                visual,
                device,
                renderer: None,
                scale: 0.,
                rect: None,
                screen: None,
                revision: None,
                corner: None,
            })
        }
    }
    /// Have DWM round it as `corner` (`corner_of`).
    fn set_corner(&mut self, corner: DWM_WINDOW_CORNER_PREFERENCE) {
        if self.corner.replace(corner) == Some(corner) {
            return;
        }
        // SAFETY: an attribute of our own window, from a local of its type.
        let _ = unsafe {
            DwmSetWindowAttribute(
                self.hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &corner as *const _ as *const _,
                size_of_val(&corner) as u32,
            )
        };
    }
    /// Its renderer, made for `scale` (again for a new one).
    fn renderer(&mut self, context: &GpuContext, scale: f32) -> Result<&mut GpuRenderer, GpuError> {
        if self.scale != scale {
            self.renderer = None;
        }
        if self.renderer.is_none() {
            self.scale = scale;
            self.renderer = Some(GpuRenderer::for_overlay_visual(
                self.visual.clone(),
                self.device.clone(),
                context,
                scale,
            )?);
        }
        Ok(self.renderer.as_mut().expect("made above"))
    }
    /// Place it at `rect` (its parent's coordinates): its origin from the
    /// parent's client origin at the parent's scale, its size at its own.
    fn place(&mut self, origin: (i32, i32), parent_scale: f32, rect: Rect) {
        self.rect = Some(rect);
        let screen = (
            origin.0 + (rect.x * parent_scale).round() as i32,
            origin.1 + (rect.y * parent_scale).round() as i32,
            (rect.width * self.scale).round().max(1.) as i32,
            (rect.height * self.scale).round().max(1.) as i32,
        );
        self.target.origin.set((rect.x, rect.y));
        self.target.scale.set(self.scale);
        if self.screen == Some(screen) {
            return;
        }
        self.screen = Some(screen);
        // SAFETY: moves our own window, never activating it.
        let _ = unsafe {
            SetWindowPos(
                self.hwnd,
                None,
                screen.0,
                screen.1,
                screen.2,
                screen.3,
                SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER,
            )
        };
    }
    /// Show it over `parent`, which owns it from now on.
    fn show(&self, parent: HWND, send: &Sink) {
        *self.target.send.borrow_mut() = Some(send.clone());
        self.target.parent.set(parent.0 as usize);
        // SAFETY: our window; its owner is a live window of this thread.
        unsafe {
            SetWindowLongPtrW(self.hwnd, GWLP_HWNDPARENT, parent.0 as isize);
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOP),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }
    fn hide(&mut self) {
        // SAFETY: our own window.
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
            if GetCapture() == self.hwnd {
                let _ = ReleaseCapture();
            }
            SetWindowLongPtrW(self.hwnd, GWLP_HWNDPARENT, 0);
        }
        *self.target.send.borrow_mut() = None;
        self.target.parent.set(0);
        self.target.tracking.set(false);
        self.rect = None;
        self.screen = None;
        self.revision = None;
        if let Some(renderer) = &mut self.renderer {
            renderer.trim();
        }
    }
}
impl Drop for Panel {
    fn drop(&mut self) {
        self.renderer = None;
        // SAFETY: our own window; its slot is cleared before it goes.
        unsafe {
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

thread_local! {
    /// Hidden windows, ready to show.
    static POOL: RefCell<Vec<Panel>> = const { RefCell::new(Vec::new()) };
    /// Windows that have shown popups, their hosts, client origins and
    /// shown popup windows.
    static PARENTS: RefCell<Vec<Parent>> = const { RefCell::new(Vec::new()) };
    /// No popup window could be made (no Direct3D 12): popups stay inside
    /// their windows from then on.
    static UNSUPPORTED: Cell<bool> = const { Cell::new(false) };
}
struct Parent {
    hwnd: usize,
    send: Sink,
    origin: (i32, i32),
    popups: Vec<usize>,
}

fn acquire() -> Result<Panel, String> {
    match POOL.with(|pool| pool.borrow_mut().pop()) {
        Some(panel) => Ok(panel),
        None => Panel::new(),
    }
}
fn release(mut panel: Panel) {
    panel.hide();
    POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        if pool.len() < pool_size() {
            pool.push(panel);
        }
    });
}

/// Have a window and its renderer ready for the popup about to open (the
/// pointer reached something that opens one, `Ui::on_popup_soon`), if none
/// is. It is shown once, empty and off screen, then hidden: a window's
/// first showing is what costs.
pub(crate) fn warm(context: &GpuContext, scale: f32) {
    if !enabled() || POOL.with(|pool| !pool.borrow().is_empty()) {
        return;
    }
    let Ok(mut panel) = Panel::new() else {
        UNSUPPORTED.set(true);
        return;
    };
    if panel.renderer(context, scale).is_err() {
        UNSUPPORTED.set(true);
        return;
    }
    // SAFETY: our own window, shown without activating, then hidden.
    unsafe {
        let _ = SetWindowPos(
            panel.hwnd,
            None,
            -32_000,
            -32_000,
            1,
            1,
            SWP_NOACTIVATE | SWP_NOZORDER | SWP_SHOWWINDOW,
        );
        let _ = ShowWindow(panel.hwnd, SW_HIDE);
    }
    POOL.with(|pool| pool.borrow_mut().push(panel));
}

/// Whether popups show in windows of their own: but with
/// `ZGUI_POPUP_WINDOWS=0` (inside their window, as elsewhere), or once none
/// could be made.
pub(crate) fn enabled() -> bool {
    thread_local! {
        static ON: bool = std::env::var_os("ZGUI_POPUP_WINDOWS").is_none_or(|v| v != "0");
    }
    ON.with(|on| *on) && !UNSUPPORTED.get()
}

fn hwnd_of(window: &winit::window::Window) -> Option<HWND> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(HWND(handle.hwnd.get() as *mut _)),
        _ => None,
    }
}
/// The window's client origin on the screen, in its points (physical pixels
/// at its scale): global coordinates, as a platform drag reports them.
pub(crate) fn content_origin(window: &winit::window::Window) -> Option<(f64, f64)> {
    let hwnd = hwnd_of(window)?;
    let (x, y) = client_origin(hwnd);
    let scale = f64::from(window_scale(hwnd));
    Some((f64::from(x) / scale, f64::from(y) / scale))
}
// todo(windows): drags between windows need the window under a point
// (`WindowFromPoint`) in global coordinates every window shares; until then
// a drag stays in its window.
pub(crate) fn window_number(_: &winit::window::Window) -> Option<isize> {
    None
}
pub(crate) fn window_at(_: f64, _: f64) -> Option<isize> {
    None
}
fn client_origin(hwnd: HWND) -> (i32, i32) {
    let mut origin = POINT::default();
    // SAFETY: converts into a local, for our own window.
    let _ = unsafe { ClientToScreen(hwnd, &mut origin) };
    (origin.x, origin.y)
}
fn window_scale(hwnd: HWND) -> f32 {
    // SAFETY: reads our own window's DPI.
    unsafe { GetDpiForWindow(hwnd) }.max(96) as f32 / 96.
}

/// Where popups may show: each monitor's work area (less the taskbar), in
/// the window's coordinates. None when popups stay inside their window.
pub(crate) fn popup_screens(window: &winit::window::Window) -> Vec<Rect> {
    if !enabled() {
        return Vec::new();
    }
    let Some(hwnd) = hwnd_of(window) else {
        return Vec::new();
    };
    let (x, y) = client_origin(hwnd);
    let scale = window_scale(hwnd);
    work_areas()
        .into_iter()
        .map(|area| {
            Rect::new(
                (area.left - x) as f32 / scale,
                (area.top - y) as f32 / scale,
                (area.right - area.left) as f32 / scale,
                (area.bottom - area.top) as f32 / scale,
            )
        })
        .collect()
}
fn work_areas() -> Vec<RECT> {
    unsafe extern "system" fn each(monitor: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        // SAFETY: `data` is the vector `work_areas` passes, alive for the
        // enumeration; the monitor is the system's.
        unsafe {
            if GetMonitorInfoW(monitor, &mut info).as_bool() {
                (*(data.0 as *mut Vec<RECT>)).push(info.rcWork);
            }
        }
        true.into()
    }
    let mut areas: Vec<RECT> = Vec::new();
    // SAFETY: the callback writes only into `areas`, during this call.
    let _ = unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(each),
            LPARAM(&mut areas as *mut _ as isize),
        )
    };
    areas
}
/// The scale of the monitor holding `rect`'s centre (the parent's
/// coordinates): a popup draws at the scale it shows at.
fn scale_at(origin: (i32, i32), parent_scale: f32, rect: Rect) -> f32 {
    let point = POINT {
        x: origin.0 + ((rect.x + rect.width / 2.) * parent_scale) as i32,
        y: origin.1 + ((rect.y + rect.height / 2.) * parent_scale) as i32,
    };
    let (mut x, mut y) = (0, 0);
    // SAFETY: reads the system's monitor and its DPI into locals.
    unsafe {
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y).is_err() {
            return parent_scale;
        }
    }
    x.max(96) as f32 / 96.
}

/// The parent's subclass: popups follow it, and close as the app
/// deactivates or its frame is pressed.
unsafe extern "system" fn parent_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    let send = |input: PopupInput| {
        let send = PARENTS.with(|parents| {
            let parents = parents.borrow();
            let parent = parents
                .iter()
                .find(|parent| parent.hwnd == hwnd.0 as usize)?;
            (!parent.popups.is_empty() || matches!(input, PopupInput::Screens))
                .then(|| parent.send.clone())
        });
        if let Some(send) = send {
            send(input);
        }
    };
    match message {
        WM_ACTIVATEAPP if wparam.0 == 0 => send(PopupInput::Dismiss),
        WM_NCLBUTTONDOWN | WM_NCRBUTTONDOWN | WM_NCMBUTTONDOWN => send(PopupInput::Dismiss),
        WM_DISPLAYCHANGE => send(PopupInput::Screens),
        WM_SETTINGCHANGE if wparam.0 as u32 == SPI_SETWORKAREA.0 => send(PopupInput::Screens),
        WM_WINDOWPOSCHANGED => {
            // SAFETY: the system's WINDOWPOS for this message.
            let flags = unsafe { (*(lparam.0 as *const WINDOWPOS)).flags };
            if !flags.contains(SWP_NOMOVE) {
                follow(hwnd);
            }
        }
        WM_NCDESTROY => {
            // SAFETY: removes this subclass from the window going away.
            let _ = unsafe { RemoveWindowSubclass(hwnd, Some(parent_proc), SUBCLASS_ID) };
        }
        _ => {}
    }
    // SAFETY: the next procedure in the window's chain.
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}
/// Move `parent`'s popup windows as far as it moved.
fn follow(parent: HWND) {
    let origin = client_origin(parent);
    let moved = PARENTS.with(|parents| {
        let mut parents = parents.borrow_mut();
        let entry = parents
            .iter_mut()
            .find(|entry| entry.hwnd == parent.0 as usize)?;
        let delta = (origin.0 - entry.origin.0, origin.1 - entry.origin.1);
        entry.origin = origin;
        (delta != (0, 0)).then(|| (delta, entry.popups.clone()))
    });
    let Some(((dx, dy), popups)) = moved else {
        return;
    };
    for popup in popups {
        let hwnd = HWND(popup as *mut _);
        let mut rect = RECT::default();
        // SAFETY: moves a popup window of ours, never activating it.
        unsafe {
            if windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut rect).is_ok() {
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    rect.left + dx,
                    rect.top + dy,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_NOACTIVATE,
                );
            }
        }
    }
}

/// A window's shown popups.
pub(crate) struct PopupHost {
    shown: Vec<(NodeId, Panel)>,
    send: Sink,
    /// The window they show over, once any has.
    parent: Option<usize>,
}
impl PopupHost {
    pub fn new(send: impl Fn(PopupInput) + 'static) -> Self {
        Self {
            shown: Vec::new(),
            send: Rc::new(send),
            parent: None,
        }
    }
    /// Whether any popup shows.
    pub fn showing(&self) -> bool {
        !self.shown.is_empty()
    }
    /// No popup window can be made: popups go back inside the window.
    fn unsupported(&mut self, error: impl std::fmt::Display) {
        eprintln!("zgui: popups stay inside their window: {error}");
        UNSUPPORTED.set(true);
        for (_, panel) in self.shown.drain(..) {
            drop(panel);
        }
        POOL.with(|pool| pool.borrow_mut().clear());
        (self.send)(PopupInput::Screens);
    }
    /// After the scene's flush: show each visible popup root in a window
    /// over `window`, draw it again where it changed (its layer revision,
    /// `Scene::set_popup`; always when `full`), and hide the rest.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        window: &winit::window::Window,
        scene: &Scene,
        _semantics: &Semantics,
        context: &GpuContext,
        scale: f32,
        full: bool,
        // Drag previews stay in their window here (`Ui::set_popup_previews`).
        _drag: Option<&zgui::input::DragState>,
        _ended: Option<zgui::input::DragEnded>,
    ) -> Result<(), GpuError> {
        if self.shown.is_empty() && scene.popups().is_empty() {
            return Ok(());
        }
        let Some(parent) = hwnd_of(window) else {
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
            let (root, panel) = self.shown.swap_remove(index);
            zgui_gpu::present_log::log(|| format!("popup hide {root:?}"));
            release(panel);
        }
        let origin = client_origin(parent);
        let parent_scale = scale;
        if !wanted.is_empty() && self.parent.replace(parent.0 as usize).is_none() {
            // SAFETY: a subclass of our own window, on its thread; it holds
            // no data of its own.
            let _ = unsafe { SetWindowSubclass(parent, Some(parent_proc), SUBCLASS_ID, 0) };
        }
        for (root, bounds) in wanted {
            let new = !self.shown.iter().any(|(shown, _)| *shown == root);
            if new {
                zgui_gpu::present_log::log(|| format!("popup show {root:?} at {bounds:?}"));
                match acquire() {
                    Ok(panel) => self.shown.push((root, panel)),
                    Err(error) => {
                        self.unsupported(error);
                        return Ok(());
                    }
                }
            }
            let panel = &mut self
                .shown
                .iter_mut()
                .find(|(shown, _)| *shown == root)
                .expect("pushed above")
                .1;
            // Its monitor's scale, found as it shows.
            let popup_scale = match new {
                true => scale_at(origin, parent_scale, bounds),
                false => panel.scale,
            };
            // Popups are small: one that changed is drawn whole.
            let revision = scene.layer_revision(root);
            let changed = new || full || panel.revision != Some(revision);
            panel.revision = Some(revision);
            let renderer = match panel.renderer(context, popup_scale) {
                Ok(renderer) => renderer,
                Err(error) => {
                    self.unsupported(error);
                    return Ok(());
                }
            };
            renderer.set_popup(root, bounds);
            let rect = renderer.plane_rect().unwrap_or(bounds);
            let resized = panel
                .rect
                .is_none_or(|old| (old.width, old.height) != (rect.width, rect.height));
            if changed || resized {
                let renderer = panel.renderer.as_mut().expect("made above");
                renderer.repaint_all();
                renderer.render(scene, &[])?;
                renderer.present()?;
            }
            panel.place(origin, parent_scale, rect);
            panel.set_corner(corner_of(scene, root, rect));
            if new {
                // Its first frame is in its visual before it shows.
                // SAFETY: our device, on its thread.
                let _ = unsafe { panel.device.Commit() };
                panel.show(parent, &self.send);
            }
        }
        let popups: Vec<usize> = self
            .shown
            .iter()
            .map(|(_, panel)| panel.hwnd.0 as usize)
            .collect();
        PARENTS.with(|parents| {
            let mut parents = parents.borrow_mut();
            match parents
                .iter_mut()
                .find(|entry| entry.hwnd == parent.0 as usize)
            {
                Some(entry) => {
                    entry.origin = origin;
                    entry.popups = popups;
                }
                None => parents.push(Parent {
                    hwnd: parent.0 as usize,
                    send: self.send.clone(),
                    origin,
                    popups,
                }),
            }
        });
        Ok(())
    }
    /// A drag's preview stays in its window here: nothing to move, show or
    /// read back.
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
    /// Hide every popup (the window closes or goes away).
    pub fn hide_all(&mut self, _: &winit::window::Window) {
        for (_, panel) in self.shown.drain(..) {
            release(panel);
        }
        self.forget_popups();
    }
    fn forget_popups(&self) {
        if let Some(parent) = self.parent {
            PARENTS.with(|parents| {
                if let Some(entry) = parents
                    .borrow_mut()
                    .iter_mut()
                    .find(|entry| entry.hwnd == parent)
                {
                    entry.popups.clear();
                }
            });
        }
    }
}
impl Drop for PopupHost {
    fn drop(&mut self) {
        for (_, panel) in self.shown.drain(..) {
            release(panel);
        }
        if let Some(parent) = self.parent {
            PARENTS.with(|parents| parents.borrow_mut().retain(|entry| entry.hwnd != parent));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zgui::scene::{Layout, QuadStyle, Style};

    /// A popup root holding one panel `size` points square, rounded `radius`.
    fn popup(size: f32, radius: f32) -> (Scene, NodeId) {
        let mut scene = Scene::new(400., 300.);
        let root = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Column),
            Style::default(),
        );
        scene.append(
            root,
            NodeKind::Quad(QuadStyle {
                radius,
                ..Default::default()
            }),
            Style {
                width: Some(size),
                height: Some(size),
                ..Default::default()
            },
        );
        let _ = scene.flush();
        (scene, root)
    }

    #[test]
    fn a_rounded_panel_filling_its_window_takes_dwm_s_corners() {
        let window = Rect::new(0., 0., 100., 100.);
        let (scene, root) = popup(100., 8.);
        assert_eq!(corner_of(&scene, root, window), DWMWCP_ROUND);
        let (scene, root) = popup(100., 4.);
        assert_eq!(corner_of(&scene, root, window), DWMWCP_ROUNDSMALL);
        let (scene, root) = popup(100., 0.);
        assert_eq!(corner_of(&scene, root, window), DWMWCP_DONOTROUND);
        // Painting past its panel (a shadow of its own), zgui draws it all.
        let (scene, root) = popup(80., 8.);
        assert_eq!(corner_of(&scene, root, window), DWMWCP_DONOTROUND);
    }

    #[test]
    fn a_popup_window_s_pixels_are_its_parent_s_points() {
        let target = Target::default();
        target.origin.set((16., 428.));
        target.scale.set(1.5);
        // Its client pixel (30, 45) is 20 by 30 points into the popup.
        assert_eq!(target.point((30, 45)), (36., 458.));
        // Dragged out above it (captured), past its edge.
        assert_eq!(target.point((-15, -30)), (6., 408.));
    }
}
