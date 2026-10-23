//! Multi-window on macOS: popups past the window's edges (menus, a submenu,
//! a select, a popover with a text field, a context menu, a tooltip), each
//! near an edge of a small window; tabs dragged between windows, or torn
//! off into a window of their own; and text, a link and a file dragged out
//! to other apps (Finder, Mail, a browser). Elsewhere popups and drags stay
//! inside their window.
//!
//! `--open=file|recent|select|details` opens one at start and
//! `--at=X,Y` places the window (physical pixels), for screenshots.
use std::{cell::Cell, rc::Rc};
use zgui::{
    compose::{Placement, PlacementEdge, PlacementSide, prelude::*},
    input::{DragItem, DragPhase, InputEvent},
    reactive::Signal,
};
use zgui_desktop::{Application, WindowContext, WindowFactory, WindowOptions};

const ACCENT: u32 = 0x3d7ef7;
/// A panel's fill letting AppKit's material show through (a translucent
/// popup panel gets one in its own window); inside a window, a plain one.
const GLASS: u32 = 0x2a2d3440;

/// A menu styled as AppKit's: a rounded panel, tight rows.
fn mac_menu(label: &str, open: Signal<bool>, anchor: View) -> View {
    menu(label, open, anchor)
        .w(220.)
        .p(5.)
        .gap(0.)
        .rounded(12.)
        .bg(rgba(GLASS))
        .text_size(13.)
}
fn item(label: &str) -> View {
    menu_item(label)
        .h(24.)
        .px(10.)
        .py(0.)
        .rounded(5.)
        .bg(rgba(0))
        .hover(|s| s.bg(rgb(ACCENT)).text_color(rgb(0xffffff)))
        .focus(|s| s.bg(rgb(ACCENT)).text_color(rgb(0xffffff)))
}
fn separator() -> View {
    div().h(1.).mx(10.).my(5.).bg(rgba(0x80808040))
}
fn toolbar_button(label: &str) -> View {
    button().h(28.).px(12.).text_size(13.).child(text(label))
}

fn file_menu(open: Signal<bool>, recent: Signal<bool>, windows: WindowFactory) -> View {
    let anchor = toolbar_button("File").on_click({
        let open = open.clone();
        move || {
            open.set(true);
        }
    });
    mac_menu("File", open, anchor).children([
        item("New Window").on_click(move || {
            let tabs = vec![Tab::new("Untitled")];
            let factory = windows.clone();
            windows.open(options(None), move |cx| document(cx, factory, tabs, None));
        }),
        item("New Tab"),
        item("Open…"),
        submenu("Open Recent", recent)
            .trigger(
                item("Open Recent").child(
                    row()
                        .w_full()
                        .justify_between()
                        .child(text("Open Recent"))
                        .child(text("›")),
                ),
            )
            .w(260.)
            .p(5.)
            .rounded(12.)
            .bg(rgba(GLASS))
            .text_size(13.)
            .children(
                [
                    "Quarterly report.numbers",
                    "Launch plan.pages",
                    "Wallpaper.heic",
                    "notes.md",
                    "zgui",
                ]
                .map(item),
            ),
        separator(),
        item("Close Window"),
        item("Save"),
        item("Save As…"),
        separator(),
        item("Print…"),
    ])
}

fn select(open: Signal<bool>, chosen: Signal<String>) -> View {
    let label = chosen.clone();
    let anchor = button()
        .h(28.)
        .w(160.)
        .px(10.)
        .text_size(13.)
        .child(
            row()
                .w_full()
                .justify_between()
                .child(text_signal(move || label.get()))
                .child(text("⌃⌄").text_size(10.)),
        )
        .on_click({
            let open = open.clone();
            move || {
                open.set(true);
            }
        });
    let sizes = [
        "Extra small",
        "Small",
        "Medium",
        "Large",
        "Extra large",
        "Huge",
        "Enormous",
        "Gigantic",
        "Colossal",
        "Astronomical",
    ];
    mac_menu("Size", open, anchor)
        .w(160.)
        .children(sizes.map(|size| {
            let chosen = chosen.clone();
            let check = chosen.clone();
            menu_item(size)
                .h(24.)
                .px(8.)
                .rounded(5.)
                .bg(rgba(0))
                .hover(|s| s.bg(rgb(ACCENT)).text_color(rgb(0xffffff)))
                .focus(|s| s.bg(rgb(ACCENT)).text_color(rgb(0xffffff)))
                .child(
                    row()
                        .gap(6.)
                        .child(
                            text_signal(move || {
                                if check.get() == size { "✓" } else { "" }.to_string()
                            })
                            .w(12.),
                        )
                        .child(text(size)),
                )
                .on_click(move || {
                    chosen.set(size.to_string());
                })
        }))
}

fn details(open: Signal<bool>, name: Signal<String>) -> View {
    let anchor = toolbar_button("Details…").on_click({
        let open = open.clone();
        move || {
            open.set(true);
        }
    });
    popover("Details", open, anchor)
        .placement(Placement {
            side: PlacementSide::Right,
            align: PlacementEdge::Center,
            offset: 8.,
        })
        .w(260.)
        .p(14.)
        .gap(10.)
        .rounded(14.)
        .bg(rgba(GLASS))
        .text_size(13.)
        .child(text("Rename").text_size(15.))
        .child(text_input("Name", name).h(28.))
        .child(
            text("Typing goes here while the window keeps its focus: this popover is a window of its own.")
                .text_wrap(true)
                .text_size(12.)
                .opacity(0.7),
        )
}

#[derive(Clone, PartialEq)]
struct Tab {
    id: u64,
    title: String,
}
impl Tab {
    fn new(title: &str) -> Self {
        thread_local! {
            static NEXT: Cell<u64> = const { Cell::new(0) };
        }
        let id = NEXT.with(|next| {
            next.set(next.get() + 1);
            next.get()
        });
        Self {
            id,
            title: title.into(),
        }
    }
}
/// A tab being dragged. `kept`: dropped back on its own window's strip.
#[derive(Clone)]
struct TabDrag {
    tab: Tab,
    kept: Rc<Cell<bool>>,
}

/// The tab strip's height and a tab's top within it.
const STRIP: f32 = 38.;
const TAB_TOP: f32 = 6.;
const STRIP_PAD: f32 = 10.;

fn tab_chip(title: &str, active: bool) -> View {
    div()
        .h(STRIP - TAB_TOP)
        .w(150.)
        .px(12.)
        .items_center()
        .justify_center()
        .rounded(8.)
        .bg(if active {
            rgba(0x80808040)
        } else {
            rgba(0x80808014)
        })
        .text_size(13.)
        .child(text(title))
}

/// A tab strip: tabs dragged within it, to another window's, or out of every
/// window (a window of their own, under the pointer).
fn tab_strip(
    tabs: Signal<Vec<Tab>>,
    active: Signal<u64>,
    window: zgui_desktop::WindowHandle,
    windows: WindowFactory,
    // The tab being dragged (dimmed where it was), and whether a tab hovers
    // the strip (lit, to drop it there).
    dragging: Signal<Option<u64>>,
    hovered: Signal<bool>,
) -> View {
    let ids = tabs.clone();
    let accept = tabs.clone();
    let shown = active.clone();
    let lit = hovered.clone();
    row()
        .w_full()
        .h(STRIP)
        .px(STRIP_PAD)
        .gap(4.)
        .items_end()
        .reactive_style(move || {
            zgui::style::Styles::new().bg(if lit.get() { rgba(0x3d7ef726) } else { rgba(0) })
        })
        .on_event(move |cx| {
            if let InputEvent::Drag(event) = &cx.event
                && event.payload.downcast_ref::<TabDrag>().is_some()
            {
                match event.phase {
                    DragPhase::Over => {
                        hovered.set(true);
                    }
                    DragPhase::Leave | DragPhase::Drop | DragPhase::End => {
                        hovered.set(false);
                    }
                    _ => {}
                }
            }
        })
        .child(
            keyed(
                move || ids.get().iter().map(|tab| tab.id).collect(),
                move |id, _| {
                    let tabs = tabs.clone();
                    let active = active.clone();
                    let title = tabs
                        .with_untracked(|tabs| tabs.iter().find(|tab| tab.id == id).cloned())
                        .map_or_else(String::new, |tab| tab.title);
                    let selected = active.clone();
                    let window = window.clone();
                    let windows = windows.clone();
                    let label = title.clone();
                    let removed = tabs.clone();
                    let torn = tabs.clone();
                    let shown = active.clone();
                    let remove = move |tabs: &Signal<Vec<Tab>>, id: u64| {
                        tabs.update(|tabs| tabs.retain(|tab| tab.id != id));
                        // Its last tab gone, the window goes; its active one,
                        // the first is.
                        match tabs.with_untracked(|tabs| tabs.first().map(|tab| tab.id)) {
                            None => window.close(),
                            Some(first) if shown.with_untracked(|id| *id) == id => {
                                shown.set(first);
                            }
                            Some(_) => {}
                        }
                    };
                    let remove_torn = remove.clone();
                    let (dragged, started, ended) =
                        (dragging.clone(), dragging.clone(), dragging.clone());
                    div()
                        .h(STRIP - TAB_TOP)
                        .w(150.)
                        .px(12.)
                        .items_center()
                        .justify_center()
                        .rounded(8.)
                        .text_size(13.)
                        .reactive_style(move || {
                            zgui::style::Styles::new()
                                .bg(if selected.get() == id {
                                    rgba(0x80808040)
                                } else {
                                    rgba(0x80808014)
                                })
                                .opacity(if dragged.get() == Some(id) { 0.35 } else { 1. })
                        })
                        .child(text(&title))
                        .on_click(move || {
                            active.set(id);
                        })
                        .on_drag({
                            let tabs = tabs.clone();
                            move || {
                                started.set(Some(id));
                                TabDrag {
                                    tab: tabs.with_untracked(|tabs| {
                                        tabs.iter().find(|tab| tab.id == id).cloned().unwrap()
                                    }),
                                    kept: Rc::default(),
                                }
                            }
                        })
                        .drag_preview(move |_: &TabDrag| tab_chip(&label, true))
                        .on_drag_end(move |drag: &TabDrag, accepted| {
                            ended.set(None);
                            if accepted && !drag.kept.get() {
                                remove(&removed, drag.tab.id);
                            }
                        })
                        .on_drag_outside(move |drag: &TabDrag, event| {
                            // A window of its own, the tab under the pointer
                            // where it was grabbed.
                            let tab = drag.tab.clone();
                            let factory = windows.clone();
                            windows.open(
                                options(Some((
                                    (STRIP_PAD + event.grab_x) as f64,
                                    (TAB_TOP + event.grab_y) as f64,
                                ))),
                                move |cx| document(cx, factory, vec![tab], None),
                            );
                            remove_torn(&torn, drag.tab.id);
                        })
                },
            )
            .flex_row()
            .gap(4.)
            .h_full()
            .items_end(),
        )
        .child(div().flex_grow(1.).h_full())
        .on_drop(move |drag: &TabDrag, _| {
            let tab = drag.tab.clone();
            if accept.with_untracked(|tabs| tabs.contains(&tab)) {
                // Back on its own strip: it stays (moved to the end).
                drag.kept.set(true);
                accept.update(|tabs| {
                    tabs.retain(|other| other.id != tab.id);
                    tabs.push(tab.clone());
                });
            } else {
                accept.update(|tabs| tabs.push(tab.clone()));
            }
            shown.set(tab.id);
        })
}

/// Something to drag out to another app.
fn export_chip(label: &str, items: Vec<DragItem>) -> View {
    let title = label.to_string();
    div()
        .h(30.)
        .px(12.)
        .items_center()
        .rounded(8.)
        .bg(rgba(0x3d7ef733))
        .text_size(12.)
        .child(text(label))
        .on_drag(move || items.clone())
        .drag_preview(move |_: &Vec<DragItem>| {
            div()
                .h(30.)
                .px(12.)
                .items_center()
                .rounded(8.)
                .bg(rgba(0x3d7ef7d0))
                .text_color(rgb(0xffffff))
                .text_size(12.)
                .child(text(&title))
        })
        .drag_export(|items: &Vec<DragItem>| items.clone())
}

fn options(under_pointer: Option<(f64, f64)>) -> WindowOptions {
    WindowOptions {
        title: "Documents".into(),
        width: 560.,
        height: 400.,
        under_pointer,
        // Tabs of its own, so none of AppKit's.
        tabbing: zgui_desktop::Tabbing::Disallowed,
        ..Default::default()
    }
}

fn document(
    cx: &mut WindowContext,
    windows: WindowFactory,
    tabs: Vec<Tab>,
    open_at_start: Option<String>,
) {
    let ui = &cx.ui;
    let first = tabs[0].id;
    let tabs = ui.signal(tabs);
    let active = ui.signal(first);
    let title = {
        let tabs = tabs.clone();
        let active = active.clone();
        move || {
            let id = active.get();
            tabs.get()
                .into_iter()
                .find(|tab| tab.id == id)
                .map_or_else(String::new, |tab| tab.title)
        }
    };
    let file = ui.signal(false);
    let recent = ui.signal(false);
    let size = ui.signal(false);
    let chosen = ui.signal(String::from("Medium"));
    let details_open = ui.signal(false);
    let name = ui.signal(String::from("Untitled"));
    let context_choice = ui.signal(String::from("Right-click (or control-click) here"));
    let shown_choice = context_choice.clone();
    let note = std::env::temp_dir().join("zgui-multiwindow-note.txt");
    let _ = std::fs::write(&note, "Dragged out of a zgui window.\n");
    let context = div()
        .flex_grow(1.)
        .w_full()
        .rounded(10.)
        .bg(rgba(0x80808018))
        .p(14.)
        .gap(10.)
        .child(text_signal(title).text_size(20.))
        .child(
            text_signal(move || shown_choice.get())
                .text_size(13.)
                .opacity(0.7),
        )
        .child(div().flex_grow(1.))
        .child(
            text("Drag out to Finder, Mail or a browser:")
                .text_size(12.)
                .opacity(0.6),
        )
        .child(
            row()
                .gap(8.)
                .child(export_chip(
                    "A line of text",
                    vec![DragItem::Text("Dragged out of a zgui window.".into())],
                ))
                .child(export_chip(
                    "A link",
                    vec![DragItem::Url("https://github.com/zeronsh/zgui".into())],
                ))
                .child(export_chip("A file", vec![DragItem::File(note)])),
        )
        .context_menu(move |_| {
            ["Copy", "Paste", "Duplicate", "Move to Trash"]
                .map(|label| {
                    let choice = context_choice.clone();
                    item(label).on_click(move || {
                        choice.set(format!("Chose “{label}”"));
                    })
                })
                .into_iter()
                .collect()
        })
        .context_menu_style(
            zgui::style::Styles::new()
                .w(200.)
                .p(5.)
                .gap(0.)
                .rounded(12.)
                .bg(rgba(GLASS))
                .text_size(13.),
        );
    cx.render(
        column()
            .w_full()
            .h_full()
            .child(tab_strip(
                tabs.clone(),
                active.clone(),
                cx.window.clone(),
                windows.clone(),
                ui.signal(None),
                ui.signal(false),
            ))
            .child(
                column()
                    .w_full()
                    .flex_grow(1.)
                    .p(14.)
                    .gap(12.)
                    .child(
                        row()
                            .w_full()
                            .gap(8.)
                            .items_center()
                            .child(file_menu(file.clone(), recent.clone(), windows))
                            .child(div().flex_grow(1.))
                            .child(
                                toolbar_button("ⓘ")
                                    .tooltip("Tooltips show above the title bar")
                                    .bg(rgba(GLASS))
                                    .rounded(8.),
                            )
                            .child(details(details_open.clone(), name)),
                    )
                    .child(context)
                    .child(
                        row()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .child(
                                text("Drag a tab to another window, or out of every window.")
                                    .text_size(12.)
                                    .opacity(0.6),
                            )
                            .child(select(size.clone(), chosen)),
                    ),
            ),
    );
    if let Some(which) = open_at_start {
        let signals = [
            ("file", file),
            ("recent", recent),
            ("select", size),
            ("details", details_open),
        ];
        let wanted: Vec<_> = signals
            .into_iter()
            .filter(|(name, _)| which == *name || (which == "recent" && *name == "file"))
            .map(|(_, open)| open)
            .collect();
        cx.tasks.spawn(async move {
            for open in wanted {
                zgui::timer::sleep(std::time::Duration::from_millis(600)).await;
                open.set(true);
            }
        });
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let open_at_start =
        std::env::args().find_map(|arg| arg.strip_prefix("--open=").map(String::from));
    let at = std::env::args().find_map(|arg| {
        let (x, y) = arg.strip_prefix("--at=")?.split_once(',')?;
        Some((x.parse().ok()?, y.parse().ok()?))
    });
    let mut first = options(None);
    first.bounds = at.map(|(x, y)| zgui_desktop::WindowBounds::new(1120, 800).at(x, y));
    // Where it was last left.
    if first.bounds.is_none() {
        first.frame_autosave = Some("Documents".into());
    }
    Application::new().window(first).run(move |cx| {
        let windows = cx.windows.clone();
        let tabs = ["Overview", "Notes", "Photos"].map(Tab::new).to_vec();
        document(cx, windows, tabs, open_at_start);
    })
}
