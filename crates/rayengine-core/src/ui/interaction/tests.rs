use super::*;
use crate::{
    ui::UiRect,
    viewport::{ScaleMode, Viewport},
};

fn region(id: u64, x: f32) -> UiRegion {
    UiRegion::new(
        UiId(id),
        Aabb2::from_center(Vec2::new(x, 20.0), Vec2::splat(30.0)),
    )
}
fn input(pointer: Option<Vec2>, primary: UiButton) -> UiInput {
    UiInput {
        pointer,
        primary,
        window_focused: true,
        ..UiInput::default()
    }
}
fn press() -> UiButton {
    UiButton {
        down: true,
        pressed: true,
        released: false,
    }
}
fn held() -> UiButton {
    UiButton {
        down: true,
        ..UiButton::default()
    }
}
fn release() -> UiButton {
    UiButton {
        released: true,
        ..UiButton::default()
    }
}

#[test]
fn clicks_require_the_original_capture_and_release_inside() {
    let mut ui = UiState::default();
    let regions = [region(1, 20.0), region(2, 70.0)];
    let capture = ui.update(&regions, input(Some(Vec2::splat(20.0)), press()));
    assert!(capture.pointer && capture.keyboard);
    assert!(ui.response(UiId(1)).unwrap().pressed);
    ui.update(&regions, input(Some(Vec2::new(70.0, 20.0)), held()));
    assert!(ui.response(UiId(1)).unwrap().held);
    ui.update(&regions, input(Some(Vec2::new(70.0, 20.0)), release()));
    assert!(ui.response(UiId(1)).unwrap().released);
    assert!(!ui.responses().iter().any(|r| r.activated));
    ui.update(&regions, input(Some(Vec2::splat(20.0)), release()));
    assert!(!ui.response(UiId(1)).unwrap().activated); // No fresh press.
    ui.update(&regions, input(Some(Vec2::splat(20.0)), press()));
    ui.update(&regions, input(Some(Vec2::splat(20.0)), release()));
    assert!(ui.response(UiId(1)).unwrap().activated);
    ui.update(
        &regions,
        input(Some(Vec2::splat(20.0)), UiButton::default()),
    );
    assert!(!ui.response(UiId(1)).unwrap().activated);
}

#[test]
fn quick_taps_sampled_between_ticks_activate_once() {
    let mut raw = Input::with_capacity(5);
    let actions = UiActions {
        primary: Action(0),
        next: Action(1),
        previous: Action(2),
        activate: Action(3),
        cancel: Action(4),
    };
    raw.set(actions.primary, true);
    raw.set(actions.primary, false);
    let mut ui = UiState::default();
    let regions = [region(1, 20.0)];
    ui.update(
        &regions,
        UiInput::from_actions(&raw, Some(Vec2::splat(20.0)), actions, true),
    );
    let response = ui.response(UiId(1)).unwrap();
    assert!(response.pressed && response.released && response.activated);
    assert!(!response.held);
    raw.consume_edges();
    ui.update(
        &regions,
        UiInput::from_actions(&raw, Some(Vec2::splat(20.0)), actions, true),
    );
    assert!(!ui.response(UiId(1)).unwrap().activated);
}

#[test]
fn focus_wraps_skips_disabled_regions_and_survives_reordering() {
    let mut ui = UiState::default();
    let mut regions = [region(1, 20.0), region(2, 70.0), region(3, 120.0)];
    regions[1].enabled = false;
    let next = UiInput {
        next: true,
        window_focused: true,
        ..UiInput::default()
    };
    let prev = UiInput {
        next: false,
        previous: true,
        ..next
    };
    ui.update(&regions, next);
    assert_eq!(ui.focused(), Some(UiId(1)));
    ui.update(&regions, next);
    assert_eq!(ui.focused(), Some(UiId(3)));
    ui.update(&regions, next);
    assert_eq!(ui.focused(), Some(UiId(1)));
    ui.update(&regions, prev);
    assert_eq!(ui.focused(), Some(UiId(3)));
    regions.swap(0, 2);
    ui.update(
        &regions,
        UiInput {
            activate: true,
            window_focused: true,
            ..UiInput::default()
        },
    );
    assert!(ui.response(UiId(3)).unwrap().activated);
    assert!(ui.response(UiId(3)).unwrap().focused);
    ui.update(
        &regions,
        UiInput {
            cancel: true,
            window_focused: true,
            ..UiInput::default()
        },
    );
    assert_eq!(ui.focused(), None);
    ui.update(&[], next); // Empty menus are valid.
    assert_eq!(ui.focused(), None);
}

#[test]
fn disabled_top_region_blocks_click_through_and_nonfocusable_surfaces_can_drag() {
    let mut ui = UiState::default();
    let mut regions = [region(1, 20.0), region(2, 20.0)];
    regions[1].enabled = false;
    let capture = ui.update(&regions, input(Some(Vec2::splat(20.0)), press()));
    assert!(capture.pointer);
    assert!(!capture.keyboard);
    assert!(ui.response(UiId(2)).unwrap().hovered);
    assert!(!ui.responses().iter().any(|r| r.pressed));
    regions[1].enabled = true;
    regions[1].focusable = false;
    regions[1].draggable = true;
    ui.update(&regions, input(Some(Vec2::splat(20.0)), press()));
    assert!(ui.response(UiId(2)).unwrap().drag_started);
    assert_eq!(ui.focused(), None);
}

#[test]
fn drags_keep_capture_outside_regions_and_bars_then_end_on_release() {
    let mut ui = UiState::default();
    let mut regions = [region(1, 20.0)];
    regions[0].draggable = true;
    ui.update(&regions, input(Some(Vec2::splat(20.0)), press()));
    assert!(ui.response(UiId(1)).unwrap().drag_started);
    ui.update(&regions, input(Some(Vec2::new(90.0, 45.0)), held()));
    let response = ui.response(UiId(1)).unwrap();
    assert_eq!(response.drag_delta, Vec2::new(70.0, 25.0));
    assert_eq!(response.drag_total, response.drag_delta);
    assert!(response.held && !response.hovered);
    assert!(ui.update(&regions, input(None, held())).pointer);
    assert_eq!(ui.response(UiId(1)).unwrap().drag_delta, Vec2::ZERO);
    ui.update(&regions, input(Some(Vec2::new(100.0, 50.0)), release()));
    let response = ui.response(UiId(1)).unwrap();
    assert!(response.drag_ended && response.released && !response.cancelled && !response.activated);
    assert_eq!(response.drag_delta, Vec2::new(10.0, 5.0));
    assert_eq!(response.drag_total, Vec2::new(80.0, 30.0));
}

#[test]
fn focus_loss_removal_disable_and_cancel_end_capture() {
    for cause in 0..4 {
        let mut ui = UiState::default();
        let mut regions = vec![region(1, 20.0)];
        regions[0].draggable = true;
        ui.update(&regions, input(Some(Vec2::splat(20.0)), press()));
        let mut tick = input(Some(Vec2::splat(20.0)), held());
        match cause {
            0 => tick.window_focused = false,
            1 => regions.clear(),
            2 => regions[0].enabled = false,
            _ => tick.cancel = true,
        }
        ui.update(&regions, tick);
        let response = ui.response(UiId(1)).unwrap();
        assert!(
            response.cancelled && response.drag_ended && response.released && !response.activated
        );
        assert_eq!(ui.focused(), None);
        ui.update(&regions, input(None, UiButton::default()));
        assert!(!ui.response(UiId(1)).is_some_and(|r| r.cancelled));
    }
}

#[test]
fn current_reference_layout_and_pointer_mapping_are_used_after_resize() {
    let layout = UiRect::bottom_right(Vec2::splat(-20.0), Vec2::new(120.0, 40.0));
    for mode in [ScaleMode::Fit, ScaleMode::IntegerFit, ScaleMode::Expand] {
        let mut ui = UiState::with_capacity(1);
        for window in [
            Vec2::new(1280.0, 720.0),
            Vec2::new(800.0, 1000.0),
            Vec2::new(2400.0, 900.0),
        ] {
            let view = Viewport::new(window, Vec2::new(960.0, 540.0), mode).unwrap();
            let bounds = layout.resolve(view.logical_size);
            let pointer = view.screen_to_ui(view.ui_to_screen(bounds.center()));
            let region = UiRegion::new(UiId(1), bounds);
            ui.update(&[region], input(pointer, press()));
            ui.update(&[region], input(pointer, release()));
            assert!(ui.response(UiId(1)).unwrap().activated);
            assert_eq!(ui.focused(), Some(UiId(1)));
        }
        assert_eq!(ui.responses.capacity(), 2); // No growth within the declared size.
    }
}

#[test]
fn routing_masks_gameplay_without_consuming_ui_input_or_replaying_edges() {
    let jump = Action(0);
    let attack = Action(1);
    let left = Action(2);
    let right = Action(3);
    let mut raw = Input::with_capacity(4);
    raw.set(jump, true);
    raw.set(attack, true);
    raw.set(right, true);
    raw.add_pointer_delta(Vec2::new(12.0, -3.0));
    let blocked = [jump, attack, right];
    let gameplay = raw.routed(&blocked, true);
    assert!(!gameplay.down(jump) && !gameplay.pressed(jump));
    assert_eq!(gameplay.axis(left, right), 0.0);
    assert_eq!(gameplay.pointer_delta(), Vec2::ZERO);
    assert!(raw.pressed(jump)); // Still available to UI.
    raw.set(attack, false);
    assert!(!raw.routed(&blocked, true).released(attack));
    assert!(raw.released(attack));
    raw.consume_edges();
    let resumed = raw.routed(&[], false);
    assert!(resumed.down(jump) && !resumed.pressed(jump));
    assert_eq!(resumed.axis(left, right), 1.0);
    assert_eq!(resumed.pointer_delta(), Vec2::ZERO);
}

#[test]
fn input_reset_cancels_capture_even_if_focus_returns_before_the_next_tick() {
    let actions = UiActions {
        primary: Action(0),
        next: Action(1),
        previous: Action(2),
        activate: Action(3),
        cancel: Action(4),
    };
    let mut raw = Input::with_capacity(5);
    let mut ui = UiState::default();
    let regions = [region(1, 20.0)];
    let pointer = Some(Vec2::splat(20.0));
    raw.set(actions.primary, true);
    ui.update(
        &regions,
        UiInput::from_actions(&raw, pointer, actions, true),
    );
    raw.consume_edges();
    raw.release_all(); // Simulation pauses here while minimized/unfocused.
    assert!(raw.reset_pending());
    ui.update(
        &regions,
        UiInput::from_actions(&raw, pointer, actions, true),
    );
    let response = ui.response(UiId(1)).unwrap();
    assert!(response.cancelled && !response.activated);
    raw.consume_edges();
    assert!(!raw.reset_pending());
    ui.update(
        &regions,
        UiInput::from_actions(&raw, pointer, actions, true),
    );
    assert!(!ui.response(UiId(1)).unwrap().activated);
}
