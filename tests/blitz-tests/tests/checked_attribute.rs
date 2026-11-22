//! The checked state of a checkbox or radio input follows its `checked`
//! attribute, both when the attribute is set and when it is removed.

use blitz_dom::{DocumentConfig, NodeId, QualName, local_name, ns};
use blitz_html::{HtmlDocument, HtmlProvider};
use blitz_traits::shell::{ColorScheme, Viewport};
use std::sync::Arc;

fn make_doc(html: &str) -> HtmlDocument {
    let mut doc = HtmlDocument::from_html(
        html,
        DocumentConfig {
            viewport: Some(Viewport::new(400, 300, 1.0, ColorScheme::Light)),
            html_parser_provider: Some(Arc::new(HtmlProvider) as _),
            ..Default::default()
        },
    );
    doc.resolve(0.0);
    doc
}

fn node_id(doc: &HtmlDocument, selector: &str) -> NodeId {
    doc.query_selector(selector).unwrap().expect(selector)
}

fn checked() -> QualName {
    QualName::new(None, ns!(), local_name!("checked"))
}

fn is_checked(doc: &HtmlDocument, node: NodeId) -> (Option<bool>, bool) {
    let element = doc.get_node(node).unwrap().element_data().unwrap();
    let matches_checked = doc.query_selector("#t:checked").unwrap().is_some();
    (element.checkbox_input_checked(), matches_checked)
}

fn removing_checked_unchecks(input_type: &str) {
    let mut doc = make_doc(&format!(
        "<html><body><input id=t type={input_type} checked></body></html>"
    ));
    let node = node_id(&doc, "#t");
    assert_eq!(is_checked(&doc, node), (Some(true), true));

    doc.mutate().clear_attribute(node, checked());
    assert_eq!(is_checked(&doc, node), (Some(false), false));

    doc.mutate().set_attribute(node, checked(), "true");
    assert_eq!(is_checked(&doc, node), (Some(true), true));

    doc.mutate().clear_attribute(node, checked());
    assert_eq!(is_checked(&doc, node), (Some(false), false));
}

#[test]
fn removing_checked_unchecks_a_checkbox() {
    removing_checked_unchecks("checkbox");
}

#[test]
fn removing_checked_unchecks_a_radio() {
    removing_checked_unchecks("radio");
}

#[test]
fn checked_is_a_boolean_attribute() {
    let mut doc = make_doc("<html><body><input id=t type=checkbox></body></html>");
    let node = node_id(&doc, "#t");
    assert_eq!(is_checked(&doc, node), (Some(false), false));

    for value in ["", "false", "checked"] {
        doc.mutate().set_attribute(node, checked(), value);
        assert_eq!(is_checked(&doc, node), (Some(true), true), "checked={value:?}");
        let attrs = doc.get_node(node).unwrap().element_data().unwrap().attrs();
        assert_eq!(
            attrs.iter().filter(|a| a.name.local == local_name!("checked")).count(),
            1
        );
    }
}

#[test]
fn removing_absent_checked_unchecks_a_changed_checkbox() {
    let mut doc = make_doc("<html><body><input id=t type=checkbox></body></html>");
    let node = node_id(&doc, "#t");
    // The user checks the box: the state changes, the attribute does not.
    doc.get_node_mut(node)
        .unwrap()
        .element_data_mut()
        .unwrap()
        .set_checkbox_input_checked(true);
    assert_eq!(is_checked(&doc, node), (Some(true), true));

    doc.mutate().clear_attribute(node, checked());
    assert_eq!(is_checked(&doc, node), (Some(false), false));
}

#[test]
fn removing_absent_value_empties_a_changed_textarea() {
    let mut doc = make_doc("<html><body><textarea id=t></textarea></body></html>");
    let node = node_id(&doc, "#t");
    let text = |doc: &HtmlDocument| {
        let element = doc.get_node(node).unwrap().element_data().unwrap();
        element.text_input_data().unwrap().editor.raw_text().to_string()
    };
    doc.mutate()
        .set_attribute(node, QualName::new(None, ns!(), local_name!("value")), "typed");
    doc.get_node_mut(node)
        .unwrap()
        .element_data_mut()
        .unwrap()
        .attrs
        .remove(&QualName::new(None, ns!(), local_name!("value")));
    assert_eq!(text(&doc), "typed");

    doc.mutate()
        .clear_attribute(node, QualName::new(None, ns!(), local_name!("value")));
    assert_eq!(text(&doc), "");
}
