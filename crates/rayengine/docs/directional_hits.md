# Directional hit geometry

`Sector2` is a validated, CPU-only static area in `rayengine::core::collision`
and both SDK preludes. It describes a closed sector using an origin, center
direction, range in world units, and **full opening angle in radians**. It has
no window, GPU, ECS, or physics-world dependency.

```rust
use rayengine::prelude::*;

let attack = Sector2::new(
    Vec2::ZERO, Vec2::X, 5.0, std::f32::consts::FRAC_PI_2,
).unwrap();
let target = Circle::new(Vec2::new(2.0, 2.5), 0.4);
assert!(!attack.contains(target.center).unwrap());
assert!(attack.intersects_circle(&target).unwrap());
```

The direction is normalized and the sector extends equally to each side.
Positive rotation from `+X` toward `+Y` appears clockwise on a Y-down screen
and counterclockwise in Y-up coordinates. Using a vector for the direction
avoids a discontinuity when facing crosses `-PI`/`PI`.
Angles from `0` through `TAU` are accepted, including wide sectors over `PI`.
Zero angle is a closed line segment, `PI` is a half disk, and `TAU` is a full
disk. Zero range reduces any sector to its origin. The origin, radial edges,
arc endpoints, and range boundary are included; a tangent circle intersects.
This contact convention differs from positive penetration in arcade physics.

Construction rejects nonfinite inputs, zero direction, negative range, angles
outside `0..=TAU`, and bounds that cannot be represented as finite `f32`
coordinates. Circle queries reject a nonfinite center or a nonpositive or
nonfinite radius, even if the caller changed `Circle`'s public fields after
construction. `SectorError` describes the invalid input. Private sector fields
preserve validation; create a new sector when its pose or size changes.

`intersects_circle` tests the distance from the circle center to the sector
interior, curved arc, or radial segments. It accounts for the entire target
radius, including centers outside the angle or range, without tessellating the
arc or approximating it by separately inflating range and angle. Calculations
use double precision internally; boundary results still follow floating-point
rounding and add no gameplay tolerance. Other target shapes are outside this API.

For a few targets, call `intersects_circle` in a loop. For many targets, index
each circle's **whole AABB**, query `SpatialIndex2D::overlapping(attack.bounds())`,
then run the exact circle test for each candidate. `bounds()` encloses the full
range disk, so it can return false positives, especially for narrow sectors.
It rounds outward and adds one representable step of padding to retain tangent
contacts with the index's positive-area overlap test. Circle AABBs supplied by
the caller must also conservatively enclose their circles. Rebuild the index
after targets move; it holds a snapshot rather than following live bodies.

The complete public-API example needs only the core crate:

```sh
cargo run --locked -p rayengine-core --example directional_hit
```

The example is included in `Sector2`'s checked API documentation. The dungeon
also uses the query against its circular enemy bodies, retaining game-owned
line-of-sight, damage, cooldown, and stagger rules.

A query describes the area **at one instant**. Testing a sector at the beginning
and end of a rotating swing does not detect everything it swept through between
those poses. Continuous rotational collision is outside this API. The dungeon
evaluates its attack once when it starts; its visible swing animation does not
turn that hit query into continuous collision. Games own active attack windows,
damage, line-of-sight and per-swing hit deduplication.
