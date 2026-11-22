//! `:focus-visible` matches a focused element when the focus came from the
//! keyboard or the element takes text input, not when a pointer press focused
//! some other element; keyboard use then makes the focused element match
//! (Selectors 4, 13.3, suggested heuristics).

use blitz_dom::{Document, DocumentConfig};
use blitz_html::{HtmlDocument, HtmlProvider};
use blitz_traits::{
    events::{
        BlitzKeyEvent, BlitzPointerEvent, BlitzPointerId, KeyState, MouseEventButton,
        MouseEventButtons, Point, PointerCoords, PointerDetails, UiEvent,
    },
    shell::{ColorScheme, Viewport},
};
use keyboard_types::{Code, Key, Location, Modifiers, NamedKey};
use std::sync::Arc;

const HTML: &str = r#"<html><body style="margin:0">
    <details><summary id="summary" style="height:20px">Summary</summary></details>
    <input id="input" type="text" style="display:block;height:20px;margin:0;border:0;padding:0">
    <button id="button" style="display:block;height:20px;margin:0;border:0;padding:0"><span>Go</span></button>
</body></html>"#;

fn make_doc() -> HtmlDocument {
    let mut doc = HtmlDocument::from_html(
        HTML,
        DocumentConfig {
            viewport: Some(Viewport::new(400, 300, 1.0, ColorScheme::Light)),
            html_parser_provider: Some(Arc::new(HtmlProvider) as _),
            ..Default::default()
        },
    );
    doc.resolve(0.0);
    doc
}

fn matches(doc: &HtmlDocument, selector: &str) -> bool {
    doc.query_selector(selector).unwrap().is_some()
}

fn key_down(key: Key, code: Code) -> UiEvent {
    UiEvent::KeyDown(BlitzKeyEvent {
        key,
        code,
        modifiers: Modifiers::empty(),
        location: Location::Standard,
        is_auto_repeating: false,
        is_composing: false,
        state: KeyState::Pressed,
        text: None,
    })
}

fn click(doc: &mut HtmlDocument, x: f32, y: f32) {
    let event = BlitzPointerEvent {
        id: BlitzPointerId::Mouse,
        is_primary: true,
        coords: PointerCoords {
            page_x: x,
            page_y: y,
            screen_x: x,
            screen_y: y,
            client_x: x,
            client_y: y,
        },
        button: MouseEventButton::Main,
        buttons: MouseEventButtons::from(MouseEventButton::Main),
        mods: Default::default(),
        details: PointerDetails::default(),
        element: Point::default(),
        active_pointers: Default::default(),
    };
    doc.handle_ui_event(UiEvent::PointerDown(event.clone()));
    doc.handle_ui_event(UiEvent::PointerUp(event));
    doc.resolve(0.0);
}

#[test]
fn tab_focus_is_visible() {
    let mut doc = make_doc();
    doc.handle_ui_event(key_down(Key::Named(NamedKey::Tab), Code::Tab));
    assert!(matches(&doc, "#summary:focus"));
    assert!(matches(&doc, "#summary:focus-visible"));
}

#[test]
fn pointer_focus_of_a_text_input_is_visible() {
    let mut doc = make_doc();
    click(&mut doc, 10.0, 30.0);
    assert!(matches(&doc, "#input:focus"));
    assert!(matches(&doc, "#input:focus-visible"));
}

#[test]
fn pointer_focus_is_not_visible_until_a_key_is_pressed() {
    let mut doc = make_doc();
    click(&mut doc, 10.0, 10.0);
    assert!(matches(&doc, "#summary:focus"));
    assert!(!matches(&doc, "#summary:focus-visible"));

    doc.handle_ui_event(key_down(Key::Named(NamedKey::Shift), Code::ShiftLeft));
    assert!(matches(&doc, "#summary:focus-visible"));
}

#[test]
fn pointer_press_focuses_the_focusable_ancestor() {
    let mut doc = make_doc();
    // Inside the button, on it or on its <span>
    click(&mut doc, 5.0, 50.0);
    assert!(matches(&doc, "#button:focus"));
    assert!(!matches(&doc, "#button:focus-visible"));
}
