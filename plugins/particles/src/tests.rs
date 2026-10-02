use super::*;

#[test]
fn seeded_variation_reset_and_independent_emitters() {
    let config = EmitterConfig {
        seed: 42,
        position: Vec3::new(2.0, 3.0, 4.0),
        position_spread: Vec3::splat(1.0),
        velocity: Vec3::Y,
        velocity_spread: Vec3::splat(2.0),
        lifetime: [0.5, 2.0],
        ..Default::default()
    };
    let mut a = Emitter::new(config.clone()).unwrap();
    let mut b = Emitter::new(config.clone()).unwrap();
    a.burst(50);
    let original = a.particles().to_vec();
    b.burst(50);
    assert_eq!(a.particles(), b.particles());
    for particle in a.particles() {
        assert!((particle.position - config.position).abs().max_element() <= 1.0);
        assert!((particle.velocity - config.velocity).abs().max_element() <= 2.0);
        assert!((0.5..=2.0).contains(&particle.lifetime));
    }
    a.step(0.2).unwrap();
    assert_eq!(b.particles(), original);
    a.reset();
    a.burst(50);
    assert_eq!(a.particles(), original);
    let mut other = Emitter::new(EmitterConfig { seed: 43, ..config }).unwrap();
    other.burst(50);
    assert_ne!(other.particles(), original);
}

#[test]
fn motion_lifetime_interpolation_and_appearance_use_explicit_dt() {
    let mut emitter = Emitter::new(EmitterConfig {
        lifetime: [2.0, 2.0],
        velocity: Vec3::X * 2.0,
        acceleration: Vec3::Y * 4.0,
        start_color: Vec4::new(1.0, 0.0, 0.0, 1.0),
        end_color: Vec4::new(0.0, 1.0, 0.0, 0.0),
        start_size: 4.0,
        end_size: 0.0,
        ..Default::default()
    })
    .unwrap();
    emitter.burst(1);
    emitter.step(1.0).unwrap();
    let particle = emitter.particles()[0];
    assert_eq!(particle.position(), Vec3::new(2.0, 2.0, 0.0));
    assert_eq!(particle.velocity(), Vec3::new(2.0, 4.0, 0.0));
    assert_eq!(
        particle.interpolated_position(0.5),
        Vec3::new(1.0, 1.0, 0.0)
    );
    assert_eq!(
        emitter.appearance(&particle, 0.5),
        Appearance {
            color: Vec4::new(0.75, 0.25, 0.0, 0.75),
            size: 3.0,
        }
    );
    emitter.step(0.0).unwrap();
    assert_eq!(
        emitter.particles()[0].interpolated_position(0.0),
        particle.position()
    );
    emitter.step(0.5).unwrap();
    assert_eq!(emitter.particles()[0].position(), Vec3::new(3.0, 4.5, 0.0));
    emitter.step(0.5).unwrap();
    assert!(emitter.is_empty());
}

#[test]
fn limits_drop_excess_without_backlog_and_never_grow_storage() {
    let mut emitter = Emitter::new(EmitterConfig {
        capacity: 7,
        max_spawn: 3,
        rate: 1000.0,
        lifetime: [0.5, 0.5],
        ..Default::default()
    })
    .unwrap();
    let pointer = emitter.particles.as_ptr();
    let storage = emitter.particles.capacity();
    assert_eq!(emitter.burst(usize::MAX), 3);
    assert_eq!(emitter.burst(usize::MAX), 3);
    assert_eq!(emitter.burst(usize::MAX), 1);
    assert_eq!(emitter.burst(usize::MAX), 0);
    for _ in 0..10_000 {
        assert!(emitter.step(1.0 / 60.0).unwrap() <= 3);
        assert!(emitter.len() <= 7);
        assert_eq!(emitter.particles.as_ptr(), pointer);
        assert_eq!(emitter.particles.capacity(), storage);
    }
    emitter.stop();
    assert_eq!(emitter.burst(10), 0);
    emitter.step(1.0).unwrap();
    assert!(emitter.is_empty());
    emitter.start();
    assert_eq!(emitter.step(0.0).unwrap(), 0);
    assert!(emitter.is_empty());
    assert_eq!(emitter.step(0.0001).unwrap(), 0); // no whole-birth backlog
    assert_eq!(emitter.step(0.001).unwrap(), 1);
    assert_eq!(emitter.particles()[0].age(), 0.0); // admitted at tick end
}

#[test]
fn fractional_emission_stop_reset_and_move_origin() {
    let mut emitter = Emitter::new(EmitterConfig {
        rate: 2.0,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(emitter.step(0.25).unwrap(), 0);
    emitter.stop();
    emitter.step(100.0).unwrap();
    emitter.start();
    emitter.set_position(Vec3::X).unwrap();
    assert_eq!(emitter.step(0.25).unwrap(), 1);
    emitter.set_position(Vec3::Y).unwrap();
    emitter.burst(1);
    assert_eq!(emitter.particles()[0].position(), Vec3::X);
    assert_eq!(emitter.particles()[1].position(), Vec3::Y);
    emitter.reset();
    assert_eq!(emitter.step(0.25).unwrap(), 0);
    assert_eq!(emitter.config().position, Vec3::Y);
}

#[test]
fn invalid_configuration_and_steps_are_rejected_transactionally() {
    let configs = [
        EmitterConfig {
            capacity: 0,
            ..Default::default()
        },
        EmitterConfig {
            capacity: MAX_PARTICLES + 1,
            ..Default::default()
        },
        EmitterConfig {
            max_spawn: 0,
            ..Default::default()
        },
        EmitterConfig {
            max_spawn: MAX_PARTICLES + 1,
            ..Default::default()
        },
        EmitterConfig {
            rate: f32::NAN,
            ..Default::default()
        },
        EmitterConfig {
            rate: -1.0,
            ..Default::default()
        },
        EmitterConfig {
            lifetime: [0.0, 1.0],
            ..Default::default()
        },
        EmitterConfig {
            lifetime: [2.0, 1.0],
            ..Default::default()
        },
        EmitterConfig {
            lifetime: [1.0, f32::INFINITY],
            ..Default::default()
        },
        EmitterConfig {
            position: Vec3::splat(f32::INFINITY),
            ..Default::default()
        },
        EmitterConfig {
            position_spread: Vec3::splat(-1.0),
            ..Default::default()
        },
        EmitterConfig {
            velocity: Vec3::splat(f32::MAX),
            velocity_spread: Vec3::splat(f32::MAX),
            ..Default::default()
        },
        EmitterConfig {
            acceleration: Vec3::splat(f32::NAN),
            ..Default::default()
        },
        EmitterConfig {
            start_color: Vec4::splat(1.1),
            ..Default::default()
        },
        EmitterConfig {
            end_color: Vec4::splat(-0.1),
            ..Default::default()
        },
        EmitterConfig {
            start_size: -1.0,
            ..Default::default()
        },
        EmitterConfig {
            end_size: f32::NAN,
            ..Default::default()
        },
    ];
    for config in configs {
        assert!(Emitter::new(config).is_err());
    }
    let mut emitter = Emitter::new(EmitterConfig::default()).unwrap();
    emitter.burst(5);
    let before = emitter.particles().to_vec();
    let rng = emitter.random;
    for dt in [-1.0, f32::NAN, f32::INFINITY] {
        assert!(emitter.step(dt).is_err());
        assert_eq!(emitter.particles(), before);
        assert_eq!(emitter.random, rng);
    }
    assert!(emitter.set_position(Vec3::splat(f32::NAN)).is_err());
    assert_eq!(emitter.config().position, Vec3::ZERO);
}

#[test]
fn extreme_finite_time_rate_and_motion_remain_bounded() {
    let mut emitter = Emitter::new(EmitterConfig {
        capacity: 4,
        max_spawn: 2,
        rate: f32::MAX,
        lifetime: [f32::MAX, f32::MAX],
        velocity: Vec3::splat(f32::MAX),
        acceleration: Vec3::splat(f32::MAX),
        ..Default::default()
    })
    .unwrap();
    emitter.burst(2);
    assert_eq!(emitter.step(2.0).unwrap(), 2); // old overflowing particles retire
    assert_eq!(emitter.len(), 2);
    assert_eq!(emitter.step(f32::MAX).unwrap(), 2); // old particles expire first
    assert!(emitter.remainder.is_finite());
    assert!(emitter.particles().iter().all(|p| p.position().is_finite()));
}

#[test]
fn lifetime_rounding_does_not_expire_on_zero_time_or_lose_small_ticks() {
    let mut emitter = Emitter::new(EmitterConfig::default()).unwrap();
    emitter.burst(1);
    emitter.step(0.5).unwrap();
    let dt = f32::from_bits(0.5_f32.to_bits() - 1);
    emitter.step(dt).unwrap();
    assert_eq!(emitter.len(), 1);
    let age = emitter.particles()[0].age();
    assert_eq!(age, 0.5 + f64::from(dt));
    assert!(age < 1.0);
    emitter.step(0.0).unwrap();
    assert_eq!(emitter.len(), 1);
    assert_eq!(emitter.particles()[0].age(), age);
    assert_eq!(emitter.particles()[0].previous_age, age);
    emitter.step(f32::EPSILON).unwrap();
    assert!(emitter.is_empty());

    let mut long = Emitter::new(EmitterConfig {
        lifetime: [1_000_000.0, 1_000_000.0],
        ..Default::default()
    })
    .unwrap();
    long.burst(1);
    long.step(100_000.0).unwrap();
    let before = long.particles()[0].age();
    let dt = 0.001;
    long.step(dt).unwrap();
    assert_eq!(long.particles()[0].age(), before + f64::from(dt));
    assert!(long.particles()[0].age() > before);
    assert_eq!(std::mem::size_of::<Particle>(), 56);
}
