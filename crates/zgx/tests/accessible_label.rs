//! `View::accessible_label`: a name given for assistive technology stays the
//! node's name whatever text it shows.
use zgx::{compose::prelude::*, semantics::Role, widgets::Ui};

#[test]
fn a_given_name_replaces_the_one_made_from_text() {
    let mut ui = Ui::new(400., 200.);
    let caption = ui.signal("on".to_string());
    let view = ui.mount(
        column()
            .child(
                button()
                    .id("switch")
                    .accessible_label("Turn off")
                    .child(text_signal({
                        let caption = caption.clone();
                        move || caption.get()
                    })),
            )
            .child(button().id("save").child(text("Save")))
            .child(
                div()
                    .id("chart")
                    .accessible_label("CPU history")
                    .size(40., 20.),
            ),
    );
    ui.prepare_frame();
    let label = |ui: &Ui, id: &str| {
        let node = view.find(id).unwrap();
        let semantics = ui.semantics.borrow();
        let node = semantics.get(node).unwrap();
        (node.role, node.label.clone())
    };
    assert_eq!(label(&ui, "switch"), (Role::Button, "Turn off".into()));
    assert_eq!(
        label(&ui, "save"),
        (Role::Button, "Save".into()),
        "text still names a button"
    );
    assert_eq!(label(&ui, "chart"), (Role::Group, "CPU history".into()));
    caption.set("off".into());
    ui.prepare_frame();
    assert_eq!(
        label(&ui, "switch").1,
        "Turn off",
        "the text changing keeps the given name"
    );
}

/// A name and value made from state are worked out only while assistive
/// technology listens: with none, the state changing calls neither; once
/// one listens, both are what the state is now, before the tree is read.
#[test]
fn names_from_state_wait_for_assistive_technology() {
    use std::{cell::Cell, rc::Rc};
    let mut ui = Ui::new(400., 200.);
    ui.set_assistive_technology(false);
    let count = ui.signal(1_u32);
    let calls = Rc::new(Cell::new(0));
    let view = ui.mount(
        column().child(
            div()
                .id("row")
                .size(100., 20.)
                .accessible_label_when({
                    let (count, calls) = (count.clone(), calls.clone());
                    move || {
                        calls.set(calls.get() + 1);
                        format!("{} unread", count.get())
                    }
                })
                .value_when({
                    let count = count.clone();
                    move || count.get().to_string()
                }),
        ),
    );
    ui.prepare_frame();
    let read = |ui: &Ui| {
        let node = view.find("row").unwrap();
        let semantics = ui.semantics.borrow();
        let node = semantics.get(node).unwrap();
        (node.label.clone(), node.value.clone())
    };
    for n in 2..10 {
        count.set(n);
        ui.prepare_frame();
    }
    assert_eq!(calls.get(), 0, "not worked out with no reader");
    // A reader asks for the tree: names and values are the state's now.
    ui.set_assistive_technology(true);
    assert_eq!(read(&ui), ("9 unread".into(), Some("9".into())));
    assert_eq!(calls.get(), 1);
    count.set(10);
    ui.prepare_frame();
    assert_eq!(
        read(&ui),
        ("10 unread".into(), Some("10".into())),
        "followed"
    );
    // It leaves: nothing more is worked out.
    ui.set_assistive_technology(false);
    let before = calls.get();
    count.set(11);
    ui.prepare_frame();
    assert_eq!(calls.get(), before);
}
