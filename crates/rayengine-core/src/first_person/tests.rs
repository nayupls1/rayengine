use super::*;
const DT: f32 = 1.0 / 120.0;
fn controller(config: FirstPersonConfig) -> FirstPersonController {
    FirstPersonController::new(Vec3::new(0.0, 0.9, 0.0), Vec3::new(0.8, 1.8, 0.8), config).unwrap()
}
fn floor() -> [Aabb3; 1] {
    [Aabb3::from_center(
        Vec3::new(0.0, -0.5, 0.0),
        Vec3::new(100.0, 1.0, 100.0),
    )]
}
#[test]
fn movement_is_yaw_relative_horizontal_normalized_and_sprint_configurable() {
    let config = FirstPersonConfig {
        response: 0.0,
        walk_speed: 3.0,
        sprint_speed: 7.0,
        ..Default::default()
    };
    for (yaw, forward, right) in [
        (0.0, -Vec3::Z, Vec3::X),
        (std::f32::consts::FRAC_PI_2, Vec3::X, Vec3::Z),
    ] {
        for (axis, expected) in [
            (Vec2::new(0.0, -1.0), forward),
            (Vec2::X, right),
            (Vec2::new(1.0, -1.0), (forward + right).normalize()),
        ] {
            let mut c = controller(config);
            c.set_look(yaw, config.max_pitch).unwrap();
            c.step(
                FirstPersonInput {
                    movement: axis,
                    ..Default::default()
                },
                DT,
                &floor(),
            );
            assert!(
                c.body
                    .velocity
                    .with_y(0.0)
                    .abs_diff_eq(expected * 3.0, 0.0001)
            );
            assert!(c.body.grounded);
            c.step(
                FirstPersonInput {
                    movement: axis,
                    sprint: true,
                    ..Default::default()
                },
                DT,
                &floor(),
            );
            assert!((c.body.velocity.with_y(0.0).length() - 7.0).abs() < 0.0001);
        }
    }
}
#[test]
fn mouse_look_is_dt_independent_bounded_and_keyboard_look_uses_time() {
    let input = FirstPersonInput {
        look_delta: Vec2::new(100.0, -80.0),
        ..Default::default()
    };
    let mut fast = controller(Default::default());
    let mut slow = fast;
    fast.step(input, DT, &floor());
    slow.step(input, 1.0 / 30.0, &floor());
    assert_eq!((fast.yaw(), fast.pitch()), (slow.yaw(), slow.pitch()));
    assert!((fast.yaw() - 0.25).abs() < 0.0001);
    assert!((fast.pitch() - 0.2).abs() < 0.0001);
    for delta in [Vec2::splat(f32::MAX), Vec2::splat(-f32::MAX)] {
        fast.step(
            FirstPersonInput {
                look_delta: delta,
                ..Default::default()
            },
            0.0,
            &[],
        );
        assert!(fast.yaw().is_finite() && fast.yaw().abs() <= std::f32::consts::PI);
        assert!((fast.config().min_pitch..=fast.config().max_pitch).contains(&fast.pitch()));
        assert!(fast.camera(1.0).view_matrix().is_finite());
    }
    let mut keyboard = controller(Default::default());
    for _ in 0..120 {
        keyboard.step(
            FirstPersonInput {
                turn_axis: 1.0,
                ..Default::default()
            },
            DT,
            &floor(),
        );
    }
    assert!((keyboard.yaw() - 1.8).abs() < 0.0001);
}
#[test]
fn configured_pitch_limits_clamp_initial_current_and_requested_look() {
    let mut c = controller(FirstPersonConfig {
        min_pitch: 0.2,
        max_pitch: 0.3,
        ..Default::default()
    });
    assert_eq!(c.pitch(), 0.2);
    c.set_look(10.0, 100.0).unwrap();
    assert_eq!(c.pitch(), 0.3);
    c.set_config(FirstPersonConfig {
        min_pitch: -0.2,
        max_pitch: 0.1,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(c.pitch(), 0.1);
    assert!(c.set_look(f32::NAN, 0.0).is_err());
    assert_eq!(c.pitch(), 0.1);
}
#[test]
fn small_positive_response_still_accelerates() {
    let config = FirstPersonConfig {
        response: 0.000001,
        ..Default::default()
    };
    let mut c = controller(config);
    c.step(
        FirstPersonInput {
            movement: Vec2::X,
            ..Default::default()
        },
        DT,
        &floor(),
    );
    let expected =
        f64::from(config.walk_speed) * (1.0 - (-f64::from(config.response) * f64::from(DT)).exp());
    assert!((f64::from(c.body.velocity.x) - expected).abs() < expected * 0.00001);
}
#[test]
fn near_pole_pitch_limits_produce_a_valid_camera_at_nonzero_coordinates() {
    let config = FirstPersonConfig {
        min_pitch: (-std::f32::consts::FRAC_PI_2).next_up(),
        max_pitch: std::f32::consts::FRAC_PI_2.next_down(),
        ..Default::default()
    };
    let mut c = controller(config);
    c.teleport(Vec3::new(6.0, 1.0, 6.0)).unwrap();
    for yaw in [0.0, std::f32::consts::FRAC_PI_2, 0.7] {
        for pitch in [config.min_pitch, config.max_pitch] {
            c.set_look(yaw, pitch).unwrap();
            let camera = c.camera(1.0);
            assert!(camera.view_matrix().is_finite());
            // Looking vertically still retains the yaw's horizontal right axis.
            let right = (camera.target - camera.position)
                .cross(camera.up)
                .normalize();
            assert!(right.abs_diff_eq(Vec3::new(yaw.cos(), 0.0, yaw.sin()), 0.00001));
        }
    }
}
#[test]
fn routed_actions_and_edges_prevent_repeated_mouse_motion_and_held_jumps() {
    let actions = FirstPersonActions {
        left: Action(20),
        right: Action(21),
        forward: Action(22),
        back: Action(23),
        jump: Action(24),
        sprint: Some(Action(25)),
        turn_left: None,
        turn_right: Some(Action(26)),
    };
    let mut input = Input::with_capacity(27);
    input.set(actions.forward, true);
    input.set(actions.jump, true);
    input.set(actions.sprint.unwrap(), true);
    input.add_pointer_delta(Vec2::new(10.0, 20.0));
    let masked = FirstPersonInput::from_view(
        input.routed(&[actions.forward, actions.jump], true),
        actions,
    );
    assert_eq!(masked.movement, Vec2::ZERO);
    assert_eq!(masked.look_delta, Vec2::ZERO);
    assert!(!masked.jump_pressed && masked.sprint);
    let tick = FirstPersonInput::from_actions(&input, actions);
    assert!(tick.jump_pressed);
    assert_eq!(tick.movement.y, -1.0);
    input.consume_edges();
    let catch_up = FirstPersonInput::from_actions(&input, actions);
    assert!(!catch_up.jump_pressed);
    assert_eq!(catch_up.look_delta, Vec2::ZERO);
    assert_eq!(catch_up.movement.y, -1.0);
}
#[test]
fn jumping_lands_and_does_not_repeat_without_another_edge() {
    let mut c = controller(Default::default());
    c.step(Default::default(), DT, &floor());
    assert!(c.body.grounded);
    c.step(
        FirstPersonInput {
            jump_pressed: true,
            ..Default::default()
        },
        DT,
        &floor(),
    );
    assert!(c.body.velocity.y > 0.0 && !c.body.grounded);
    for _ in 0..240 {
        c.step(Default::default(), DT, &floor());
    }
    assert!(c.body.grounded && (c.body.position.y - 0.9).abs() < 0.0001);
    assert_eq!(c.body.velocity.y, 0.0);
}
#[test]
fn zero_grace_windows_allow_a_grounded_edge_but_not_a_midair_jump() {
    let mut c = controller(FirstPersonConfig {
        coyote_time: 0.0,
        jump_buffer_time: 0.0,
        ..Default::default()
    });
    c.step(Default::default(), DT, &floor());
    c.step(
        FirstPersonInput {
            jump_pressed: true,
            ..Default::default()
        },
        DT,
        &floor(),
    );
    let velocity = c.body.velocity.y;
    c.step(
        FirstPersonInput {
            jump_pressed: true,
            ..Default::default()
        },
        DT,
        &floor(),
    );
    assert!(c.body.velocity.y < velocity);
}
#[test]
fn coyote_and_buffered_jumps_work_once_and_teleport_clears_them() {
    let mut c = controller(Default::default());
    c.step(Default::default(), DT, &floor());
    c.step(Default::default(), DT, &[]); // leave support
    assert!(!c.body.grounded);
    c.step(
        FirstPersonInput {
            jump_pressed: true,
            ..Default::default()
        },
        DT,
        &[],
    );
    assert!(c.body.velocity.y > 0.0);
    c.teleport(Vec3::new(0.0, 0.91, 0.0)).unwrap();
    c.body.velocity.y = -2.0;
    c.step(
        FirstPersonInput {
            jump_pressed: true,
            ..Default::default()
        },
        DT,
        &floor(),
    );
    assert!(c.body.grounded); // edge retained while landing
    c.step(Default::default(), DT, &floor());
    assert!(c.body.velocity.y > 0.0);
    c.teleport(Vec3::new(0.0, 0.9, 0.0)).unwrap();
    for _ in 0..10 {
        c.step(Default::default(), DT, &floor());
    }
    assert!(c.body.grounded && c.body.velocity.y == 0.0);
    let mut expired = controller(Default::default());
    expired.step(Default::default(), DT, &floor());
    for _ in 0..30 {
        expired.step(Default::default(), DT, &[]);
    }
    expired.step(
        FirstPersonInput {
            jump_pressed: true,
            ..Default::default()
        },
        DT,
        &[],
    );
    assert!(expired.body.velocity.y < 0.0);
}
#[test]
fn walls_ceilings_and_terminal_fall_speed_use_body_contacts() {
    let config = FirstPersonConfig {
        response: 0.0,
        ..Default::default()
    };
    let mut c = controller(config);
    let wall = Aabb3::from_center(Vec3::new(2.0, 2.0, 0.0), Vec3::new(1.0, 4.0, 100.0));
    let solids = [floor()[0], wall];
    for _ in 0..120 {
        c.step(
            FirstPersonInput {
                movement: Vec2::X,
                ..Default::default()
            },
            DT,
            &solids,
        );
    }
    assert!(c.body.bounds().max.x <= wall.min.x);
    assert_eq!(c.body.velocity.x, 0.0);
    let ceiling = Aabb3::from_center(Vec3::new(0.0, 2.2, 0.0), Vec3::new(100.0, 0.2, 100.0));
    c.step(
        FirstPersonInput {
            jump_pressed: true,
            ..Default::default()
        },
        0.1,
        &[floor()[0], ceiling],
    );
    assert!(c.body.bounds().max.y <= ceiling.min.y);
    assert_eq!(c.body.velocity.y, 0.0);
    assert!(!c.body.grounded);
    for _ in 0..240 {
        c.step(Default::default(), DT, &[]);
    }
    assert_eq!(c.body.velocity.y, -config.max_fall_speed);
}
#[test]
fn camera_interpolates_position_uses_latest_angles_and_teleports_without_smearing() {
    let mut c = controller(Default::default());
    c.step(
        FirstPersonInput {
            movement: Vec2::X,
            look_delta: Vec2::new(10.0, -10.0),
            ..Default::default()
        },
        DT,
        &floor(),
    );
    for alpha in [0.0, 0.5, 1.0] {
        let camera = c.camera(alpha);
        assert!(camera.position.abs_diff_eq(
            c.previous.lerp(c.body.position, alpha) + c.config().eye_offset,
            0.00001
        ));
        assert!(((camera.target - camera.position).length() - 1.0).abs() < 0.00001);
        assert_eq!(camera.vertical_fov, c.config().vertical_fov);
    }
    let angles = (c.yaw(), c.pitch());
    c.teleport(Vec3::new(10.0, 5.0, 10.0)).unwrap();
    assert_eq!(c.camera(0.0).position, c.camera(1.0).position);
    assert_eq!((c.yaw(), c.pitch()), angles);
}
#[test]
fn zero_dt_retains_contact_and_interpolation_and_invalid_settings_are_atomic() {
    let mut c = controller(Default::default());
    c.step(Default::default(), DT, &floor());
    let (position, previous) = (c.body.position, c.previous);
    c.step(
        FirstPersonInput {
            look_delta: Vec2::ONE,
            jump_pressed: true,
            ..Default::default()
        },
        0.0,
        &[],
    );
    assert!(c.body.grounded);
    assert_eq!((c.body.position, c.previous), (position, previous));
    let mut bad = *c.config();
    bad.gravity = f32::NAN;
    assert!(c.set_config(bad).is_err());
    assert_eq!(c.config().gravity, 26.0);
    for bad in [
        FirstPersonConfig {
            response: -1.0,
            ..Default::default()
        },
        FirstPersonConfig {
            min_pitch: 1.0,
            max_pitch: 0.5,
            ..Default::default()
        },
        FirstPersonConfig {
            max_pitch: std::f32::consts::FRAC_PI_2,
            ..Default::default()
        },
        FirstPersonConfig {
            vertical_fov: 180.0,
            ..Default::default()
        },
        FirstPersonConfig {
            eye_offset: Vec3::NAN,
            ..Default::default()
        },
    ] {
        assert!(bad.validate().is_err());
    }
    assert!(FirstPersonController::new(Vec3::ZERO, Vec3::ZERO, Default::default()).is_err());
    assert!(
        FirstPersonController::new(Vec3::splat(f32::MAX), Vec3::ONE, Default::default()).is_err()
    );
    assert!(c.teleport(Vec3::splat(f32::MAX)).is_err());
    assert!(c.teleport(Vec3::NAN).is_err());
    assert_eq!(c.body.position, position);
}
