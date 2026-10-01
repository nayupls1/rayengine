//! Original progressive crack geometry, cached once and shared across targets.
use rayengine_core::{
    glam::{Vec2, Vec3},
    mesh::MeshData,
};
use rayengine_voxel::Face;
/// Five increasingly fractured stages, with no per-frame uploads.
pub const STAGES: usize = 5;
/// Zero/nonfinite progress has no overlay. Positive progress selects one cached stage.
pub fn stage(progress: f32) -> Option<usize> {
    if !progress.is_finite() || progress <= 0.0 {
        None
    } else {
        Some(((progress.min(1.0) * STAGES as f32).ceil() as usize - 1).min(STAGES - 1))
    }
}
/// Thin opaque strips just outside all six cube faces, leaving texture visible
/// between cracks. Vertex winding faces outward; no transparent sorting is needed.
/// The stage index is in 0..STAGES. Positions are relative to the target cell.
pub fn mesh(stage: usize) -> MeshData {
    assert!(stage < STAGES);
    let mut mesh = MeshData {
        indices: Some(Vec::new()),
        ..Default::default()
    };
    // Eight irregular branches grow from a shared center, including side splits.
    let ends = [
        Vec2::new(0.05, 0.1),
        Vec2::new(0.45, 0.03),
        Vec2::new(0.95, 0.14),
        Vec2::new(0.98, 0.52),
        Vec2::new(0.91, 0.94),
        Vec2::new(0.54, 0.98),
        Vec2::new(0.07, 0.9),
        Vec2::new(0.03, 0.47),
    ];
    for face in Face::ALL {
        for (arm, end) in ends.into_iter().enumerate() {
            let center = Vec2::new(0.48, 0.51);
            let dir = end - center;
            let side = Vec2::new(-dir.y, dir.x).normalize();
            let mut previous = center;
            for segment in 1..=stage + 1 {
                let fraction = segment as f32 / STAGES as f32;
                let jitter = if segment == STAGES {
                    0.0
                } else if (arm + segment) % 2 == 0 {
                    0.035
                } else {
                    -0.025
                };
                let next = center + dir * fraction + side * jitter;
                strip(
                    &mut mesh,
                    face,
                    previous,
                    next,
                    0.006 + stage as f32 * 0.0015,
                );
                if segment >= 3 && arm % 2 == 0 {
                    strip(
                        &mut mesh,
                        face,
                        previous,
                        previous + side * 0.09 + dir * 0.06,
                        0.005,
                    );
                }
                previous = next;
            }
        }
    }
    mesh
}
fn point(face: Face, uv: Vec2) -> Vec3 {
    match face {
        Face::NegX => Vec3::new(-0.003, uv.y, uv.x),
        Face::PosX => Vec3::new(1.003, uv.y, 1.0 - uv.x),
        Face::NegY => Vec3::new(uv.x, -0.003, uv.y),
        Face::PosY => Vec3::new(uv.x, 1.003, 1.0 - uv.y),
        Face::NegZ => Vec3::new(1.0 - uv.x, uv.y, -0.003),
        Face::PosZ => Vec3::new(uv.x, uv.y, 1.003),
    }
}
fn strip(mesh: &mut MeshData, face: Face, a: Vec2, b: Vec2, width: f32) {
    let d = (b - a).normalize();
    let side = Vec2::new(-d.y, d.x) * width;
    let base = mesh.positions.len() as u16;
    mesh.positions.extend([
        point(face, a - side),
        point(face, b - side),
        point(face, b + side),
        point(face, a + side),
    ]);
    mesh.indices
        .as_mut()
        .unwrap()
        .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stages_grow_and_geometry_is_outward_and_offsets_prevent_depth_fighting() {
        assert_eq!(stage(0.0), None);
        assert_eq!(stage(f32::NAN), None);
        assert_eq!(stage(0.001), Some(0));
        assert_eq!(stage(0.5), Some(2));
        assert_eq!(stage(1.0), Some(4));
        let mut count = 0;
        for s in 0..STAGES {
            let mesh = mesh(s);
            let info = mesh.validate().unwrap();
            assert!(info.vertex_count > count);
            count = info.vertex_count;
            for p in mesh.positions.as_chunks::<4>().0 {
                let normal = (p[1] - p[0]).cross(p[2] - p[0]);
                let center = (p[0] + p[1] + p[2] + p[3]) * 0.25;
                assert!(normal.dot(center - Vec3::splat(0.5)) > 0.0);
                assert!(center.min_element() < 0.0 || center.max_element() > 1.0);
            }
        }
    }
}
