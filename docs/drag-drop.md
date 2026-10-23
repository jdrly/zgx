# Typed retained drag and drop

A drag source produces a Rust value after a primary-button gesture travels four logical pixels. A normal click still activates the source. A target accepts only its payload type and can apply an additional predicate:

```rust
#[derive(Clone)]
struct Card { name: String }
let source = div().size(100., 80.)
    .child(text("Drag me"))
    .on_drag(|| Card { name: "Card".into() })
    .drag_preview(|card: &Card| div().p(12.).child(text(&card.name)))
    .on_drag_end(|_: &Card, accepted| println!("accepted: {accepted}"));
let destination = div().size(240., 120.)
    .on_drop_when(|card: &Card| !card.name.is_empty(), |card: &Card, _| {
        println!("Dropped {}", card.name);
    });
```

The preview is an ordinary component tree, retained for the gesture and moved by compositor translation. Its top-left follows the pointer minus the grab point (`DragEvent::grab_x`/`grab_y`, the press position within the source), so a same-sized preview stays under the spot that was pressed. `drag_preview_at_cursor(offset, build)` instead places a badge-style preview at a fixed offset from the pointer. Custom gestures calling `EventContext::start_drag_from(payload, x, y)` supply the press position; `start_drag` uses the current pointer. It is excluded from drag hit testing and disposed on completion, cancellation or source removal. Sources and destinations obey visibility, disabled state and focus scopes. The nearest accepted destination wins; acceptance is checked again at release. Escape, native cancellation, hiding the source, host interaction cancellation and removal release the session. A completed drag suppresses source click activation. Raw `InputEvent::Drag` supplies Start/Move/Over/Leave/Drop/End phases for application hover styling and custom gestures; `EventContext::accept_drag` explicitly accepts an Over event.

No gesture timer is allocated and idle windows do not wake for drag handling.

On macOS a drag goes where the pointer does. Over another of the app's windows, the source's destinations hear nothing (`InputDispatcher::set_drag_away`) and that window's hear it as a visit (`drag_over`; `DragEvent::visiting`); released there, it drops there and the source's `on_drag_end` hears whether it was taken. Released over none of the app's windows, `on_drag_outside(|value, event| ...)` hears it: a tab torn off its window opens in one of its own, `WindowOptions::under_pointer` putting the grab point under the pointer. The preview shows in a window of its own, over every window and app, drawn once and then moved by the window server (no frame is drawn as the pointer moves); a drag nothing takes slides its preview back to the source, as AppKit's drag images do.

A source with `.drag_export(|value| vec![DragItem::Text(..), DragItem::Url(..), DragItem::File(..), DragItem::Image(..)])` carries those to other apps: as the drag leaves every window of the app, the host hands it to AppKit (an `NSDraggingSession`, the preview's last frame its image) and Finder, Mail or a browser take it; back over the app's windows its destinations still hear it. Elsewhere payloads stay inside their window. Windows needs `DoDragDrop` (its modal loop must keep the host painting), X11 an XDND source, Wayland `wl_data_device.start_drag`, which winit does not expose.

`cargo run -p zgui-desktop --example multiwindow` shows tabs dragged between windows and torn off, and text, a link and a file dragged out. `cargo run --release -p zgui-desktop --example drag_probe` measures it with synthetic AppKit events posted to its own windows: per pointer move during a drag, about 0.5 M instructions on the main thread, of which zgui's share is about 0.12 M (the same drag with the preview inside the window, redrawn each move: 0.75 M).

`on_files_drop` receives native paths grouped at the event-loop dispatch boundary. Winit 0.30.13 emits the X11 XdndDrop path list synchronously and queues the macOS pasteboard filename list in one callback, so grouping introduces no timeout. The grouping boundary is a dispatch batch, not a transaction identifier supplied by winit. Legacy per-path `FileDrop` events remain separately available through `on_event`. Group assembly rejects a whole batch above 1,024 paths or 1 MiB of encoded path bytes and emits `FilesDropRejected`; it never delivers a truncated grouped payload. Native file events from winit carry no pointer position; their targeting uses the host's last logical pointer location. Wayland transport and native external-file validation have separate platform evidence.

`cargo test -p zgui --test drag_drop` checks typed acceptance, rejection, nested destinations, source/target removal, hidden-source cancellation, preview disposal, click suppression and grouped-path routing. `cargo run -p zgui-desktop --example drag_drop` runs the three-card gallery. The X11 fixture `scripts/drag_drop_smoke.py` exercises native pointer delivery, accepted drop, visible preview and Escape cancellation; screenshots and logs are in `docs/platform-validation/drag-drop`. `scripts/file_drop_smoke.py` uses a real GTK Xdnd source to drop two files, including a filename with a space, and then cancels a second drag with Escape; evidence is in `docs/platform-validation/file-drop`.
