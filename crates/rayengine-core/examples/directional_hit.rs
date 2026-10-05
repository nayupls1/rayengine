//! A headless directional hit query. Run with:
//! `cargo run -p rayengine-core --example directional_hit`.

use rayengine_core::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let attack = Sector2::new(Vec2::ZERO, Vec2::X, 5.0, std::f32::consts::FRAC_PI_2)?;
    let targets = [
        Circle::new(Vec2::new(2.0, 2.5), 0.4), // Center outside, radius crosses an edge.
        Circle::new(Vec2::new(6.0, 0.0), 1.0), // Touches the arc.
        Circle::new(Vec2::new(-3.0, 0.0), 0.5),
    ];
    let mut index = SpatialIndex2D::new();
    index.rebuild(targets.iter().enumerate().map(|(id, circle)| {
        (
            id,
            Aabb2::from_center(circle.center, Vec2::splat(circle.radius * 2.0)),
        )
    }))?;

    let mut hits = Vec::new();
    for id in index.overlapping(attack.bounds())? {
        if attack.intersects_circle(&targets[id])? {
            hits.push(id);
        }
    }
    hits.sort_unstable(); // Index traversal order is not input order.
    assert_eq!(hits, vec![0, 1]);
    assert!(!attack.contains(targets[0].center)?);
    println!("Static directional area hits targets {hits:?}");
    // The game decides whether to deal damage, require line-of-sight, or
    // ignore a target already hit during this swing.
    Ok(())
}
