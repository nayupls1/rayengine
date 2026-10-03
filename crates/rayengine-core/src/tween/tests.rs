use super::*;
use crate::camera::{Camera2D, Camera3D};
use crate::events::Events;
use glam::{Quat, Vec2, Vec3, Vec4};

fn ms(value: u64) -> Duration {
    Duration::from_millis(value)
}

#[test]
fn every_curve_is_pinned_at_and_beyond_its_endpoints() {
    for ease in Ease::ALL {
        assert_eq!(ease.apply(0.0), 0.0, "{ease:?}");
        assert_eq!(ease.apply(1.0), 1.0, "{ease:?}");
        assert_eq!(ease.apply(-3.0), 0.0, "{ease:?}");
        assert_eq!(ease.apply(f32::NEG_INFINITY), 0.0, "{ease:?}");
        assert_eq!(ease.apply(f32::NAN), 0.0, "{ease:?}");
        assert_eq!(ease.apply(7.0), 1.0, "{ease:?}");
        assert_eq!(ease.apply(f32::INFINITY), 1.0, "{ease:?}");
        assert_eq!(ease.function()(0.5), ease.apply(0.5));
        // Curves approach their endpoints continuously.
        assert!(ease.apply(1e-6).abs() < 0.01, "{ease:?}");
        assert!((ease.apply(1.0 - 1e-6) - 1.0).abs() < 0.01, "{ease:?}");
        for step in 0..=1000 {
            assert!(ease.apply(step as f32 / 1000.0).is_finite(), "{ease:?}");
        }
    }
}

#[test]
fn curves_match_reference_values() {
    type Case = (fn(f32) -> f32, f32, f32);
    let cases: [Case; 22] = [
        (ease::linear, 0.3, 0.3),
        (ease::quad_in, 0.5, 0.25),
        (ease::quad_out, 0.5, 0.75),
        (ease::quad_in_out, 0.25, 0.125),
        (ease::cubic_in, 0.5, 0.125),
        (ease::cubic_out, 0.5, 0.875),
        (ease::cubic_in_out, 0.75, 0.9375),
        (ease::sine_in, 0.5, 1.0 - std::f32::consts::FRAC_1_SQRT_2),
        (ease::sine_out, 0.5, std::f32::consts::FRAC_1_SQRT_2),
        (ease::sine_in_out, 0.5, 0.5),
        (ease::expo_in, 0.5, 0.03125),
        (ease::expo_out, 0.5, 0.96875),
        (ease::expo_in_out, 0.5, 0.5),
        (ease::back_in, 0.5, -0.0876975),
        (ease::back_out, 0.5, 1.0876975),
        (ease::back_in_out, 0.5, 0.5),
        (ease::elastic_in, 0.5, -0.015625),
        (ease::elastic_out, 0.5, 1.015625),
        (ease::elastic_in_out, 0.5, 0.5),
        (ease::bounce_in, 0.5, 0.234375),
        (ease::bounce_out, 0.5, 0.765625),
        (ease::bounce_in_out, 0.5, 0.5),
    ];
    for (curve, t, expected) in cases {
        assert!((curve(t) - expected).abs() < 1e-5, "{t}: {}", curve(t));
    }
    // In/out pairs mirror each other, and in-out curves are symmetric.
    for t in [0.1, 0.33, 0.6, 0.9] {
        assert!((ease::bounce_in(t) - (1.0 - ease::bounce_out(1.0 - t))).abs() < 1e-6);
        assert!((ease::cubic_in_out(t) - (1.0 - ease::cubic_in_out(1.0 - t))).abs() < 1e-6);
    }
    assert!(ease::back_in(0.2) < 0.0 && ease::back_out(0.8) > 1.0);
}

#[test]
fn one_shot_tween_eases_holds_and_completes_exactly_once() {
    let mut tween = Tween::new(10.0_f32, 20.0, ms(100))
        .with_ease(Ease::QuadIn)
        .with_id(TweenId(3));
    assert_eq!(tween.value(), 10.0);
    assert_eq!(tween.advance(ms(50)), None);
    assert_eq!(tween.progress(), 0.5);
    assert_eq!(tween.value(), 12.5);
    assert_eq!(
        tween.advance(ms(50)),
        Some(TweenCompleted { id: TweenId(3) })
    );
    assert!(tween.is_finished());
    assert_eq!(tween.value(), 20.0);
    assert_eq!(tween.advance(Duration::MAX), None);
    assert_eq!(tween.value(), 20.0);
    tween.reset();
    assert_eq!(tween.value(), 10.0);
    assert!(tween.advance(Duration::MAX).is_some());
}

#[test]
fn endpoints_are_exact_for_every_value_type() {
    let mut v2 = Tween::new(Vec2::new(0.1, 0.7), Vec2::new(1.3, -9.1), ms(30));
    v2.advance(ms(30));
    assert_eq!(v2.value(), Vec2::new(1.3, -9.1));
    let mut v3 =
        Tween::new(Vec3::splat(0.1), Vec3::new(3.3, 0.7, -1.9), ms(7)).with_ease(Ease::ElasticOut);
    v3.advance(ms(7));
    assert_eq!(v3.value(), Vec3::new(3.3, 0.7, -1.9));
    let mut color = Tween::new(Vec4::ONE, Vec4::new(1.0, 0.1, 0.1, 0.5), ms(9));
    color.advance(ms(9));
    assert_eq!(color.value(), Vec4::new(1.0, 0.1, 0.1, 0.5));
    let door = Quat::from_rotation_y(1.5);
    let mut hinge = Tween::new(Quat::IDENTITY, door, ms(10));
    hinge.advance(ms(5));
    assert!(hinge.value().abs_diff_eq(Quat::from_rotation_y(0.75), 1e-6));
    hinge.advance(ms(5));
    assert_eq!(hinge.value(), door);
}

#[test]
fn byte_colors_round_and_clamp_overshoot() {
    let mut flash = Tween::new([0, 100, 255, 255], [255, 0, 0, 255], ms(100));
    flash.advance(ms(50));
    assert_eq!(flash.value(), [128, 50, 128, 255]);
    let mut overshoot =
        Tween::new([250, 5, 0, 0], [255, 0, 0, 0], ms(100)).with_ease(Ease::BackOut);
    overshoot.advance(ms(60));
    assert_eq!(overshoot.value(), [255, 0, 0, 0]);
    assert_eq!(
        <[u8; 4]>::interpolate([10, 10, 10, 10], [20, 0, 20, 0], -5.0),
        [0, 60, 0, 60]
    );
}

#[test]
fn delay_holds_start_value_and_carries_remaining_time() {
    let mut tween = Tween::new(0.0_f32, 100.0, ms(100)).with_delay(ms(40));
    assert_eq!(tween.delay(), ms(40));
    tween.advance(ms(30));
    assert_eq!(tween.value(), 0.0);
    assert_eq!(tween.progress(), 0.0);
    tween.advance(ms(30));
    assert_eq!(tween.value(), 20.0);
    // The delay applies once, not per loop.
    let mut looping = Tween::new(0.0_f32, 1.0, ms(10))
        .with_delay(ms(5))
        .with_mode(TweenMode::Loop);
    looping.advance(ms(5 + 10 + 3));
    assert_eq!(looping.progress(), 0.3);
    assert_eq!(looping.completed_cycles(), 1);
}

#[test]
fn zero_durations_complete_on_first_advance() {
    let mut instant = Tween::new(1.0_f32, 2.0, Duration::ZERO);
    assert_eq!(instant.value(), 1.0);
    assert!(instant.advance(Duration::ZERO).is_some());
    assert_eq!(instant.value(), 2.0);
    assert!(instant.advance(Duration::ZERO).is_none());
    let mut delayed = Tween::new(1.0_f32, 2.0, Duration::ZERO).with_delay(ms(5));
    assert!(delayed.advance(Duration::ZERO).is_none());
    assert!(delayed.advance(ms(5)).is_some());
    let mut nonzero = Tween::new(1.0_f32, 2.0, ms(1));
    assert!(nonzero.advance(Duration::ZERO).is_none());
}

#[test]
#[should_panic(expected = "repeating tween duration must be nonzero")]
fn zero_duration_loops_are_rejected() {
    let _ = Tween::new(0.0_f32, 1.0, Duration::ZERO).with_mode(TweenMode::Loop);
}

#[test]
#[should_panic(expected = "tween cycle count must be positive")]
fn zero_cycles_are_rejected() {
    let _ = Tween::new(0.0_f32, 1.0, ms(1)).with_cycles(0);
}

#[test]
fn loops_wrap_exact_boundaries_and_never_complete_by_default() {
    let mut tween = Tween::new(0.0_f32, 10.0, ms(100)).with_mode(TweenMode::Loop);
    assert_eq!(tween.advance(ms(100)), None);
    assert_eq!(tween.value(), 0.0);
    assert_eq!(tween.advance(ms(250)), None);
    assert_eq!(tween.value(), 5.0);
    assert_eq!(tween.completed_cycles(), 3);
    assert!(tween.advance(Duration::MAX).is_none());
    assert_eq!(tween.completed_cycles(), u32::MAX);
    assert!(!tween.is_finished());
}

#[test]
fn ping_pong_reverses_each_leg_and_finite_cycles_end_on_the_right_side() {
    let mut tween = Tween::new(0.0_f32, 10.0, ms(100))
        .with_mode(TweenMode::PingPong)
        .with_ease(Ease::QuadIn);
    tween.advance(ms(50));
    assert_eq!(tween.value(), 2.5);
    tween.advance(ms(100));
    // Second leg is the exact time reversal of the first.
    assert_eq!(tween.value(), 2.5);
    tween.advance(ms(50));
    assert_eq!(tween.value(), 0.0);
    tween.advance(ms(1000));
    assert_eq!(tween.value(), 0.0);

    for (cycles, end) in [(1, 10.0), (2, 0.0), (3, 10.0)] {
        let mut finite = Tween::new(0.0_f32, 10.0, ms(10))
            .with_mode(TweenMode::PingPong)
            .with_cycles(cycles);
        assert_eq!(finite.advance(ms(10) * cycles - ms(1)), None);
        assert!(finite.advance(ms(5)).is_some());
        assert_eq!(finite.value(), end, "{cycles}");
        assert_eq!(finite.completed_cycles(), cycles);
        let mut skipped = finite;
        skipped.reset();
        assert!(skipped.finish().is_some());
        assert_eq!(skipped.value(), end, "{cycles}");
    }
}

#[test]
fn finite_loops_report_leftover_time() {
    let mut tween = Tween::new(0.0_f32, 1.0, ms(10))
        .with_mode(TweenMode::Loop)
        .with_cycles(3)
        .with_delay(ms(2));
    assert_eq!(tween.step(ms(45)), Some(ms(13)));
    assert_eq!(tween.value(), 1.0);
    assert!(tween.is_done());
    let mut once = Tween::new(0.0_f32, 1.0, ms(10));
    assert_eq!(once.step(ms(25)), Some(ms(15)));
}

#[test]
fn long_running_loops_do_not_drift() {
    let step = Duration::from_nanos(8_333_333); // 120 Hz
    let mut stepped = Tween::new(0.0_f32, 1.0, ms(700)).with_mode(TweenMode::PingPong);
    let mut whole = stepped;
    let ticks = 120 * 60 * 60 * 10; // ten simulated hours
    for _ in 0..ticks {
        stepped.advance(step);
    }
    whole.advance(step * ticks);
    assert_eq!(stepped, whole);
    let total = step.as_nanos() * u128::from(ticks);
    assert_eq!(
        stepped.completed_cycles() as u128,
        total / ms(700).as_nanos()
    );
    assert_eq!(
        stepped.progress(),
        ((total % ms(700).as_nanos()) as f64 / ms(700).as_nanos() as f64) as f32
    );
}

#[test]
fn pause_discards_time_and_cancel_freezes_until_reset() {
    let mut tween = Tween::new(0.0_f32, 100.0, ms(100));
    tween.advance(ms(30));
    tween.pause();
    assert!(tween.is_paused());
    assert!(tween.advance(ms(1000)).is_none());
    assert!((tween.value() - 30.0).abs() < 1e-4);
    tween.resume();
    tween.advance(ms(10));
    assert!((tween.value() - 40.0).abs() < 1e-4);

    tween.cancel();
    assert!(tween.is_cancelled() && !tween.is_finished());
    assert!(tween.advance(ms(1000)).is_none());
    assert!(tween.finish().is_none());
    assert!((tween.value() - 40.0).abs() < 1e-4);

    tween.pause();
    tween.reset();
    assert!(tween.is_paused() && !tween.is_cancelled());
    assert_eq!(tween.value(), 0.0);
    // Explicit completion works while paused and is reported once.
    assert_eq!(tween.finish(), Some(TweenCompleted { id: TweenId(0) }));
    assert_eq!(tween.value(), 100.0);
    assert!(tween.finish().is_none());
    tween.cancel();
    assert!(tween.is_finished());
}

#[test]
fn retarget_starts_from_the_current_value() {
    let mut slide = Tween::new(Vec2::ZERO, Vec2::new(100.0, 0.0), ms(100)).with_id(TweenId(4));
    slide.advance(ms(25));
    slide.retarget(Vec2::new(0.0, 50.0));
    assert_eq!(slide.from(), Vec2::new(25.0, 0.0));
    assert_eq!(slide.to(), Vec2::new(0.0, 50.0));
    assert_eq!(slide.value(), Vec2::new(25.0, 0.0));
    assert_eq!(slide.duration(), ms(100));
    assert_eq!(slide.id(), TweenId(4));
    slide.advance(ms(100));
    assert_eq!(slide.value(), Vec2::new(0.0, 50.0));
}

#[test]
fn completions_flow_through_typed_events() {
    let mut events = Events::with_capacity(4);
    let mut tweens = [
        Tween::new(0.0_f32, 1.0, ms(10)).with_id(TweenId(1)),
        Tween::new(0.0_f32, 1.0, ms(30)).with_id(TweenId(2)),
    ];
    for _ in 0..10 {
        for tween in &mut tweens {
            if let Some(done) = tween.advance(ms(5)) {
                events.send(done);
            }
        }
    }
    let ids: Vec<_> = events.drain().map(|done| done.id).collect();
    assert_eq!(ids, [TweenId(1), TweenId(2)]);
}

#[test]
fn sequences_carry_leftover_time_across_members() {
    let mut sequence = Sequence::new([
        Tween::new(0.0_f32, 10.0, ms(10)),
        Tween::new(10.0_f32, 20.0, ms(20)).with_delay(ms(5)),
        Tween::new(20.0_f32, 0.0, Duration::ZERO),
    ])
    .with_id(TweenId(9));
    assert_eq!(sequence.value(), Some(0.0));
    assert!(sequence.advance(ms(25)).is_none());
    assert_eq!(sequence.current(), 1);
    assert_eq!(sequence.value(), Some(15.0));
    // Partitioning cannot shift later members.
    let mut whole = sequence.clone();
    assert_eq!(whole.step(ms(100)), Some(ms(90)));
    for _ in 0..9 {
        assert!(sequence.advance(ms(1)).is_none());
    }
    assert_eq!(
        sequence.advance(ms(1)),
        Some(TweenCompleted { id: TweenId(9) })
    );
    assert_eq!(sequence.current(), 3);
    assert_eq!(sequence.value(), Some(0.0));
    assert!(sequence.advance(ms(1)).is_none());
    sequence.reset();
    assert_eq!(sequence.current(), 0);
    assert_eq!(sequence.value(), Some(0.0));
    assert!(!sequence.tracks()[0].is_finished());
}

#[test]
fn sequence_controls_and_member_states() {
    let mut empty = Sequence::new(Vec::<Tween<f32>>::new());
    assert_eq!(empty.value(), None);
    assert!(empty.advance(Duration::ZERO).is_some());

    let mut sequence = Sequence::new(vec![
        Tween::new(0.0_f32, 1.0, ms(10)),
        Tween::new(1.0_f32, 2.0, ms(10)),
        Tween::new(2.0_f32, 3.0, ms(10)),
    ]);
    sequence.tracks_mut()[1].cancel();
    assert!(sequence.advance(ms(15)).is_none());
    assert_eq!(sequence.current(), 2);
    assert_eq!(sequence.value(), Some(2.5));
    sequence.pause();
    assert!(sequence.advance(ms(100)).is_none());
    sequence.resume();
    // A paused member holds the whole sequence.
    sequence.tracks_mut()[2].pause();
    assert!(sequence.advance(ms(100)).is_none());
    assert_eq!(sequence.value(), Some(2.5));
    sequence.tracks_mut()[2].resume();
    sequence.cancel();
    assert!(sequence.is_cancelled());
    assert!(sequence.advance(ms(100)).is_none());
    sequence.reset();
    assert!(sequence.finish().is_some());
    assert!(sequence.is_finished());
    assert!(sequence.tracks().iter().all(Tween::is_finished));
    assert_eq!(sequence.value(), Some(3.0));
}

#[test]
fn parallel_groups_mix_types_and_end_with_the_longest_member() {
    let mut group = Parallel::new((
        Tween::new(Vec2::ZERO, Vec2::ONE, ms(10)),
        Tween::new([0, 0, 0, 255], [255, 255, 255, 255], ms(30)),
        Tween::new(1.0_f32, 0.0, ms(5)).with_delay(ms(10)),
    ))
    .with_id(TweenId(2));
    assert!(group.advance(ms(12)).is_none());
    assert_eq!(group.tracks().0.value(), Vec2::ONE);
    assert_eq!(group.tracks().2.value(), 0.6);
    assert_eq!(group.step(ms(25)), Some(ms(7)));
    assert!(group.is_finished());
    assert_eq!(group.tracks().1.value(), [255; 4]);
    assert!(group.advance(ms(1)).is_none());
    group.reset();
    assert_eq!(group.tracks().1.value(), [0, 0, 0, 255]);
    // Cancelled members count as done; paused members hold the group.
    group.tracks_mut().1.cancel();
    group.tracks_mut().2.pause();
    assert!(group.advance(ms(100)).is_none());
    group.tracks_mut().2.resume();
    assert!(group.advance(Duration::ZERO).is_none());
    assert!(group.advance(ms(15)).is_some());

    let mut empty = Parallel::new(Vec::<Tween<f32>>::new());
    assert_eq!(empty.step(ms(3)), Some(ms(3)));
}

#[test]
fn groups_nest_and_skip_to_end_recursively() {
    let mut nested = Sequence::new((
        Parallel::new([
            Tween::new(0.0_f32, 1.0, ms(10)),
            Tween::new(0.0_f32, 1.0, ms(20)),
        ]),
        Tween::new(Vec3::ZERO, Vec3::X, ms(10)),
    ));
    assert!(nested.advance(ms(25)).is_none());
    assert_eq!(nested.current(), 1);
    assert_eq!(nested.tracks().1.value(), Vec3::new(0.5, 0.0, 0.0));
    nested.reset();
    assert_eq!(nested.tracks().0.tracks()[1].value(), 0.0);
    assert!(nested.finish().is_some());
    assert_eq!(nested.tracks().0.tracks()[1].value(), 1.0);
    assert_eq!(nested.tracks().1.value(), Vec3::X);
    assert_eq!(nested.tracks().len(), 2);
    assert!(!nested.tracks().is_empty());
}

#[test]
#[should_panic(expected = "track index 2 out of range for 2 tracks")]
fn tuple_tracks_reject_out_of_range_indices() {
    let tracks = (
        Tween::new(0.0_f32, 1.0, ms(1)),
        Tween::new(0.0_f32, 1.0, ms(1)),
    );
    tracks.track(2);
}

#[test]
fn shake_is_seeded_smooth_bounded_and_decays_to_exact_zero() {
    let config = ShakeConfig {
        max_offset: Vec2::new(4.0, 2.0),
        max_roll: 0.1,
        frequency: 10.0,
        decay: 1.0,
        seed: 7,
    };
    let mut a = Shake::new(config).unwrap();
    let mut b = Shake::new(config).unwrap();
    assert_eq!(a.sample(), ShakeSample::default());
    a.add_trauma(0.5);
    a.add_trauma(f32::NAN);
    a.add_trauma(0.7);
    assert_eq!(a.trauma(), 1.0);
    b.set_trauma(1.0);
    let mut previous = a.sample();
    for _ in 0..200 {
        a.advance(ms(1));
        b.advance(ms(1));
        let sample = a.sample();
        assert_eq!(sample, b.sample());
        assert!(sample.offset.x.abs() <= 4.0 * a.intensity());
        assert!(sample.offset.y.abs() <= 2.0 * a.intensity());
        assert!(sample.roll.abs() <= 0.1 * a.intensity());
        // Smooth noise moves only a little per millisecond.
        assert!((sample.offset - previous.offset).length() < 0.2);
        previous = sample;
    }
    assert!((a.trauma() - 0.8).abs() < 1e-5);
    assert!((a.intensity() - 0.64).abs() < 1e-4);
    let mut other = Shake::new(ShakeConfig { seed: 8, ..config }).unwrap();
    other.set_trauma(1.0);
    other.advance(ms(200));
    assert_ne!(other.sample(), b.sample());
    a.advance(Duration::MAX);
    assert_eq!(a.trauma(), 0.0);
    assert_eq!(a.sample(), ShakeSample::default());
}

#[test]
fn shake_advancement_is_partition_independent_over_long_runs() {
    let mut stepped = Shake::new(ShakeConfig {
        decay: 0.0,
        ..ShakeConfig::default()
    })
    .unwrap();
    stepped.set_trauma(1.0);
    let mut whole = stepped.clone();
    let step = Duration::from_nanos(8_333_333);
    let ticks = 120 * 60 * 60; // one simulated hour
    for _ in 0..ticks {
        stepped.advance(step);
    }
    whole.advance(step * ticks);
    assert!((stepped.sample().offset - whole.sample().offset).length() < 1e-3);
    assert!((stepped.sample().roll - whole.sample().roll).abs() < 1e-5);
}

#[test]
fn shake_moves_2d_and_3d_cameras_along_their_view_axes() {
    let mut shake = Shake::new(ShakeConfig::default()).unwrap();
    let camera = Camera2D {
        rotation: 0.4,
        ..Camera2D::default()
    };
    let eye = Camera3D::default();
    assert_eq!(shake.apply_2d(camera).target, camera.target);
    assert_eq!(shake.apply_3d(eye).position, eye.position);
    shake.set_trauma(1.0);
    shake.advance(ms(37));
    let sample = shake.sample();
    let shaken = shake.apply_2d(camera);
    assert!((shaken.rotation - camera.rotation - sample.roll).abs() < 1e-6);
    // The displacement is the sample in the camera's rotated screen axes.
    let screen = glam::Mat2::from_angle(camera.rotation) * (shaken.target - camera.target);
    assert!(screen.abs_diff_eq(sample.offset, 1e-4));

    let moved = shake.apply_3d(eye);
    let offset = moved.position - eye.position;
    assert!(offset.abs_diff_eq(moved.target - eye.target, 1e-5));
    let forward = (eye.target - eye.position).normalize();
    assert!(offset.dot(forward).abs() < 1e-4);
    assert!((offset.length() - sample.offset.length()).abs() < 1e-4);
    assert!(
        moved
            .up
            .abs_diff_eq(Quat::from_axis_angle(forward, sample.roll) * eye.up, 1e-6)
    );
    let degenerate = Camera3D {
        target: eye.position,
        ..eye
    };
    assert_eq!(shake.apply_3d(degenerate).position, degenerate.position);
}

#[test]
fn shake_rejects_invalid_configuration() {
    let base = ShakeConfig::default();
    for config in [
        ShakeConfig {
            max_offset: Vec2::new(-1.0, 0.0),
            ..base
        },
        ShakeConfig {
            max_roll: f32::NAN,
            ..base
        },
        ShakeConfig {
            frequency: 0.0,
            ..base
        },
        ShakeConfig {
            decay: f32::INFINITY,
            ..base
        },
    ] {
        assert!(Shake::new(config).is_err());
    }
    let mut shake = Shake::new(base).unwrap();
    assert!(
        shake
            .set_config(ShakeConfig {
                frequency: -1.0,
                ..base
            })
            .is_err()
    );
    assert_eq!(shake.config(), &base);
    assert!(shake.set_config(ShakeConfig { seed: 3, ..base }).is_ok());
    assert_eq!(ShakeError("x").to_string(), "x",);
}

#[test]
fn active_tweens_reuse_their_storage() {
    fn copyable<T: Copy>(_: &T) {}
    let tween = Tween::new(Vec3::ZERO, Vec3::ONE, ms(10)).with_mode(TweenMode::Loop);
    copyable(&tween);
    let mut group = Parallel::new(vec![tween; 64]);
    let mut sequence = Sequence::new(vec![Tween::new(0.0_f32, 1.0, ms(10)); 64]);
    let storage = (group.tracks().as_ptr(), group.tracks().capacity());
    let ordered = (sequence.tracks().as_ptr(), sequence.tracks().capacity());
    for frame in 0..10_000 {
        group.advance(Duration::from_micros(16_667));
        sequence.advance(ms(1));
        if frame % 1_000 == 0 {
            group.reset();
            sequence.reset();
        }
    }
    assert_eq!(
        storage,
        (group.tracks().as_ptr(), group.tracks().capacity())
    );
    assert_eq!(
        ordered,
        (sequence.tracks().as_ptr(), sequence.tracks().capacity())
    );
}
