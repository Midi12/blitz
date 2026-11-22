use blitz_dom::node::SpecialElementData;
use blitz_test_harness::Harness;

/// A 2x2 table of 50x20 cells with a 5px border, and a 10px caption with 3px vertical margins.
fn table(caption_css: &str) -> Harness {
    Harness::from_html(&format!(
        r#"<!DOCTYPE html><style>
            body {{ margin: 0; }}
            table {{ border-spacing: 0; border: 5px solid black; }}
            td {{ padding: 0; width: 50px; height: 20px; }}
            caption {{ height: 10px; margin: 3px 0; {caption_css} }}
        </style><table id="table"><caption id="caption"></caption>
            <tr><td id="cell"></td><td></td></tr><tr><td></td><td></td></tr></table>"#
    ))
}

/// The start of the first row track the table recorded for painting, relative to its box.
fn first_row_track(harness: &Harness) -> f32 {
    let table = harness.node("#table");
    let doc = harness.base();
    let element = doc.get_node(table).unwrap().element_data().unwrap();
    let SpecialElementData::TableRoot(context) = &element.special_data else {
        panic!("not a table root");
    };
    let Some(grid) = &*context.computed_grid_info.borrow() else {
        panic!("the table has not been laid out");
    };
    grid.rows.positions[0].start
}

#[test]
fn caption_sits_above_the_table_box() {
    let harness = table("");
    let table = harness.layout_rect("#table");
    let caption = harness.layout_rect("#caption");
    let cell = harness.layout_rect("#cell");
    // The caption is as wide as the table box, which is 2 * 50 + 2 * 5 wide
    assert_eq!((caption.x, caption.width), (table.x, 110.0));
    assert_eq!(caption.y - table.y, 3.0);
    // The table box starts below the caption's margin box, its border above the first row
    assert_eq!(cell.y - table.y, 16.0 + 5.0);
    assert_eq!(table.height, 16.0 + 50.0);
    assert_eq!(first_row_track(&harness), cell.y - table.y);
}

#[test]
fn caption_side_bottom_puts_the_caption_below_the_table_box() {
    let harness = table("caption-side: bottom");
    let table = harness.layout_rect("#table");
    let caption = harness.layout_rect("#caption");
    let cell = harness.layout_rect("#cell");
    assert_eq!(cell.y - table.y, 5.0);
    assert_eq!(caption.y - table.y, 50.0 + 3.0);
    assert_eq!(table.height, 50.0 + 16.0);
    assert_eq!(first_row_track(&harness), 5.0);
}
