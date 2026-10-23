//! Where a new window opens (macOS): with a given point of its content under
//! the pointer (`WindowOptions::under_pointer`), as a tab torn off its window
//! opens where it was dropped.
//!
//! todo(windows): `GetCursorPos` for the pointer, then `SetWindowPos` before
//! the window shows. todo(x11): `XQueryPointer` could place it; the window
//! manager may still move it. todo(wayland): the compositor places new
//! windows, and the app cannot.

/// Put content point `(x, y)` (logical) of `window` (not shown yet) under
/// the pointer.
#[cfg(target_os = "macos")]
pub(crate) fn under_pointer(window: &winit::window::Window, (x, y): (f64, f64)) {
    use objc2::rc::Retained;
    use objc2_app_kit::{NSEvent, NSView};
    use objc2_foundation::NSPoint;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return;
    };
    // SAFETY: winit's content view, alive while its window is.
    let Some(view) = (unsafe { Retained::<NSView>::retain(handle.ns_view.as_ptr().cast()) }) else {
        return;
    };
    let Some(window) = view.window() else {
        return;
    };
    let frame = window.frame();
    let content = window.contentRectForFrameRect(frame);
    // Screen space runs bottom-up: the content's top is above the pointer by
    // `y`, the frame's by the title bar's height more.
    let pointer = NSEvent::mouseLocation();
    let title = (frame.origin.y + frame.size.height) - (content.origin.y + content.size.height);
    window.setFrameTopLeftPoint(NSPoint::new(
        pointer.x - x - (content.origin.x - frame.origin.x),
        pointer.y + y + title,
    ));
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn under_pointer(_: &winit::window::Window, _: (f64, f64)) {}
