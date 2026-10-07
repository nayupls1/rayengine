//! CPU-only example: a projectile checks walls before game-owned targets.
use rayengine_core::prelude::*;

fn main() -> Result<(), QueryError> {
    let mut world = PhysicsWorld2D::new(4.0);
    let mut target = PhysicsBody2D::new(Vec2::new(10.0, 0.0), Shape2D::round(1.0));
    target.is_trigger = true;
    let target = world.insert(target);
    let mut wall = PhysicsBody2D::new(
        Vec2::new(5.0, 0.0),
        Shape2D::box_shape(Vec2::new(0.1, 10.0)),
    );
    wall.kind = BodyKind::Static;
    let wall = world.insert(wall);
    let projectile_shape = Shape2D::round(0.25);
    let projectile = world.insert(PhysicsBody2D::new(Vec2::ZERO, projectile_shape));
    let excluded = [projectile];
    let filter = QueryFilter {
        excluded: &excluded,
        include_triggers: true,
        ..QueryFilter::default()
    };
    let hit = world
        .cast_shape(projectile_shape, Vec2::ZERO, Vec2::X * 100.0, filter)?
        .expect("wall blocks the projectile");
    assert_eq!(hit.body, wall);
    println!(
        "First collider: {:?}, center at {:?}, distance {}",
        hit.body, hit.hit.position, hit.hit.distance
    );

    world.remove(wall);
    let ray = Ray2::new(Vec2::ZERO, Vec2::X).expect("finite ray");
    assert_eq!(world.raycast(ray, 100.0, filter)?.unwrap().body, target);
    world.visit_overlaps(Shape2D::round(2.0), Vec2::X * 10.0, filter, |hit| {
        println!("Overlap: {:?}, depth {}", hit.body, hit.penetration.depth);
    })?;
    // Decide lifetime, damage and faction eligibility in game code.
    Ok(())
}
