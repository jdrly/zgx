//! The platform's own colours, which the themes take over their built-in
//! values (`Themes::set_system_tokens`): today the text selection's, so a
//! selection looks as the system draws one and follows the user's accent.
//! An app theme or `theme_scope` that sets these tokens keeps its own.
// Linux reads no system values yet.
#[cfg_attr(not(any(target_os = "macos", windows)), allow(unused_imports))]
use zgui::{scene::Color, theme::tok};

/// The system's selection colours, as theme tokens: macOS's in a key window
/// and in one at the back; Windows' accent, the selection colour of its own
/// text fields (WinUI's, the new Notepad's), with white text over it. None
/// where zgui does not read them (Linux: a hook for its host), and the
/// built-in theme's apply.
fn selection() -> Vec<(zgui::theme::TokenId, Color)> {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{NSColor, NSColorSpace};
        let srgb = |color: objc2::rc::Retained<NSColor>| {
            let color = color.colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
            let channel = |value: f64| (value.clamp(0., 1.) * 255.).round() as u8;
            Some(Color(
                channel(color.redComponent()),
                channel(color.greenComponent()),
                channel(color.blueComponent()),
                channel(color.alphaComponent()),
            ))
        };
        let (Some(active), Some(inactive)) = (
            srgb(NSColor::selectedTextBackgroundColor()),
            srgb(NSColor::unemphasizedSelectedTextBackgroundColor()),
        ) else {
            return Vec::new();
        };
        vec![
            (tok::SELECTION.0, active),
            (tok::SELECTION_INACTIVE.0, inactive),
        ]
    }
    #[cfg(windows)]
    {
        use windows::UI::ViewManagement::{UIColorType, UISettings};
        let Some(accent) = UISettings::new()
            .and_then(|settings| settings.GetColorValue(UIColorType::Accent))
            .ok()
        else {
            return Vec::new();
        };
        vec![
            (
                tok::SELECTION.0,
                Color(accent.R, accent.G, accent.B, accent.A),
            ),
            (tok::SELECTION_TEXT.0, Color(255, 255, 255, 255)),
            // At the back the built-in grey selection shows; its text keeps
            // its colour (white over the grey would vanish).
            (tok::SELECTION_TEXT_INACTIVE.0, Color(0, 0, 0, 0)),
        ]
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        Vec::new()
    }
}

/// Read the system's colours again and give them to `ui`'s themes: at
/// startup, and when the system's appearance or the window's activity
/// changes (the user may have picked another accent meanwhile).
pub(crate) fn sync(ui: &zgui::widgets::Ui) {
    let tokens = selection();
    if !tokens.is_empty() {
        ui.theme().set_system_tokens(
            tokens
                .into_iter()
                .map(|(token, color)| (token, color.into())),
        );
    }
}
