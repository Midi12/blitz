//! The `select` element and its options (HTML §4.10.7, §4.10.10).
//!
//! An option's selectedness is its `ElementState::CHECKED` bit, which is also what `:checked`
//! matches (Selectors 4 §13.3.1). A select shows its drop-down list while it has
//! `ElementState::OPEN`, which is what `:open` matches. The list is drawn inside the document,
//! below the select's box (`layout::compute_select_layout`); the selected option is painted in
//! the box itself.

use blitz_traits::events::{BlitzInputEvent, DomEvent, DomEventData};
use blitz_traits::node_id::NodeId;
use keyboard_types::{Key, NamedKey};
use markup5ever::local_name;
use style_dom::ElementState;

use crate::layout::damage::ALL_DAMAGE;
use crate::{BaseDocument, Node};

impl BaseDocument {
    /// The select element's list of options: its `option` children and the `option` children of
    /// its `optgroup` children, in tree order (HTML §4.10.7).
    pub fn select_options(&self, select_id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        self.nodes[select_id]
            .children
            .iter()
            .flat_map(move |&child_id| {
                let child = &self.nodes[child_id];
                let grandchildren: &[NodeId] =
                    if child.data.is_element_with_tag_name(&local_name!("optgroup")) {
                        &child.children
                    } else {
                        &[]
                    };
                std::iter::once(child_id).chain(grandchildren.iter().copied())
            })
            .filter(move |&id| {
                self.nodes[id]
                    .data
                    .is_element_with_tag_name(&local_name!("option"))
            })
    }

    /// The select element whose list of options holds `option_id`, if any.
    pub fn option_select(&self, option_id: NodeId) -> Option<NodeId> {
        let parent_id = self.nodes[option_id].parent?;
        let parent = &self.nodes[parent_id];
        let select_id = if parent.data.is_element_with_tag_name(&local_name!("optgroup")) {
            parent.parent?
        } else {
            parent_id
        };
        self.nodes[select_id]
            .data
            .is_element_with_tag_name(&local_name!("select"))
            .then_some(select_id)
    }

    /// Whether an option's selectedness is true.
    pub fn option_is_selected(&self, option_id: NodeId) -> bool {
        self.nodes[option_id]
            .element_data()
            .is_some_and(|el| el.element_state.contains(ElementState::CHECKED))
    }

    /// The first option of the select's list whose selectedness is true (the option of
    /// `selectedIndex`).
    pub fn selected_option(&self, select_id: NodeId) -> Option<NodeId> {
        self.select_options(select_id)
            .find(|&id| self.option_is_selected(id))
    }

    /// An option's value: its `value` attribute, else its text with ASCII whitespace stripped and
    /// collapsed (HTML §4.10.10).
    pub fn option_value(&self, option_id: NodeId) -> String {
        let node = &self.nodes[option_id];
        if let Some(value) = node.attr(local_name!("value")) {
            return value.to_string();
        }
        let text = node.text_content();
        let mut value = String::with_capacity(text.len());
        for word in text.split_ascii_whitespace() {
            if !value.is_empty() {
                value.push(' ');
            }
            value.push_str(word);
        }
        value
    }

    /// The select's value: the value of its selected option, or the empty string (HTML §4.10.7).
    pub fn select_value(&self, select_id: NodeId) -> String {
        self.selected_option(select_id)
            .map(|id| self.option_value(id))
            .unwrap_or_default()
    }

    /// Whether the select's drop-down list is shown.
    pub fn select_is_open(&self, select_id: NodeId) -> bool {
        self.nodes.get(select_id).is_some_and(|node| {
            node.data.is_element_with_tag_name(&local_name!("select"))
                && node
                    .element_data()
                    .is_some_and(|el| el.element_state.contains(ElementState::OPEN))
        })
    }

    /// An option is disabled when it has a `disabled` attribute or is the child of an `optgroup`
    /// that has one (HTML §4.10.10).
    fn option_is_disabled(&self, option_id: NodeId) -> bool {
        let node = &self.nodes[option_id];
        node.data.has_attr(local_name!("disabled"))
            || node.parent.is_some_and(|parent_id| {
                let parent = &self.nodes[parent_id];
                parent
                    .data
                    .is_element_with_tag_name(&local_name!("optgroup"))
                    && parent.data.has_attr(local_name!("disabled"))
            })
    }

    fn select_is_multiple(&self, select_id: NodeId) -> bool {
        self.nodes[select_id].data.has_attr(local_name!("multiple"))
    }

    /// Sets an option's selectedness, invalidating the styles that depend on `:checked`.
    fn set_option_selectedness(&mut self, option_id: NodeId, selected: bool) {
        if self.option_is_selected(option_id) == selected {
            return;
        }
        let set = move |node: &mut Node| {
            if let Some(el) = node.element_data_mut() {
                el.element_state.set(ElementState::CHECKED, selected);
            }
            node.mark_ancestors_dirty();
        };
        if self.nodes[option_id].flags.is_in_document() {
            self.snapshot_node_and(option_id, ElementState::CHECKED, set);
        } else {
            set(&mut self.nodes[option_id]);
        }
    }

    /// Makes `option_id` the only selected option of the select's list ("If the multiple
    /// attribute is absent, whenever an option element in the select element's list of options
    /// has its selectedness set to true, the user agent must set the selectedness of all the
    /// other option elements in its list of options to false", HTML §4.10.7).
    fn select_only(&mut self, select_id: NodeId, option_id: NodeId) {
        self.set_option_selectedness(option_id, true);
        loop {
            let other = self
                .select_options(select_id)
                .find(|&id| id != option_id && self.option_is_selected(id));
            let Some(other) = other else {
                break;
            };
            self.set_option_selectedness(other, false);
        }
    }

    /// The selectedness setting algorithm (HTML §4.10.7), run when the list of options of a
    /// select element without `multiple` gains or loses options: with no option selected, the
    /// first option that is not disabled becomes selected; with more than one, only the last of
    /// them stays selected.
    pub(crate) fn reset_select(&mut self, select_id: NodeId) {
        let Some(node) = self.nodes.get(select_id) else {
            return;
        };
        if !node.data.is_element_with_tag_name(&local_name!("select"))
            || self.select_is_multiple(select_id)
        {
            return;
        }
        let last_selected = self
            .select_options(select_id)
            .filter(|&id| self.option_is_selected(id))
            .last();
        match last_selected {
            Some(option_id) => self.select_only(select_id, option_id),
            None => {
                let first_enabled = self
                    .select_options(select_id)
                    .find(|&id| !self.option_is_disabled(id));
                if let Some(option_id) = first_enabled {
                    self.set_option_selectedness(option_id, true);
                }
            }
        }
    }

    /// The `selected` attribute was added to (`present`) or removed from an option: its
    /// selectedness follows, as for an option whose dirtiness is false, then its select is kept
    /// to one selected option (HTML §4.10.10, §4.10.7).
    pub(crate) fn option_selected_attribute_changed(&mut self, option_id: NodeId, present: bool) {
        self.set_option_selectedness(option_id, present);
        let Some(select_id) = self.option_select(option_id) else {
            return;
        };
        if self.select_is_multiple(select_id) {
            return;
        }
        if present {
            self.select_only(select_id, option_id);
        } else {
            self.reset_select(select_id);
        }
    }

    /// Sets the select's value as the `value` IDL setter does (HTML §4.10.7): the first option
    /// whose value is `value` becomes selected and every other option unselected. No option is
    /// selected when none has that value.
    pub(crate) fn set_select_value(&mut self, select_id: NodeId, value: &str) {
        let target = self
            .select_options(select_id)
            .find(|&id| self.option_value(id) == value);
        loop {
            let other = self
                .select_options(select_id)
                .find(|&id| Some(id) != target && self.option_is_selected(id));
            let Some(other) = other else {
                break;
            };
            self.set_option_selectedness(other, false);
        }
        if let Some(option_id) = target {
            self.set_option_selectedness(option_id, true);
        }
    }

    /// Shows (`open`) or hides the select's drop-down list. The options move between the box and
    /// the list, so the select is laid out again.
    pub(crate) fn set_select_open(&mut self, select_id: NodeId, open: bool) {
        if !self.nodes.contains_key(select_id)
            || !self.nodes[select_id]
                .data
                .is_element_with_tag_name(&local_name!("select"))
            || self.select_is_open(select_id) == open
        {
            return;
        }
        self.snapshot_node_and(select_id, ElementState::OPEN, |node| {
            if let Some(el) = node.element_data_mut() {
                el.element_state.set(ElementState::OPEN, open);
            }
            node.insert_damage(ALL_DAMAGE);
            node.mark_ancestors_dirty();
        });
        self.shell_provider.request_redraw();
    }

    /// The user picked `option_id` in the select (from its list or with the keyboard): it
    /// becomes the selected option and, when that changes the selection, `input` is fired at the
    /// select (HTML §4.10.7, "send select update notifications"; Blitz has no `change` event
    /// type, so an embedder derives `change` from this `input`, as for checkboxes).
    pub(crate) fn pick_option(
        &mut self,
        select_id: NodeId,
        option_id: NodeId,
        dispatch_event: &mut dyn FnMut(DomEvent),
    ) {
        if self.option_is_disabled(option_id) {
            return;
        }
        let changed = self.selected_option(select_id) != Some(option_id);
        self.select_only(select_id, option_id);
        if changed {
            let value = self.option_value(option_id);
            dispatch_event(DomEvent::new(
                select_id,
                DomEventData::Input(BlitzInputEvent { value }),
            ));
        }
        self.shell_provider.request_redraw();
    }

    /// The default action of a key pressed on a focused select, as desktop browsers on Windows
    /// and Linux do: ArrowDown and ArrowUp pick the next and previous enabled option, Home and
    /// End the first and last, whether the list is open or not; Space opens and closes the list,
    /// Enter and Escape close it.
    pub(crate) fn handle_select_key(
        &mut self,
        select_id: NodeId,
        key: &Key,
        dispatch_event: &mut dyn FnMut(DomEvent),
    ) {
        let target = match key {
            Key::Named(NamedKey::ArrowDown) => {
                let current = self.selected_option(select_id);
                let mut past_current = current.is_none();
                self.select_options(select_id).find(|&id| {
                    if past_current {
                        return !self.option_is_disabled(id);
                    }
                    past_current = Some(id) == current;
                    false
                })
            }
            Key::Named(NamedKey::ArrowUp) => {
                let current = self.selected_option(select_id);
                self.select_options(select_id)
                    .take_while(|&id| Some(id) != current)
                    .filter(|&id| !self.option_is_disabled(id))
                    .last()
            }
            Key::Named(NamedKey::Home) => self
                .select_options(select_id)
                .find(|&id| !self.option_is_disabled(id)),
            Key::Named(NamedKey::End) => self
                .select_options(select_id)
                .filter(|&id| !self.option_is_disabled(id))
                .last(),
            Key::Named(NamedKey::Enter | NamedKey::Escape) => {
                self.set_select_open(select_id, false);
                None
            }
            Key::Character(c) if c.as_str() == " " => {
                let open = self.select_is_open(select_id);
                self.set_select_open(select_id, !open);
                None
            }
            _ => None,
        };
        if let Some(option_id) = target {
            self.pick_option(select_id, option_id, dispatch_event);
        }
    }
}

#[cfg(test)]
mod test {
    use blitz_traits::node_id::NodeId;
    use keyboard_types::{Key, NamedKey};

    use crate::{Attribute, BaseDocument, DocumentConfig, qual_name};

    /// A select holding three options with the values "a", "b" and "c".
    fn select(doc: &mut BaseDocument) -> (NodeId, [NodeId; 3]) {
        let mut mutator = doc.mutate();
        let select = mutator.create_element(qual_name!("select", html), vec![]);
        let options = ["a", "b", "c"].map(|value| {
            let attr = Attribute {
                name: qual_name!("value"),
                value: value.to_string(),
            };
            mutator.create_element(qual_name!("option", html), vec![attr])
        });
        mutator.append_children(select, &options);
        (select, options)
    }

    #[test]
    fn selectedness() {
        let mut doc = BaseDocument::new(DocumentConfig::default());
        let (select, [a, b, c]) = select(&mut doc);
        // With no option selected, the first one is
        assert_eq!(doc.selected_option(select), Some(a));

        let mut mutator = doc.mutate();
        mutator.set_attribute(c, qual_name!("selected"), "");
        drop(mutator);
        assert_eq!(doc.selected_option(select), Some(c));
        assert!(!doc.option_is_selected(a));

        let mut mutator = doc.mutate();
        mutator.clear_attribute(c, qual_name!("selected"));
        drop(mutator);
        assert_eq!(doc.selected_option(select), Some(a));

        let mut mutator = doc.mutate();
        mutator.set_attribute(select, qual_name!("value"), "b");
        drop(mutator);
        assert_eq!(doc.selected_option(select), Some(b));
        assert_eq!(doc.select_value(select), "b");

        let mut events = Vec::new();
        doc.handle_select_key(select, &Key::Named(NamedKey::ArrowDown), &mut |event| {
            events.push(event)
        });
        assert_eq!(doc.selected_option(select), Some(c));
        assert_eq!(events.len(), 1);
        doc.handle_select_key(select, &Key::Named(NamedKey::ArrowDown), &mut |event| {
            events.push(event)
        });
        assert_eq!(events.len(), 1, "no input event without a change");
    }
}
