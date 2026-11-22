use super::ElementCx;
use crate::color::{Color, ToColorColor as _};
use anyrender::PaintScene;
use blitz_dom::local_name;
use kurbo::{Affine, BezPath, Cap, Circle, Join, Point, RoundedRect, Stroke, Vec2};
use peniko::Fill;
use style::dom::TElement as _;

impl ElementCx<'_, '_> {
    pub(super) fn draw_input(&self, scene: &mut impl PaintScene) {
        if self.node.local_name() != "input" {
            return;
        }
        let Some(checked) = self.element.checkbox_input_checked() else {
            return;
        };

        let type_attr = self.node.attr(local_name!("type"));
        let disabled = self.node.attr(local_name!("disabled")).is_some();

        // TODO this should be coming from css accent-color, but I couldn't find how to retrieve it
        let accent_color = if disabled {
            Color::from_rgba8(209, 209, 209, 255)
        } else {
            self.style.clone_color().as_srgb_color()
        };

        let width = self.frame.border_box.width();
        let height = self.frame.border_box.height();
        let min_dimension = width.min(height);
        let scale = (min_dimension - 4.0).max(0.0) / 16.0;

        let frame = self.frame.border_box.to_rounded_rect(scale * 2.0);

        match type_attr {
            Some("checkbox") => {
                draw_checkbox(scene, checked, frame, self.transform, accent_color, scale);
            }
            Some("radio") => {
                let center = frame.center();
                draw_radio_button(scene, checked, center, self.transform, accent_color, scale);
            }
            _ => {}
        }
    }

    /// A select's drop-down arrow, in its inline-end padding, and the text of its selected
    /// option in its content box (`pos`). The text is that option's own text layout, which
    /// blitz-dom's `compute_select_layout` sizes to the box.
    pub(super) fn draw_select(&self, scene: &mut impl PaintScene, pos: Point) {
        if self.node.local_name() != "select" {
            return;
        }

        let padding_box = self.frame.padding_box;
        let center = Point::new(
            padding_box.x1 - self.frame.padding_width.x1 / 2.0,
            padding_box.center().y,
        );
        let half_width = 4.0 * self.scale;
        let mut arrow = BezPath::new();
        arrow.move_to((center.x - half_width, center.y - half_width / 2.0));
        arrow.line_to((center.x + half_width, center.y - half_width / 2.0));
        arrow.line_to((center.x, center.y + half_width / 2.0));
        arrow.close_path();
        let color = self.style.clone_color().as_srgb_color();
        scene.fill(Fill::NonZero, self.transform, color, None, &arrow);

        let dom = self.context.dom;
        let Some(option) = dom
            .selected_option(self.node.id)
            .and_then(|id| dom.get_node(id))
            .filter(|option| option.flags.is_inline_root())
        else {
            return;
        };
        let Some(text_layout) = option
            .element_data()
            .and_then(|el| el.inline_layout_data.as_deref())
        else {
            return;
        };
        let layout = option.final_layout();
        let pos = Point {
            x: pos.x + f64::from(layout.padding.left + layout.border.left),
            y: pos.y + f64::from(layout.padding.top + layout.border.top + text_layout.block_offset),
        };
        let transform = self.transform * Affine::translate((pos.x * self.scale, pos.y * self.scale));
        let mut draw_text_context = self.context.draw_text_context.borrow_mut();
        crate::text::stroke_text(
            scene,
            text_layout.layout.lines(),
            dom,
            transform,
            self.scale,
            option.id,
            &mut draw_text_context,
        );
    }
}

fn draw_checkbox(
    scene: &mut impl PaintScene,
    checked: bool,
    frame: RoundedRect,
    transform: Affine,
    accent_color: Color,
    scale: f64,
) {
    if checked {
        scene.fill(Fill::NonZero, transform, accent_color, None, &frame);
        //Tick code derived from masonry
        let mut path = BezPath::new();
        path.move_to((2.0, 9.0));
        path.line_to((6.0, 13.0));
        path.line_to((14.0, 2.0));

        path.apply_affine(Affine::translate(Vec2 { x: 2.0, y: 1.0 }).then_scale(scale));

        let style = Stroke {
            width: 2.0 * scale,
            join: Join::Round,
            miter_limit: 10.0,
            start_cap: Cap::Round,
            end_cap: Cap::Round,
            dash_pattern: Default::default(),
            dash_offset: 0.0,
        };

        scene.stroke(&style, transform, Color::WHITE, None, &path);
    } else {
        scene.fill(Fill::NonZero, transform, Color::WHITE, None, &frame);
        scene.stroke(&Stroke::default(), transform, accent_color, None, &frame);
    }
}

fn draw_radio_button(
    scene: &mut impl PaintScene,
    checked: bool,
    center: Point,
    transform: Affine,
    accent_color: Color,
    scale: f64,
) {
    let outer_ring = Circle::new(center, 8.0 * scale);
    let gap = Circle::new(center, 6.0 * scale);
    let inner_circle = Circle::new(center, 4.0 * scale);
    if checked {
        scene.fill(Fill::NonZero, transform, accent_color, None, &outer_ring);
        scene.fill(Fill::NonZero, transform, Color::WHITE, None, &gap);
        scene.fill(Fill::NonZero, transform, accent_color, None, &inner_circle);
    } else {
        const GRAY: Color = color::palette::css::GRAY;
        scene.fill(Fill::NonZero, transform, GRAY, None, &outer_ring);
        scene.fill(Fill::NonZero, transform, Color::WHITE, None, &gap);
    }
}
