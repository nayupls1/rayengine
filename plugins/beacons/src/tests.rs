use super::*;
use rayengine::diagnostics::DiagnosticsConfig;
use std::{cell::Cell, rc::Rc};

#[test]
fn cpu_step_touches_only_owned_entity_and_interpolates_across_both_wrap_directions() {
    for speed in [-1.0, 1.0] {
        let mut world = BeaconWorld::default();
        let mut beacon = Beacon::new(Vec3::ZERO, Color::WHITE, speed).unwrap();
        // CPU fixtures substitute an entity without creating a graphics context.
        let owned = world.scene.spawn_3d(Transform3D::default(), ());
        let other = world.scene.spawn_3d(Transform3D::at(Vec3::X), ());
        beacon.entity = Some(owned);
        beacon.angle = if speed < 0.0 { 0.01 } else { TAU - 0.01 };
        beacon.step(&mut world, 0.02);
        assert!((beacon.angle - beacon.previous_angle - speed * 0.02).abs() < 1e-6);
        let middle = Quat::from_rotation_y((beacon.angle + beacon.previous_angle) * 0.5);
        assert!(
            middle.abs_diff_eq(Quat::IDENTITY, 1e-6) || middle.abs_diff_eq(-Quat::IDENTITY, 1e-6)
        );
        assert_eq!(
            world
                .scene
                .world
                .get::<&Transform3D>(other)
                .unwrap()
                .rotation,
            Quat::IDENTITY
        );
        assert_eq!(
            world
                .scene
                .world
                .get::<&Transform3D>(other)
                .unwrap()
                .position,
            Vec3::X
        );
        let angle = beacon.angle;
        beacon.step(&mut world, 0.0);
        assert_eq!(beacon.angle, angle);
        assert_eq!(beacon.previous_angle, angle);
        world.scene.despawn(owned).unwrap();
        beacon.step(&mut world, 0.1); // External entity removal is tolerated.
    }
}

#[test]
fn configuration_and_extreme_finite_steps_are_bounded() {
    assert!(Beacon::new(Vec3::splat(f32::NAN), Color::WHITE, 1.0).is_err());
    assert!(Beacon::new(Vec3::ZERO, Color::WHITE, f32::INFINITY).is_err());
    let mut world = BeaconWorld::default();
    for speed in [-f32::MAX, f32::MAX] {
        let mut beacon = Beacon::new(Vec3::ZERO, Color::WHITE, speed).unwrap();
        beacon.entity = Some(world.scene.spawn_3d(Transform3D::default(), ()));
        beacon.step(&mut world, f32::MAX);
        assert!(beacon.angle.is_finite());
        assert!(beacon.previous_angle.is_finite());
        assert!((0.0..TAU).contains(&beacon.angle));
        assert!(
            world
                .scene
                .world
                .get::<&Transform3D>(beacon.entity.unwrap())
                .unwrap()
                .rotation
                .is_finite()
        );
    }
    assert_eq!(geometry().validate().unwrap().triangle_count, 8);
}

#[test]
#[ignore = "requires native OpenGL; scripts/native_smoke.sh runs serially"]
fn native_plugin_composition_unload_reinit_and_error_propagation() {
    struct Probe {
        world: BeaconWorld,
        a: Beacon,
        b: Beacon,
        updates: Rc<Cell<u64>>,
    }
    impl Game for Probe {
        fn init(&mut self, context: &mut InitContext<'_, '_>) -> Result<(), Error> {
            self.a.init(&mut self.world, context)?;
            self.b.init(&mut self.world, context)?;
            assert_eq!(context.assets.resource_counts().meshes, 2);
            let old_a = self.a.mesh().unwrap();
            let b = self.b.mesh().unwrap();
            let b_entity = self.b.entity().unwrap();
            assert!(self.a.init(&mut self.world, context).is_err());
            assert_eq!(context.assets.resource_counts().meshes, 2);
            self.a.unload(&mut self.world, context.assets);
            self.a.unload(&mut self.world, context.assets);
            assert!(context.assets.mesh(old_a).is_none());
            assert!(context.assets.mesh(b).is_some());
            assert!(self.world.scene.world.contains(b_entity));
            assert_eq!(self.world.scene.world.len(), 1);
            self.a.init(&mut self.world, context)?;
            assert_ne!(self.a.mesh().unwrap(), old_a);
            assert_eq!(self.world.scene.world.len(), 2);
            assert_eq!(context.assets.resource_counts().meshes, 2);
            Ok(())
        }
        fn fixed_update(&mut self, context: &mut Update<'_, '_>) {
            self.a.fixed_update(&mut self.world, context);
            self.b.fixed_update(&mut self.world, context);
            self.updates.set(self.updates.get() + 1);
        }
        fn draw(&mut self, frame: &mut Frame<'_, '_>) {
            frame.clear(Color::BLACK);
            self.a.draw(&self.world, frame);
            self.b.draw(&self.world, frame);
            assert_eq!(frame.draw_counters().unwrap().meshes, 2);
        }
    }
    let updates = Rc::new(Cell::new(0));
    let probe = Probe {
        world: BeaconWorld::default(),
        a: Beacon::new(Vec3::NEG_X, Color::SKYBLUE, 1.0).unwrap(),
        b: Beacon::new(Vec3::X, Color::ORANGE, -1.0).unwrap(),
        updates: updates.clone(),
    };
    let mut config = Config::new("Plugin lifecycle probe");
    config.audio = false;
    config.vsync = false;
    let report = App::new(config.clone())
        .with_options(RunOptions {
            frames: Some(4),
            hidden: true,
            diagnostics: Some(DiagnosticsConfig::new("plugins/beacons.v1")),
            ..Default::default()
        })
        .run(probe)
        .unwrap();
    assert_eq!(report.frames, 4);
    assert!(updates.get() > 0);

    struct Fails;
    impl Plugin<BeaconWorld> for Fails {
        fn init(&mut self, _: &mut BeaconWorld, _: &mut InitContext<'_, '_>) -> Result<(), Error> {
            Err(Error::Asset("expected plugin init failure".into()))
        }
    }
    struct FailingGame {
        world: BeaconWorld,
        first: Beacon,
        second: Fails,
        dropped: Rc<Cell<bool>>,
    }
    impl Drop for FailingGame {
        fn drop(&mut self) {
            self.dropped.set(true);
        }
    }
    impl Game for FailingGame {
        fn init(&mut self, context: &mut InitContext<'_, '_>) -> Result<(), Error> {
            self.first.init(&mut self.world, context)?;
            self.second.init(&mut self.world, context)?;
            panic!("failed plugin must stop initialization");
        }
        fn fixed_update(&mut self, _: &mut Update<'_, '_>) {
            panic!("failed init must not update");
        }
        fn draw(&mut self, _: &mut Frame<'_, '_>) {
            panic!("failed init must not draw");
        }
    }
    let dropped = Rc::new(Cell::new(false));
    let error = App::new(config)
        .with_options(RunOptions {
            frames: Some(1),
            hidden: true,
            ..Default::default()
        })
        .run(FailingGame {
            world: BeaconWorld::default(),
            first: Beacon::new(Vec3::ZERO, Color::WHITE, 0.0).unwrap(),
            second: Fails,
            dropped: dropped.clone(),
        })
        .unwrap_err();
    assert!(matches!(error, Error::Asset(message) if message == "expected plugin init failure"));
    assert!(dropped.get());
}
