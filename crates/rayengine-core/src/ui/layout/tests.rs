use super::*;
use crate::ui::{UiButton, UiId, UiInput, UiRegion, UiState};

fn bounds(min: Vec2, size: Vec2) -> Aabb2 {
    Aabb2 {
        min,
        max: min + size,
    }
}
fn tick(pointer: Vec2) -> UiInput {
    UiInput {
        pointer: Some(pointer),
        window_focused: true,
        ..UiInput::default()
    }
}

#[test]
fn layouts_size_empty_partial_rows_and_lists() {
    let grid = UiLayout::grid(3, Vec2::new(20.0, 10.0), Vec2::splat(2.0));
    assert_eq!(grid.content_size(0), Vec2::ZERO);
    assert_eq!(grid.content_size(2), Vec2::new(42.0, 10.0));
    assert_eq!(grid.content_size(4), Vec2::new(64.0, 22.0));
    assert_eq!(grid.item(3).min, Vec2::new(0.0, 12.0));
    let list = UiLayout::list(Vec2::new(80.0, 20.0), 4.0);
    assert_eq!(list.item(2).max, Vec2::new(80.0, 68.0));
    assert_eq!(list.content_size(3), Vec2::new(80.0, 68.0));
}

#[test]
fn scroll_clamps_resizes_translates_and_reveals_focus() {
    let mut scroll = UiScrollState::default();
    scroll.configure(Vec2::splat(100.0), Vec2::new(200.0, 500.0));
    scroll.scroll_by(Vec2::new(-20.0, 999.0));
    assert_eq!(scroll.offset(), Vec2::new(0.0, 400.0));
    scroll.reveal(bounds(Vec2::new(20.0, 180.0), Vec2::splat(20.0)));
    assert_eq!(scroll.offset(), Vec2::new(0.0, 180.0));
    scroll.reveal(bounds(Vec2::new(150.0, 300.0), Vec2::splat(20.0)));
    assert_eq!(scroll.offset(), Vec2::new(70.0, 220.0));
    let item = scroll.item_bounds(
        bounds(Vec2::splat(10.0), Vec2::splat(100.0)),
        bounds(Vec2::new(70.0, 220.0), Vec2::ONE),
    );
    assert_eq!(item.min, Vec2::splat(10.0));
    scroll.configure(Vec2::splat(300.0), Vec2::splat(150.0));
    assert_eq!(scroll.offset(), Vec2::ZERO);
    scroll.scroll_by(Vec2::splat(f32::NAN));
    assert_eq!(scroll.offset(), Vec2::ZERO);
}

#[test]
fn thumb_drag_uses_matching_geometry_and_clamps() {
    let mut scroll = UiScrollState::default();
    let track = bounds(Vec2::new(200.0, 20.0), Vec2::new(12.0, 100.0));
    scroll.configure(Vec2::splat(100.0), Vec2::new(100.0, 400.0));
    assert_eq!(scroll.vertical_thumb(track, 10.0).unwrap().max.y, 45.0);
    assert!(scroll.drag_vertical_thumb(track, 10.0, 37.5));
    assert_eq!(scroll.offset().y, 150.0);
    assert_eq!(scroll.vertical_thumb(track, 10.0).unwrap().min.y, 57.5);
    scroll.drag_vertical_thumb(track, 10.0, 100.0);
    assert_eq!(scroll.offset().y, 300.0);
    assert!(!scroll.drag_vertical_thumb(track, 100.0, 10.0));
}

#[test]
fn nested_empty_clips_and_pixel_centers_agree_at_edges() {
    let outer = UiClip::new(bounds(Vec2::new(10.25, 20.25), Vec2::splat(30.0)));
    let inner = UiClip::new(bounds(Vec2::new(0.0, 25.0), Vec2::splat(30.0)));
    let clip = outer.intersect(inner);
    assert!(clip.contains(Vec2::new(10.25, 25.0)));
    assert!(!clip.contains(Vec2::new(30.0, 25.0)));
    assert!(
        !clip
            .intersect(UiClip::new(bounds(Vec2::splat(99.0), Vec2::ONE)))
            .contains(Vec2::ZERO)
    );
    for scale in [Vec2::ONE, Vec2::splat(1.5), Vec2::new(2.0, 3.0)] {
        let (x, y, w, h) = clip.scissor(scale);
        for px in 0..130 {
            for py in 0..160 {
                let center = Vec2::new(px as f32 + 0.5, py as f32 + 0.5) / scale;
                assert_eq!(
                    clip.contains(center),
                    px >= x && px < x + w && py >= y && py < y + h
                );
            }
        }
    }
}

#[test]
fn clipped_items_keep_focus_and_only_topmost_region_routes_wheel() {
    let viewport = bounds(Vec2::splat(10.0), Vec2::splat(100.0));
    let clip = UiClip::new(viewport);
    let mut surface = UiRegion::new(UiId(1), viewport);
    surface.focusable = false;
    surface.scroll_target = Some(surface.id);
    let mut item = UiRegion::new(UiId(2), bounds(Vec2::new(10.0, 80.0), Vec2::splat(100.0)));
    item.clip = Some(clip);
    item.scroll_target = Some(surface.id);
    let mut ui = UiState::default();
    let mut input = tick(Vec2::new(20.0, 90.0));
    input.next = true;
    input.scroll = Vec2::new(0.0, -2.0);
    assert!(ui.update(&[surface, item], input).pointer);
    assert_eq!(ui.focused(), Some(item.id));
    assert_eq!(ui.response(surface.id).unwrap().scroll, input.scroll);
    let mut moved = item;
    moved.bounds.min.y -= 300.0;
    moved.bounds.max.y -= 300.0;
    ui.update(&[surface, moved], tick(Vec2::new(20.0, 90.0)));
    assert_eq!(ui.focused(), Some(item.id));
    assert!(!ui.response(item.id).unwrap().hovered);
    // The clipped part cannot receive a press, even though original bounds contain it.
    input = tick(Vec2::new(20.0, 110.0));
    input.primary = UiButton {
        pressed: true,
        released: true,
        ..UiButton::default()
    };
    assert!(!ui.update(&[surface, item], input).pointer);
    assert!(!ui.response(item.id).unwrap().activated);
    // Overlaid disabled surfaces block both pointer and wheel propagation.
    let mut overlay = UiRegion::new(UiId(3), viewport);
    overlay.enabled = false;
    input = tick(Vec2::new(20.0, 90.0));
    input.scroll = Vec2::ONE;
    ui.update(&[surface, item, overlay], input);
    assert!(ui.response(overlay.id).unwrap().hovered);
    assert_eq!(ui.response(surface.id).unwrap().scroll, Vec2::ZERO);
}

#[test]
fn release_in_clipped_portion_does_not_activate_and_drag_keeps_capture() {
    let mut region = UiRegion::new(UiId(1), bounds(Vec2::ZERO, Vec2::splat(100.0)));
    region.clip = Some(UiClip::new(bounds(Vec2::ZERO, Vec2::splat(50.0))));
    for draggable in [false, true] {
        region.draggable = draggable;
        let mut ui = UiState::default();
        let mut input = tick(Vec2::splat(20.0));
        input.primary = UiButton {
            down: true,
            pressed: true,
            released: false,
        };
        ui.update(&[region], input);
        input = tick(Vec2::splat(70.0));
        input.primary.released = true;
        assert!(ui.update(&[region], input).pointer);
        let response = ui.response(region.id).unwrap();
        assert!(!response.activated);
        assert_eq!(response.drag_ended, draggable);
        assert_eq!(
            response.drag_delta,
            if draggable {
                Vec2::splat(50.0)
            } else {
                Vec2::ZERO
            }
        );
    }
}
