//! Sticky positioning (CSS Positioned Layout 3 §3.4).
//!
//! Taffy lays a `position: sticky` box out as it lays out a static one: the box's insets are not
//! offsets but the distances it keeps from the edges of its nearest scrollport. After each layout,
//! and whenever a scroll offset changes, every sticky box is given the offset that keeps its
//! border box that far inside the scrollport, as far as its margin box stays inside its
//! containing block. The offset is kept beside the layout, in `LayoutData::sticky_offset`, like a
//! scroll offset: paint, hit testing and geometry queries add it to the box's location, while
//! layout and scrollable overflow do not see it.

use blitz_traits::node_id::NodeId;
use style::values::computed::Overflow;
use taffy::{Point, Rect};

use crate::resolved_style::resolve_inset;
use crate::{BaseDocument, Node};

impl BaseDocument {
    /// Collect the sticky boxes after a layout and position them.
    pub(crate) fn resolve_sticky_positions(&mut self) {
        // A box that has stopped being sticky must not keep its last offset.
        for id in std::mem::take(&mut self.sticky_nodes) {
            if let Some(data) = self.nodes.get_mut(id).and_then(Node::try_layout_data_mut) {
                data.sticky_offset = Point::ZERO;
            }
        }

        // ponytail: one pass over every node per layout; follow `position` changes through
        // restyle damage instead if this shows up in profiles.
        let mut sticky: Vec<(usize, NodeId)> = self
            .nodes
            .iter()
            .filter(|(_, node)| {
                node.flags.is_in_document()
                    && node.taffy_position() == taffy::Position::Sticky
                    // A non-atomic inline has no box of its own to shift.
                    && !node.is_non_atomic_inline()
            })
            .map(|(id, _)| (self.containing_block_depth(id), id))
            .collect();
        // Ancestors first, so that a nested sticky box is placed against their shifted boxes.
        sticky.sort_by_key(|&(depth, _)| depth);
        self.sticky_nodes = sticky.into_iter().map(|(_, id)| id).collect();

        self.update_sticky_offsets();
    }

    /// Recompute the offset of every sticky box from the current layout and scroll offsets.
    pub(crate) fn update_sticky_offsets(&mut self) {
        let mut changed = false;
        let sticky_nodes = std::mem::take(&mut self.sticky_nodes);
        for &id in &sticky_nodes {
            let offset = self.sticky_offset_of(id).unwrap_or(Point::ZERO);
            if let Some(data) = self.nodes.get_mut(id).and_then(Node::try_layout_data_mut) {
                changed |= data.sticky_offset != offset;
                data.sticky_offset = offset;
            }
        }
        self.sticky_nodes = sticky_nodes;
        if changed {
            // Hoisted paint offsets memoised against the geometry generation include it.
            self.nodes.bump_geometry_generation();
        }
    }

    /// The number of boxes on the containing block chain of `id`, itself included.
    fn containing_block_depth(&self, id: NodeId) -> usize {
        std::iter::successors(Some(id), |&id| self.nodes.get(id)?.containing_block()).count()
    }

    /// The offset of the sticky box `id` from its in-flow position, or `None` when the box has
    /// no containing block.
    fn sticky_offset_of(&self, id: NodeId) -> Option<Point<f32>> {
        let node = self.nodes.get(id)?;
        let style = node.primary_styles()?;
        let layout = node.final_layout();
        let containing_block_id = node.containing_block()?;
        let containing_block = self.nodes.get(containing_block_id)?;

        // From here on, coordinates are relative to the containing block's border box, in
        // which the box's `location` is given. The box must stay in the content box.
        let cb_layout = containing_block.final_layout();
        let mut containing = Rect {
            left: cb_layout.border.left + cb_layout.padding.left,
            right: cb_layout.size.width
                - cb_layout.border.right
                - cb_layout.scrollbar_size.width
                - cb_layout.padding.right,
            top: cb_layout.border.top + cb_layout.padding.top,
            bottom: cb_layout.size.height
                - cb_layout.border.bottom
                - cb_layout.scrollbar_size.height
                - cb_layout.padding.bottom,
        };

        // Find the nearest scroll container on the containing block chain, or the viewport,
        // which the root element's overflow scrolls. `origin` is the containing block's border
        // box in the coordinates of the border box of `current`.
        let root_id = self.try_root_element().map(|root| root.id);
        let mut origin = Point::ZERO;
        let mut current = containing_block;
        let port = loop {
            if Some(current.id) == root_id || current.containing_block().is_none() {
                origin = origin + current.final_layout().location;
                break self.viewport_scrollport();
            }
            if is_scroll_container(current) {
                if current.id == containing_block_id {
                    // Content scrolls within the scrollable overflow area, which is the limit
                    // here rather than the scroll container's content box.
                    let overflow = cb_layout.scrollable_overflow_rect;
                    containing.right = containing
                        .right
                        .max(cb_layout.border.left + overflow.right - cb_layout.padding.right);
                    containing.bottom = containing
                        .bottom
                        .max(cb_layout.border.top + overflow.bottom - cb_layout.padding.bottom);
                }
                break scrollport_of(current);
            }
            origin = origin + current.final_layout().location + current.sticky_offset();
            current = self.nodes.get(current.containing_block()?)?;
        };
        let port = Rect {
            left: port.left - origin.x,
            right: port.right - origin.x,
            top: port.top - origin.y,
            bottom: port.bottom - origin.y,
        };

        // Percentage insets resolve against the scrollport's size.
        let inset = style.get_position();
        let port_width = port.right - port.left;
        let port_height = port.bottom - port.top;
        Some(Point {
            x: sticky_shift(
                layout.location.x,
                layout.size.width,
                (layout.margin.left, layout.margin.right),
                (
                    resolve_inset(&inset.left, port_width),
                    resolve_inset(&inset.right, port_width),
                ),
                (port.left, port.right),
                (containing.left, containing.right),
            ),
            y: sticky_shift(
                layout.location.y,
                layout.size.height,
                (layout.margin.top, layout.margin.bottom),
                (
                    resolve_inset(&inset.top, port_height),
                    resolve_inset(&inset.bottom, port_height),
                ),
                (port.top, port.bottom),
                (containing.top, containing.bottom),
            ),
        })
    }

    /// The viewport's scrollport in the coordinates of the root element's containing block.
    fn viewport_scrollport(&self) -> Rect<f32> {
        let scroll = self.viewport_scroll();
        let scale = self.viewport.scale_f64();
        let width = self.viewport.window_size.0 as f64 / scale;
        let height = self.viewport.window_size.1 as f64 / scale;
        Rect {
            left: scroll.x as f32,
            right: (scroll.x + width) as f32,
            top: scroll.y as f32,
            bottom: (scroll.y + height) as f32,
        }
    }
}

/// Whether the box has a scrolling box: an `overflow` other than `visible` or `clip` on either
/// axis (CSS Overflow 3 §3). `hidden` counts, as it can be scrolled programmatically.
fn is_scroll_container(node: &Node) -> bool {
    node.primary_styles().is_some_and(|style| {
        let scrolls = |overflow: Overflow| !matches!(overflow, Overflow::Visible | Overflow::Clip);
        scrolls(style.clone_overflow_x()) || scrolls(style.clone_overflow_y())
    })
}

/// The scrollport (padding box) of a scroll container at its current scroll offset, in the
/// coordinates of its border box.
fn scrollport_of(node: &Node) -> Rect<f32> {
    let layout = node.final_layout();
    let scroll = node.scroll_offset();
    let (x, y) = (scroll.x as f32, scroll.y as f32);
    Rect {
        left: layout.border.left + x,
        right: layout.size.width - layout.border.right - layout.scrollbar_size.width + x,
        top: layout.border.top + y,
        bottom: layout.size.height - layout.border.bottom - layout.scrollbar_size.height + y,
    }
}

/// The shift along one axis of a sticky box whose border box starts at `start` and is `size`
/// long, with `margin` before and after it: inwards from the edges of `port`, by the `inset` on
/// that side, but not so far that its margin box leaves `containing`. A box that is already
/// outside `containing` in flow is not moved into it. When the two insets conflict, the start
/// one wins.
fn sticky_shift(
    start: f32,
    size: f32,
    margin: (f32, f32),
    inset: (Option<f32>, Option<f32>),
    port: (f32, f32),
    containing: (f32, f32),
) -> f32 {
    let mut position = start;
    if let Some(end) = inset.1 {
        position = position.min(port.1 - end - size);
    }
    if let Some(begin) = inset.0 {
        position = position.max(port.0 + begin);
    }
    let min = (containing.0 + margin.0).min(start);
    let max = (containing.1 - margin.1 - size).max(start);
    position.max(min).min(max) - start
}

#[cfg(test)]
mod tests {
    use super::sticky_shift;

    #[test]
    fn sticky_shift_keeps_box_in_port_and_containing_block() {
        let shift = |inset, port| sticky_shift(50.0, 20.0, (0.0, 0.0), inset, port, (0.0, 200.0));
        // Not yet at the threshold: stays in flow.
        assert_eq!(shift((Some(0.0), None), (0.0, 100.0)), 0.0);
        // Past the top threshold: sticks `top` below the port's top edge.
        assert_eq!(shift((Some(10.0), None), (100.0, 200.0)), 60.0);
        // Never leaves the containing block.
        assert_eq!(shift((Some(0.0), None), (300.0, 400.0)), 130.0);
        // Below the port: `bottom` pulls it up to the port's bottom edge.
        assert_eq!(shift((None, Some(0.0)), (0.0, 40.0)), -30.0);
        // Conflicting insets: the start one wins.
        assert_eq!(shift((Some(0.0), Some(0.0)), (60.0, 70.0)), 10.0);
    }
}
