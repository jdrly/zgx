//! The focused editor's caret blinking as the system's do (`Ui::set_caret_blink`).
//!
//! Each blink recolours the caret's rectangle, so it damages that rectangle
//! alone and changes no geometry; the host wakes for it as for any other
//! interaction deadline (`Ui::next_interaction_deadline`), so a blinking
//! caret costs one small frame a phase and no frame loop. It shows solid
//! while the editor redraws it (typing, selecting, moving the caret), and
//! stops, solid, after the system's timeout, while the window is not
//! presented, and as the editor loses the focus (its window deactivating).
use crate::scene::{Color, NodeId, NodeKind};
use crate::widgets::Ui;
use std::cell::Cell;
use std::time::{Duration, Instant};

/// How a caret blinks: shown for `on`, hidden for `off`, and after `timeout`
/// without the user typing or moving it, solid. Read from the system by the
/// desktop host (Windows' `GetCaretBlinkTime` and caret timeout, macOS's
/// insertion point blink periods, GTK's `cursor-blink-time` and
/// `cursor-blink-timeout`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaretBlink {
    /// How long each blink shows the caret.
    pub on: Duration,
    /// How long each blink hides it.
    pub off: Duration,
    /// `None`: as long as the editor has the focus.
    pub timeout: Option<Duration>,
}

impl CaretBlink {
    /// The shortest phase: shorter would be a flicker, not a blink.
    const SHORTEST: Duration = Duration::from_millis(50);

    fn held(self) -> Self {
        Self {
            on: self.on.max(Self::SHORTEST),
            off: self.off.max(Self::SHORTEST),
            timeout: self.timeout,
        }
    }
}

/// The document's one blinking caret: the focused editor's.
#[derive(Default)]
pub(crate) struct Blink {
    settings: Cell<Option<CaretBlink>>,
    caret: Cell<Option<NodeId>>,
    /// The caret's colour while it is shown: hidden, it is this, transparent.
    color: Cell<Option<Color>>,
    hidden: Cell<bool>,
    next: Cell<Option<Instant>>,
    /// When it stops, solid (`CaretBlink::timeout`).
    stop: Cell<Option<Instant>>,
}

impl Blink {
    /// The editor drew its caret shown (as the user typed, selected or moved
    /// it, or it took the focus): solid now, blinking from a phase on.
    pub(crate) fn restart(&self, caret: NodeId, color: Color, now: Instant) {
        self.caret.set(Some(caret));
        self.color.set(Some(color));
        self.hidden.set(false);
        let settings = self.settings.get();
        self.next.set(settings.map(|s| now + s.on));
        self.stop.set(
            settings
                .and_then(|s| s.timeout)
                .map(|timeout| now + timeout),
        );
    }

    /// The editor drew its caret hidden (it lost the focus, or composes).
    pub(crate) fn release(&self, caret: NodeId) {
        if self.caret.get() == Some(caret) {
            self.caret.set(None);
            self.hidden.set(false);
            self.next.set(None);
            self.stop.set(None);
        }
    }

    fn set(&self, ui: &Ui, settings: Option<CaretBlink>) {
        let settings = settings.map(CaretBlink::held);
        if self.settings.replace(settings) == settings {
            return;
        }
        if let (Some(caret), Some(color)) = (self.caret.get(), self.color.get()) {
            self.show(ui, caret, color);
            self.restart(caret, color, Instant::now());
        }
    }

    fn show(&self, ui: &Ui, caret: NodeId, color: Color) {
        if self.hidden.replace(false) {
            let mut scene = ui.scene.borrow_mut();
            if scene.contains(caret) {
                scene.set_kind(caret, NodeKind::Rect(color));
            }
        }
    }
}

impl Ui {
    /// Blink the focused editor's caret (`CaretBlink`), as the system's
    /// carets blink; `None` (the default, as before a host sets it) keeps
    /// it solid. The desktop host follows the system's setting.
    pub fn set_caret_blink(&self, blink: Option<CaretBlink>) {
        self.caret_blink().set(self, blink);
    }

    /// How the caret blinks (`Ui::set_caret_blink`).
    pub fn caret_blink_settings(&self) -> Option<CaretBlink> {
        self.caret_blink().settings.get()
    }

    /// Whether the focused editor's caret is in a blink's hidden phase.
    pub fn caret_blinked_off(&self) -> bool {
        self.caret_blink().hidden.get()
    }
}

/// When the caret next changes, while one blinks in a presented window.
pub(crate) fn next(ui: &Ui) -> Option<Instant> {
    let blink = ui.caret_blink();
    let caret = blink.caret.get()?;
    if !ui.is_presented() || !ui.scene.borrow().contains(caret) {
        return None;
    }
    blink.next.get()
}

/// Show or hide the caret if its phase is over, or stop it, solid, at its
/// timeout. Whether it changed.
pub(crate) fn advance(ui: &Ui, now: Instant) -> bool {
    let blink = ui.caret_blink();
    let (Some(caret), Some(color), Some(settings)) =
        (blink.caret.get(), blink.color.get(), blink.settings.get())
    else {
        return false;
    };
    let Some(due) = blink.next.get().filter(|due| *due <= now) else {
        return false;
    };
    if !ui.is_presented() || !ui.scene.borrow().contains(caret) {
        return false;
    }
    if blink.stop.get().is_some_and(|stop| stop <= now) {
        let changed = blink.hidden.get();
        blink.show(ui, caret, color);
        blink.next.set(None);
        return changed;
    }
    let hidden = !blink.hidden.get();
    blink.hidden.set(hidden);
    let shown = if hidden {
        Color(color.0, color.1, color.2, 0)
    } else {
        color
    };
    ui.scene.borrow_mut().set_kind(caret, NodeKind::Rect(shown));
    // From the phase's own start, unless the wake came a phase late.
    let phase = if hidden { settings.off } else { settings.on };
    let start = if now.saturating_duration_since(due) < phase {
        due
    } else {
        now
    };
    blink.next.set(Some(start + phase));
    true
}
