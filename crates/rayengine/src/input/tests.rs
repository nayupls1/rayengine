use super::*;

const MOVE: Axis = Axis(0);
const JUMP: Action = Action(0);

struct Backend {
    focused: bool,
    keys: Vec<KeyboardKey>,
    tap: bool,
    pad: Option<f32>,
    pad_button: bool,
    requested_device: i32,
}
impl Default for Backend {
    fn default() -> Self {
        Self {
            focused: true,
            keys: Vec::new(),
            tap: false,
            pad: None,
            pad_button: false,
            requested_device: 2,
        }
    }
}
impl PhysicalInput for Backend {
    fn focused(&self) -> bool {
        self.focused
    }
    fn button(&self, button: Button) -> (bool, bool) {
        match button {
            Button::Key(key) => (
                self.keys.contains(&key),
                key == KeyboardKey::KEY_SPACE && self.tap,
            ),
            Button::Gamepad { device, .. } => (
                device == self.requested_device && self.pad.is_some() && self.pad_button,
                false,
            ),
            _ => (false, false),
        }
    }
    fn axis(&self, device: i32, _: GamepadAxis) -> Option<f32> {
        if device == self.requested_device {
            self.pad
        } else {
            None
        }
    }
}
fn keyboard() -> AxisBinding {
    AxisBinding::new(AxisSource::Buttons {
        negative: KeyboardKey::KEY_A.into(),
        positive: KeyboardKey::KEY_D.into(),
    })
}
fn stick() -> AxisBinding {
    AxisBinding::new(AxisSource::Gamepad {
        device: 2,
        axis: GamepadAxis::GAMEPAD_AXIS_LEFT_X,
    })
}
fn trigger() -> AxisBinding {
    AxisBinding::new(AxisSource::Gamepad {
        device: 2,
        axis: GamepadAxis::GAMEPAD_AXIS_RIGHT_TRIGGER,
    })
}
fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
}

#[test]
fn normalization_dead_zone_inversion_sensitivity_and_invalid_samples() {
    let mut binding = stick();
    binding.dead_zone = 0.2;
    for raw in [-0.2, 0.0, 0.2] {
        close(binding.process(Some(raw)), 0.0);
    }
    close(binding.process(Some(0.6)), 0.5);
    close(binding.process(Some(-0.6)), -0.5);
    close(binding.process(Some(2.0)), 1.0);
    binding.inverted = true;
    binding.sensitivity = 2.0;
    close(binding.process(Some(0.6)), -1.0);
    close(binding.process(None), 0.0);
    close(binding.process(Some(f32::NAN)), 0.0);
    close(binding.process(Some(f32::INFINITY)), 0.0);
    let mut trigger = trigger();
    trigger.dead_zone = 0.0;
    close(trigger.process(Some(-1.0)), 0.0);
    close(trigger.process(Some(0.0)), 0.5);
    close(trigger.process(Some(1.0)), 1.0);
    close(trigger.process(None), 0.0); // Never turns missing raw zero into half pressure.
    trigger.inverted = true;
    close(trigger.process(Some(1.0)), -1.0);
}

#[test]
fn keyboard_and_controller_share_intent_with_stable_conflict_resolution() {
    let bindings = Bindings::new()
        .bind_axis(MOVE, keyboard())
        .unwrap()
        .bind_axis(MOVE, stick())
        .unwrap();
    let mut input = Input::default();
    let mut backend = Backend {
        keys: vec![KeyboardKey::KEY_D],
        pad: Some(-0.6),
        ..Backend::default()
    };
    bindings.sample_from(&backend, &mut input);
    close(input.value(MOVE), 1.0);
    backend.pad = Some(-1.0); // Equal magnitude: earlier keyboard wins.
    bindings.sample_from(&backend, &mut input);
    close(input.value(MOVE), 1.0);
    backend.keys.push(KeyboardKey::KEY_A); // Keyboard opposites cancel, stick still wins.
    bindings.sample_from(&backend, &mut input);
    close(input.value(MOVE), -1.0);
    backend.keys.clear();
    backend.pad = Some(0.575);
    bindings.sample_from(&backend, &mut input);
    close(input.value(MOVE), 0.5);
    backend.requested_device = 0; // No implicit fallback to another device.
    bindings.sample_from(&backend, &mut input);
    close(input.value(MOVE), 0.0);
}

#[test]
fn values_and_button_edges_across_render_and_fixed_rates_disconnect_and_focus() {
    let bindings = Bindings::new()
        .bind(JUMP, KeyboardKey::KEY_SPACE)
        .bind(
            JUMP,
            Button::Gamepad {
                device: 2,
                button: GamepadButton::GAMEPAD_BUTTON_RIGHT_FACE_DOWN,
            },
        )
        .bind_axis(MOVE, trigger())
        .unwrap();
    let mut backend = Backend {
        tap: true,
        pad: Some(1.0),
        pad_button: true,
        ..Backend::default()
    };
    let mut input = Input::default();
    bindings.sample_from(&backend, &mut input);
    backend.tap = false;
    bindings.sample_from(&backend, &mut input); // No fixed update between these frames.
    assert!(input.pressed(JUMP));
    close(input.value(MOVE), 1.0);
    input.consume_edges();
    for _ in 0..3 {
        // Catch-up ticks retain velocity, never repeat edges.
        assert!(input.down(JUMP));
        assert!(!input.pressed(JUMP));
        close(input.value(MOVE), 1.0);
        input.consume_edges();
    }
    backend.pad = None;
    bindings.sample_from(&backend, &mut input);
    assert!(input.released(JUMP));
    assert!(!input.down(JUMP));
    close(input.value(MOVE), 0.0);
    input.consume_edges();
    backend.tap = true; // Backend quick tap not held at sample time.
    bindings.sample_from(&backend, &mut input);
    assert!(input.pressed(JUMP) && input.released(JUMP));
    input.consume_edges();
    backend.keys.push(KeyboardKey::KEY_SPACE);
    backend.pad = Some(1.0);
    bindings.sample_from(&backend, &mut input);
    backend.focused = false;
    bindings.sample_from(&backend, &mut input);
    assert!(!input.down(JUMP) && input.released(JUMP) && input.reset_pending());
    close(input.value(MOVE), 0.0);
}

#[test]
fn button_or_release_one_source_does_not_release_action() {
    let bindings = Bindings::new()
        .bind(JUMP, KeyboardKey::KEY_SPACE)
        .bind(JUMP, KeyboardKey::KEY_W);
    let mut backend = Backend {
        keys: vec![KeyboardKey::KEY_SPACE, KeyboardKey::KEY_W],
        ..Backend::default()
    };
    let mut input = Input::default();
    bindings.sample_from(&backend, &mut input);
    input.consume_edges();
    backend.keys.remove(0);
    bindings.sample_from(&backend, &mut input);
    assert!(input.down(JUMP) && !input.released(JUMP) && !input.pressed(JUMP));
}

#[test]
fn runtime_rebind_remove_and_complete_replace_clear_only_changed_states() {
    let unchanged = Action(1);
    let mut bindings = Bindings::new()
        .bind(JUMP, KeyboardKey::KEY_SPACE)
        .bind(unchanged, KeyboardKey::KEY_W)
        .bind_axis(MOVE, keyboard())
        .unwrap();
    let mut previous = bindings.clone();
    let mut backend = Backend {
        keys: vec![
            KeyboardKey::KEY_SPACE,
            KeyboardKey::KEY_W,
            KeyboardKey::KEY_D,
        ],
        ..Backend::default()
    };
    let mut input = Input::default();
    bindings.sample_from(&backend, &mut input);
    input.consume_edges();
    bindings
        .rebind(JUMP, vec![KeyboardKey::KEY_ENTER.into()])
        .unwrap();
    bindings.remove_axis(MOVE);
    bindings.reconcile(&mut previous, &mut input).unwrap();
    assert!(!input.down(JUMP) && input.released(JUMP));
    close(input.value(MOVE), 0.0);
    assert!(input.down(unchanged) && !input.released(unchanged));
    input.consume_edges(); // A second fixed tick before a new frame remains neutral.
    backend.keys.push(KeyboardKey::KEY_ENTER);
    bindings.sample_from(&backend, &mut input);
    assert!(input.down(JUMP) && input.pressed(JUMP));
    bindings.replace(BindingConfig::default()).unwrap();
    input.consume_edges();
    bindings.reconcile(&mut previous, &mut input).unwrap();
    assert!(input.released(JUMP) && input.released(unchanged));
    assert!(!input.down(JUMP) && !input.down(unchanged));
}

#[test]
fn invalid_runtime_changes_leave_previous_configuration_intact() {
    let mut bindings = Bindings::new()
        .bind(JUMP, KeyboardKey::KEY_SPACE)
        .bind_axis(MOVE, keyboard())
        .unwrap();
    let previous = bindings.clone();
    let mut bad = stick();
    bad.dead_zone = 1.0;
    assert!(bindings.rebind_axis(MOVE, vec![bad]).is_err());
    assert!(
        bindings
            .rebind(
                JUMP,
                vec![Button::Gamepad {
                    device: -1,
                    button: GamepadButton::GAMEPAD_BUTTON_LEFT_THUMB
                }]
            )
            .is_err()
    );
    let mut config = bindings.config();
    config.schema_version = 99;
    assert!(bindings.replace(config).is_err());
    assert_eq!(bindings, previous);
    assert!(!bindings.remove_button(JUMP, KeyboardKey::KEY_ENTER.into()));
    assert!(bindings.remove_button(JUMP, KeyboardKey::KEY_SPACE.into()));
    assert!(bindings.buttons(JUMP).is_empty());
}

#[test]
fn settings_round_trip_all_sources_and_explicit_game_owned_files() {
    let mut bindings = Bindings::new()
        .bind(JUMP, KeyboardKey::KEY_SPACE)
        .bind(JUMP, Button::Mouse(MouseButton::MOUSE_BUTTON_LEFT))
        .bind(
            JUMP,
            Button::Gamepad {
                device: 2,
                button: GamepadButton::GAMEPAD_BUTTON_RIGHT_FACE_DOWN,
            },
        )
        .bind_axis(MOVE, keyboard())
        .unwrap()
        .bind_axis(MOVE, stick())
        .unwrap();
    let mut trigger = trigger();
    trigger.inverted = true;
    trigger.sensitivity = 0.75;
    bindings.add_axis(Axis(1), trigger).unwrap();
    assert_eq!(
        Bindings::from_json(&bindings.to_json().unwrap()).unwrap(),
        bindings
    );
    let path = std::env::temp_dir().join(format!("rayengine-bindings-{}.json", std::process::id()));
    bindings.save(&path).unwrap();
    assert_eq!(Bindings::load(&path).unwrap(), bindings);
    std::fs::remove_file(&path).unwrap();
    assert!(matches!(Bindings::load(&path), Err(Error::Io(_))));
}

#[test]
fn invalid_configuration_reports_context_and_rejects_unknown_fields_and_enums() {
    let bindings = Bindings::new()
        .bind(JUMP, KeyboardKey::KEY_SPACE)
        .bind_axis(MOVE, stick())
        .unwrap();
    let mut config = bindings.config();
    config.actions.push(config.actions[0].clone());
    assert!(
        config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("duplicate action ID")
    );
    let mut config = bindings.config();
    config.axes.push(config.axes[0].clone());
    assert!(
        config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("duplicate axis ID")
    );
    for value in [f32::NAN, f32::INFINITY, -0.1, 1.0] {
        let mut config = bindings.config();
        config.axes[0].sources[0].dead_zone = value;
        assert!(
            config
                .validate()
                .unwrap_err()
                .to_string()
                .contains("axis 0 source 0: dead_zone")
        );
    }
    for value in [f32::NAN, f32::INFINITY, -1.0, 101.0] {
        let mut config = bindings.config();
        config.axes[0].sources[0].sensitivity = value;
        assert!(
            config
                .validate()
                .unwrap_err()
                .to_string()
                .contains("sensitivity")
        );
    }
    let json = bindings.to_json().unwrap();
    for invalid in [
        json.replace("KEY_SPACE", "KEY_UNKNOWN"),
        json.replace("\"schema_version\": 1", "\"schema_version\": 2"),
        json.replace(
            "\"schema_version\": 1",
            "\"schema_version\": 1, \"typo\": true",
        ),
        json.replace("\"dead_zone\": 0.15", "\"dead_zone\": 0.15, \"typo\": true"),
        json.replace("\"device\": 2", "\"device\": 4"),
        json.replace("\"device\": 2", "\"device\": -1"),
        json.replace("\"action\": 0", "\"action\": 65536"),
        json.replace("\"dead_zone\": 0.15", "\"dead_zone\": null"),
    ] {
        assert!(
            Bindings::from_json(&invalid).is_err(),
            "accepted: {invalid}"
        );
    }
}

#[test]
fn invalid_whole_set_assignment_is_rejected_before_reconciling_or_sampling() {
    let mut previous = Bindings::new().bind(JUMP, KeyboardKey::KEY_SPACE);
    let original = previous.clone();
    let mut input = Input::default();
    input.set(JUMP, true);
    input.consume_edges();
    for device in [-1, 4, i32::MIN, i32::MAX] {
        let assigned = Bindings::new().bind(
            JUMP,
            Button::Gamepad {
                device,
                button: GamepadButton::GAMEPAD_BUTTON_LEFT_THUMB,
            },
        );
        assert!(matches!(
            assigned.reconcile(&mut previous, &mut input),
            Err(Error::Config(_))
        ));
        assert_eq!(previous, original);
        assert!(input.down(JUMP) && !input.released(JUMP));
    }
}
