//! CPU triangle data for procedural geometry, independent of a graphics context.

use glam::{Vec2, Vec3};
use std::fmt;

/// Largest vertex count accepted by raylib's 16-bit indexed mesh builder.
pub const MAX_INDEXED_VERTICES: usize = u16::MAX as usize;
/// Largest vertex count whose position buffer fits raylib's signed byte count.
pub const MAX_MESH_VERTICES: usize = i32::MAX as usize / size_of::<[f32; 3]>();

/// Owned CPU mesh data. Safe to build on a worker and send to the render thread.
///
/// Triangles use counterclockwise winding when viewed from the front. With no
/// indices, each consecutive three positions form a triangle. Optional vertex
/// attributes must contain one element per position. Missing UVs use zero;
/// missing colors use white. Normals are optional and are not generated.
#[derive(Clone, Debug, Default)]
pub struct MeshData {
    /// Local-space vertex positions.
    pub positions: Vec<Vec3>,
    /// Optional local-space normals, usually unit vectors.
    pub normals: Option<Vec<Vec3>>,
    /// Optional texture coordinates. Values outside 0..1 are allowed.
    pub texcoords: Option<Vec<Vec2>>,
    /// Optional RGBA vertex colors, with components in 0..=255.
    pub colors: Option<Vec<[u8; 4]>>,
    /// Optional triangle indices, three per triangle.
    pub indices: Option<Vec<u16>>,
}

/// Counts returned by successful mesh validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeshInfo {
    /// Number of vertices, including unused indexed vertices.
    pub vertex_count: usize,
    /// Number of complete triangles.
    pub triangle_count: usize,
}

/// Vertex attribute referenced by a validation error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshAttribute {
    /// Local positions.
    Positions,
    /// Vertex normals.
    Normals,
    /// Texture coordinates.
    Texcoords,
    /// RGBA colors.
    Colors,
}

/// Invalid mesh data, detected before any GPU allocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeshError {
    /// Positions or the selected triangle stream are empty.
    Empty,
    /// A vertex or index buffer exceeds the backend's supported count.
    TooLarge,
    /// The vertex/index stream is not a multiple of three.
    IncompleteTriangle,
    /// An optional attribute has a different length from the positions.
    AttributeLength {
        /// Attribute with the wrong length.
        attribute: MeshAttribute,
        /// Required length.
        expected: usize,
        /// Supplied length.
        actual: usize,
    },
    /// A position, normal, or UV contains NaN or infinity.
    NonFinite {
        /// Attribute containing the invalid value.
        attribute: MeshAttribute,
        /// Vertex offset in that attribute.
        vertex: usize,
    },
    /// A triangle index references a missing vertex.
    IndexOutOfBounds {
        /// Offset in the index stream.
        offset: usize,
        /// Referenced vertex.
        index: u16,
        /// Number of available vertices.
        vertex_count: usize,
    },
}

impl fmt::Display for MeshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "mesh must contain vertices and at least one triangle"),
            Self::TooLarge => write!(
                f,
                "mesh exceeds backend limits; split it into smaller meshes"
            ),
            Self::IncompleteTriangle => write!(f, "triangle stream length must be a multiple of 3"),
            Self::AttributeLength {
                attribute,
                expected,
                actual,
            } => write!(
                f,
                "{attribute:?} has {actual} elements; expected {expected}"
            ),
            Self::NonFinite { attribute, vertex } => {
                write!(f, "{attribute:?} vertex {vertex} contains NaN or infinity")
            }
            Self::IndexOutOfBounds {
                offset,
                index,
                vertex_count,
            } => write!(
                f,
                "index {offset} references vertex {index}; only {vertex_count} vertices exist"
            ),
        }
    }
}

impl std::error::Error for MeshError {}

impl MeshData {
    /// Creates an unindexed mesh with no optional attributes.
    pub fn new(positions: Vec<Vec3>) -> Self {
        Self {
            positions,
            ..Self::default()
        }
    }

    /// Checks counts, attribute lengths, finite values, and index bounds.
    ///
    /// This performs no allocation and needs no display. Degenerate triangles,
    /// overlapping triangles, and non-unit normals are permitted.
    pub fn validate(&self) -> Result<MeshInfo, MeshError> {
        let vertex_count = self.positions.len();
        let stream_len = self.indices.as_ref().map_or(vertex_count, Vec::len);
        validate_counts(vertex_count, stream_len, self.indices.is_some())?;
        for (attribute, actual) in [
            (MeshAttribute::Normals, self.normals.as_ref().map(Vec::len)),
            (
                MeshAttribute::Texcoords,
                self.texcoords.as_ref().map(Vec::len),
            ),
            (MeshAttribute::Colors, self.colors.as_ref().map(Vec::len)),
        ] {
            if let Some(actual) = actual.filter(|&len| len != vertex_count) {
                return Err(MeshError::AttributeLength {
                    attribute,
                    expected: vertex_count,
                    actual,
                });
            }
        }
        check_finite(
            MeshAttribute::Positions,
            self.positions.iter().map(|v| v.is_finite()),
        )?;
        if let Some(normals) = &self.normals {
            check_finite(
                MeshAttribute::Normals,
                normals.iter().map(|v| v.is_finite()),
            )?;
        }
        if let Some(texcoords) = &self.texcoords {
            check_finite(
                MeshAttribute::Texcoords,
                texcoords.iter().map(|v| v.is_finite()),
            )?;
        }
        if let Some(indices) = &self.indices
            && let Some((offset, &index)) = indices
                .iter()
                .enumerate()
                .find(|(_, i)| usize::from(**i) >= vertex_count)
        {
            return Err(MeshError::IndexOutOfBounds {
                offset,
                index,
                vertex_count,
            });
        }
        Ok(MeshInfo {
            vertex_count,
            triangle_count: stream_len / 3,
        })
    }
}

fn validate_counts(vertices: usize, stream: usize, indexed: bool) -> Result<(), MeshError> {
    if vertices == 0 || stream == 0 {
        return Err(MeshError::Empty);
    }
    if vertices > MAX_MESH_VERTICES
        || (indexed
            && (vertices > MAX_INDEXED_VERTICES || stream > i32::MAX as usize / size_of::<u16>()))
    {
        return Err(MeshError::TooLarge);
    }
    if !stream.is_multiple_of(3) {
        return Err(MeshError::IncompleteTriangle);
    }
    Ok(())
}

fn check_finite(
    attribute: MeshAttribute,
    mut values: impl Iterator<Item = bool>,
) -> Result<(), MeshError> {
    if let Some(vertex) = values.position(|finite| !finite) {
        Err(MeshError::NonFinite { attribute, vertex })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle() -> MeshData {
        MeshData::new(vec![Vec3::ZERO, Vec3::X, Vec3::Y])
    }

    #[test]
    fn indexed_quad_and_unindexed_triangle() {
        assert_eq!(
            triangle().validate().unwrap(),
            MeshInfo {
                vertex_count: 3,
                triangle_count: 1
            }
        );
        let mesh = MeshData {
            positions: vec![Vec3::ZERO, Vec3::X, Vec3::ONE, Vec3::Y],
            indices: Some(vec![0, 1, 2, 0, 2, 3]),
            normals: Some(vec![Vec3::Z; 4]),
            texcoords: Some(vec![Vec2::splat(2.0); 4]),
            colors: Some(vec![[255; 4]; 4]),
        };
        assert_eq!(
            mesh.validate().unwrap(),
            MeshInfo {
                vertex_count: 4,
                triangle_count: 2
            }
        );
    }

    #[test]
    fn rejects_empty_and_incomplete_streams() {
        assert_eq!(MeshData::default().validate(), Err(MeshError::Empty));
        let mut mesh = triangle();
        mesh.positions.pop();
        assert_eq!(mesh.validate(), Err(MeshError::IncompleteTriangle));
        mesh.indices = Some(vec![]);
        assert_eq!(mesh.validate(), Err(MeshError::Empty));
        mesh.indices = Some(vec![0, 1]);
        assert_eq!(mesh.validate(), Err(MeshError::IncompleteTriangle));
    }

    #[test]
    fn checks_optional_lengths_and_all_float_channels() {
        for attribute in [
            MeshAttribute::Normals,
            MeshAttribute::Texcoords,
            MeshAttribute::Colors,
        ] {
            let mut mesh = triangle();
            match attribute {
                MeshAttribute::Normals => mesh.normals = Some(vec![]),
                MeshAttribute::Texcoords => mesh.texcoords = Some(vec![Vec2::ZERO]),
                MeshAttribute::Colors => mesh.colors = Some(vec![[0; 4]; 4]),
                MeshAttribute::Positions => unreachable!(),
            }
            assert!(
                matches!(mesh.validate(), Err(MeshError::AttributeLength { attribute: a, expected: 3, .. }) if a == attribute)
            );
        }
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for attribute in [
                MeshAttribute::Positions,
                MeshAttribute::Normals,
                MeshAttribute::Texcoords,
            ] {
                let mut mesh = triangle();
                match attribute {
                    MeshAttribute::Positions => mesh.positions[1].x = invalid,
                    MeshAttribute::Normals => {
                        mesh.normals = Some(vec![Vec3::ZERO, Vec3::splat(invalid), Vec3::ZERO])
                    }
                    MeshAttribute::Texcoords => {
                        mesh.texcoords = Some(vec![Vec2::ZERO, Vec2::splat(invalid), Vec2::ZERO])
                    }
                    MeshAttribute::Colors => unreachable!(),
                }
                assert_eq!(
                    mesh.validate(),
                    Err(MeshError::NonFinite {
                        attribute,
                        vertex: 1
                    })
                );
            }
        }
    }

    #[test]
    fn validates_index_bounds_and_backend_count_limits() {
        let mut mesh = triangle();
        mesh.indices = Some(vec![0, 1, 3]);
        assert_eq!(
            mesh.validate(),
            Err(MeshError::IndexOutOfBounds {
                offset: 2,
                index: 3,
                vertex_count: 3
            })
        );
        mesh.positions = vec![Vec3::ZERO; MAX_INDEXED_VERTICES];
        mesh.indices = Some(vec![0, 1, u16::MAX - 1]);
        assert!(mesh.validate().is_ok());
        mesh.positions.push(Vec3::ZERO);
        assert_eq!(mesh.validate(), Err(MeshError::TooLarge));
        assert_eq!(
            validate_counts(MAX_MESH_VERTICES + 1, 3, false),
            Err(MeshError::TooLarge)
        );
        assert_eq!(
            validate_counts(3, i32::MAX as usize / 2 + 1, true),
            Err(MeshError::TooLarge)
        );
        assert!(validate_counts(MAX_MESH_VERTICES, 3, false).is_ok());
    }

    #[test]
    fn mesh_data_can_cross_threads() {
        let info = std::thread::spawn(|| triangle().validate())
            .join()
            .unwrap()
            .unwrap();
        assert_eq!(info.triangle_count, 1);
    }
}
