//! The system's scroll bar style: overlay scrollbars, hidden until a view
//! scrolls (`Ui::set_overlay_scrollbars`), where the user has them.

/// Whether the system draws overlay scrollbars: macOS's "Show scroll bars"
/// (automatic follows the pointing device: a trackpad's overlay, a mouse's
/// legacy); Windows' "Always show scrollbars" off (Accessibility, Visual
/// effects; Windows 10 2004 on). `None` where zgui does not read it, and
/// scrollbars always show.
fn overlay() -> Option<bool> {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{NSScroller, NSScrollerStyle};
        let main = objc2::MainThreadMarker::new()?;
        Some(NSScroller::preferredScrollerStyle(main) == NSScrollerStyle::Overlay)
    }
    #[cfg(windows)]
    {
        use windows::UI::ViewManagement::UISettings;
        UISettings::new()
            .and_then(|settings| settings.AutoHideScrollBars())
            .ok()
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        None
    }
}

/// Read the system's style again and give it to `ui`: at startup, when the
/// window comes to the front (the user may have changed it meanwhile), and
/// on Windows when the system's settings change.
pub(crate) fn sync(ui: &zgui::widgets::Ui) {
    if let Some(overlay) = overlay() {
        ui.set_overlay_scrollbars(overlay);
    }
}
