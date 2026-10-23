//! Where a new window opens (macOS): with a given point of its content under
//! the pointer (`WindowOptions::under_pointer`: a tab torn off its window
//! opens where it was dropped), where it was last left
//! (`WindowOptions::frame_autosave`), or, placed nowhere in particular,
//! cascaded from the app's front window, as AppKit's document windows are,
//! rather than over it. It also joins AppKit's window tabs as `Tabbing`
//! says.
//!
//! Windows (`win32`): placed nowhere in particular, a window opens where
//! Windows puts it (`CW_USEDEFAULT`: each new window a step on from the last,
//! as Notepad's). A saved frame is its placement (`GetWindowPlacement`: the
//! restored frame, and whether it was maximized), kept in the registry under
//! `HKCU\Software\zgui\<executable>\Frames`, and reopened only where a monitor
//! still shows its title bar. AppKit's window tabs have no Windows
//! counterpart: `Tabbing` does nothing there.
//! todo(x11): `XQueryPointer` could place it; the window manager may still
//! move it. todo(wayland): the compositor places new windows, and the app
//! cannot.

/// AppKit's window tabs (macOS; elsewhere none): an app's windows gather as
/// tabs of one window, as Finder's do.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Tabbing {
    /// As the user's setting says (System Settings ▸ Desktop & Dock ▸
    /// Prefer tabs when opening documents), and with Window ▸ Merge All
    /// Windows.
    #[default]
    Automatic,
    /// New windows open as tabs of the front window.
    Preferred,
    /// Never: for windows with tabs of their own (zgui's), as Safari's.
    Disallowed,
}

/// How `place` left a window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Placed {
    /// Its saved frame: its size is that, not its options'.
    pub restored: bool,
    /// It was left maximized: it opens so.
    pub maximized: bool,
}

/// Place `window` (not shown yet): `under_pointer` is the content point
/// (logical) to put under the pointer; `placed`, whether its options gave it
/// a position already.
#[cfg(target_os = "macos")]
pub(crate) fn place(
    window: &winit::window::Window,
    under_pointer: Option<(f64, f64)>,
    placed: bool,
    options: &crate::WindowOptions,
) -> Placed {
    use objc2::{MainThreadOnly, rc::Retained};
    use objc2_app_kit::{NSApplication, NSEvent, NSView, NSWindowTabbingMode};
    use objc2_foundation::{NSPoint, NSString};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = window.window_handle() else {
        return Placed::default();
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Placed::default();
    };
    // SAFETY: winit's content view, alive while its window is.
    let Some(view) = (unsafe { Retained::<NSView>::retain(handle.ns_view.as_ptr().cast()) }) else {
        return Placed::default();
    };
    let Some(window) = view.window() else {
        return Placed::default();
    };
    window.setTabbingMode(match options.tabbing {
        Tabbing::Automatic => NSWindowTabbingMode::Automatic,
        Tabbing::Preferred => NSWindowTabbingMode::Preferred,
        Tabbing::Disallowed => NSWindowTabbingMode::Disallowed,
    });
    if let Some(identifier) = &options.tabbing_identifier {
        window.setTabbingIdentifier(&NSString::from_str(identifier));
    }
    // Where it was last left, if it was: kept from now on.
    let mut placed = placed;
    if let Some(name) = &options.frame_autosave {
        let name = NSString::from_str(name);
        placed |= under_pointer.is_none() && window.setFrameUsingName(&name);
        window.setFrameAutosaveName(&name);
    }
    let frame = window.frame();
    let content = window.contentRectForFrameRect(frame);
    if let Some((x, y)) = under_pointer {
        // Screen space runs bottom-up: the content's top is above the
        // pointer by `y`, the frame's by the title bar's height more.
        let pointer = NSEvent::mouseLocation();
        let title = (frame.origin.y + frame.size.height) - (content.origin.y + content.size.height);
        window.setFrameTopLeftPoint(NSPoint::new(
            pointer.x - x - (content.origin.x - frame.origin.x),
            pointer.y + y + title,
        ));
        return Placed::default();
    }
    if placed {
        return Placed::default();
    }
    // Cascaded from the app's front window: placed at its top-left, then
    // one step on (AppKit's offset, kept on its screen).
    // Not yet main or key as the app starts: then another window that can
    // be (not a popup's panel).
    let app = NSApplication::sharedApplication(window.mtm());
    let front = app
        .mainWindow()
        .or_else(|| app.keyWindow())
        .filter(|other| *other != window)
        .or_else(|| {
            app.windows()
                .iter()
                .find(|other| *other != window && other.canBecomeMainWindow())
        });
    if let Some(front) = front {
        let frame = front.frame();
        let top_left = NSPoint::new(frame.origin.x, frame.origin.y + frame.size.height);
        let next = window.cascadeTopLeftFromPoint(top_left);
        window.cascadeTopLeftFromPoint(next);
    }
    Placed::default()
}

#[cfg(windows)]
mod win32;
#[cfg(windows)]
pub(crate) use win32::place;

#[cfg(not(any(target_os = "macos", windows)))]
pub(crate) fn place(
    _: &winit::window::Window,
    _: Option<(f64, f64)>,
    _: bool,
    _: &crate::WindowOptions,
) -> Placed {
    Placed::default()
}
