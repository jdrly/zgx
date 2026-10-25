use zgx::{compose::prelude::*, widgets::Ui};

#[test]
fn the_caret_takes_the_set_width_in_editors_already_shown() {
    let mut ui = Ui::new(400., 200.);
    let value = ui.signal(String::from("wide"));
    let mounted = ui.mount(column().child(text_input("Name", value).id("name").size(200., 30.)));
    ui.prepare_frame();
    let node = mounted.find("name").unwrap();
    ui.input.focus(&ui.scene, Some(node));
    ui.prepare_frame();
    let editor = ui.focused_editor().unwrap();
    assert_eq!(ui.scene.borrow().bounds(editor.caret).width, 1.);
    // A thicker text cursor, as Windows' accessibility settings set it.
    ui.set_caret_width(3.);
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().bounds(editor.caret).width, 3.);
    ui.set_caret_width(f32::NAN);
    assert_eq!(ui.caret_width(), 3.);
    ui.set_caret_width(50.);
    assert_eq!(ui.caret_width(), 20.);
    ui.set_caret_width(0.);
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().bounds(editor.caret).width, 1.);
}
