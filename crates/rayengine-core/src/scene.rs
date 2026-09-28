//! Dense entities/components with an optional, validated transform hierarchy.

use crate::transform::*;
use glam::{Mat3, Mat4};
use hecs::{Bundle, Entity, World};
use std::{
    collections::{HashMap, HashSet},
    fmt,
    ops::Mul,
};

/// Invalid hierarchy edits or malformed manually inserted parent components.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HierarchyError {
    /// An entity no longer exists.
    MissingEntity(Entity),
    /// The parent chain contains a cycle.
    Cycle(Entity),
    /// Child and parent must have transforms of the same dimension.
    DimensionMismatch(Entity),
}

impl fmt::Display for HierarchyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingEntity(e) => write!(f, "missing scene entity {e:?}"),
            Self::Cycle(e) => write!(f, "cycle in scene hierarchy at {e:?}"),
            Self::DimensionMismatch(e) => write!(f, "incompatible or missing transform at {e:?}"),
        }
    }
}

impl std::error::Error for HierarchyError {}

struct Node<M> {
    local: M,
    parent: Option<Entity>,
    global: Option<M>,
    visiting: bool,
}

/// Scene container. Game-defined components remain ordinary Rust structs.
///
/// Scratch storage is reused between propagations. Parenting is optional and
/// global transforms are updated explicitly once after local transforms change.
///
/// ```
/// use rayengine_core::prelude::*;
/// struct Health(u32);
/// let mut scene = Scene::new();
/// let root = scene.spawn_2d(Transform2D::at(Vec2::new(10.0, 0.0)), ());
/// let child = scene.spawn_2d(Transform2D::at(Vec2::new(5.0, 0.0)), (Health(100),));
/// scene.set_parent(child, Some(root)).unwrap();
/// scene.propagate().unwrap();
/// let global = scene.world.get::<&GlobalTransform2D>(child).unwrap();
/// assert_eq!(global.0.transform_point2(Vec2::ZERO), Vec2::new(15.0, 0.0));
/// ```
#[derive(Default)]
pub struct Scene {
    /// Dense ECS storage. Direct hierarchy edits are checked by [`Self::propagate`].
    pub world: World,
    nodes_2d: HashMap<Entity, Node<Mat3>>,
    nodes_3d: HashMap<Entity, Node<Mat4>>,
    entities: Vec<Entity>,
    path: Vec<Entity>,
}

impl Scene {
    /// Creates an empty scene.
    pub fn new() -> Self {
        Self::default()
    }

    /// Spawns custom components and the local/global 2D transform pair.
    pub fn spawn_2d(&mut self, transform: Transform2D, components: impl Bundle) -> Entity {
        let entity = self.world.spawn(components);
        self.world
            .insert(entity, (transform, GlobalTransform2D(transform.matrix())))
            .expect("newly spawned entity exists");
        entity
    }

    /// Spawns custom components and the local/global 3D transform pair.
    pub fn spawn_3d(&mut self, transform: Transform3D, components: impl Bundle) -> Entity {
        let entity = self.world.spawn(components);
        self.world
            .insert(entity, (transform, GlobalTransform3D(transform.matrix())))
            .expect("newly spawned entity exists");
        entity
    }

    /// Changes a parent after checking entity lifetime, dimension and cycles.
    /// A failed edit leaves the previous parent unchanged.
    pub fn set_parent(
        &mut self,
        child: Entity,
        parent: Option<Entity>,
    ) -> Result<(), HierarchyError> {
        self.require_entity(child)?;
        if let Some(parent) = parent {
            self.require_entity(parent)?;
            let child_dim = self.dimension(child);
            if child_dim == 0 || self.dimension(parent) != child_dim {
                return Err(HierarchyError::DimensionMismatch(parent));
            }
            let mut seen = HashSet::new();
            seen.insert(child);
            let mut current = Some(parent);
            while let Some(entity) = current {
                self.require_entity(entity)?;
                if !seen.insert(entity) {
                    return Err(HierarchyError::Cycle(entity));
                }
                current = self.world.get::<&Parent>(entity).ok().map(|p| p.0);
            }
            self.world
                .insert_one(child, Parent(parent))
                .expect("validated child");
        } else {
            let _ = self.world.remove_one::<Parent>(child);
        }
        Ok(())
    }

    /// Despawns an entity and detaches its direct children, keeping local transforms.
    pub fn despawn(&mut self, entity: Entity) -> Result<(), hecs::NoSuchEntity> {
        let children: Vec<_> = self
            .world
            .query::<(Entity, &Parent)>()
            .iter()
            .filter_map(|(e, p)| (p.0 == entity).then_some(e))
            .collect();
        self.world.despawn(entity)?;
        for child in children {
            let _ = self.world.remove_one::<Parent>(child);
        }
        Ok(())
    }

    /// Computes global transforms in O(entities + parent links), without recursion.
    /// Entities spawned directly into `world` need their own global component.
    /// Invalid parent chains return an error before any globals are changed.
    pub fn propagate(&mut self) -> Result<(), HierarchyError> {
        self.nodes_2d.clear();
        self.nodes_3d.clear();
        for (entity, transform, parent) in self
            .world
            .query::<(Entity, &Transform2D, Option<&Parent>)>()
            .iter()
        {
            self.nodes_2d.insert(
                entity,
                Node {
                    local: transform.matrix(),
                    parent: parent.map(|p| p.0),
                    global: None,
                    visiting: false,
                },
            );
        }
        for (entity, transform, parent) in self
            .world
            .query::<(Entity, &Transform3D, Option<&Parent>)>()
            .iter()
        {
            self.nodes_3d.insert(
                entity,
                Node {
                    local: transform.matrix(),
                    parent: parent.map(|p| p.0),
                    global: None,
                    visiting: false,
                },
            );
        }
        resolve(
            &self.world,
            &mut self.nodes_2d,
            &mut self.entities,
            &mut self.path,
            Mat3::IDENTITY,
        )?;
        resolve(
            &self.world,
            &mut self.nodes_3d,
            &mut self.entities,
            &mut self.path,
            Mat4::IDENTITY,
        )?;
        for (entity, global) in self
            .world
            .query::<(Entity, &mut GlobalTransform2D)>()
            .iter()
        {
            if let Some(node) = self.nodes_2d.get(&entity) {
                global.0 = node.global.expect("resolved node");
            }
        }
        for (entity, global) in self
            .world
            .query::<(Entity, &mut GlobalTransform3D)>()
            .iter()
        {
            if let Some(node) = self.nodes_3d.get(&entity) {
                global.0 = node.global.expect("resolved node");
            }
        }
        Ok(())
    }

    fn require_entity(&self, entity: Entity) -> Result<(), HierarchyError> {
        if self.world.contains(entity) {
            Ok(())
        } else {
            Err(HierarchyError::MissingEntity(entity))
        }
    }

    fn dimension(&self, entity: Entity) -> u8 {
        u8::from(self.world.get::<&Transform2D>(entity).is_ok())
            + 2 * u8::from(self.world.get::<&Transform3D>(entity).is_ok())
    }
}

fn resolve<M: Copy + Mul<Output = M>>(
    world: &World,
    nodes: &mut HashMap<Entity, Node<M>>,
    entities: &mut Vec<Entity>,
    path: &mut Vec<Entity>,
    identity: M,
) -> Result<(), HierarchyError> {
    entities.clear();
    entities.extend(nodes.keys().copied());
    for &entity in entities.iter() {
        path.clear();
        let mut current = entity;
        let mut matrix = loop {
            let node = nodes.get_mut(&current).ok_or_else(|| {
                if world.contains(current) {
                    HierarchyError::DimensionMismatch(current)
                } else {
                    HierarchyError::MissingEntity(current)
                }
            })?;
            if let Some(global) = node.global {
                break global;
            }
            if node.visiting {
                return Err(HierarchyError::Cycle(current));
            }
            node.visiting = true;
            path.push(current);
            if let Some(parent) = node.parent {
                current = parent;
            } else {
                break identity;
            }
        };
        for &child in path.iter().rev() {
            let node = nodes.get_mut(&child).expect("visited node exists");
            matrix = matrix * node.local;
            node.global = Some(matrix);
            node.visiting = false;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Vec2, Vec3};

    #[test]
    fn parent_rotation_and_scale_affect_child_translation() {
        let mut scene = Scene::new();
        let root = scene.spawn_2d(
            Transform2D {
                position: Vec2::new(10.0, 0.0),
                rotation: std::f32::consts::FRAC_PI_2,
                scale: Vec2::splat(2.0),
            },
            (),
        );
        let child = scene.spawn_2d(Transform2D::at(Vec2::X), ());
        scene.set_parent(child, Some(root)).unwrap();
        scene.propagate().unwrap();
        let global = scene.world.get::<&GlobalTransform2D>(child).unwrap();
        assert!(
            global
                .0
                .transform_point2(Vec2::ZERO)
                .abs_diff_eq(Vec2::new(10.0, 2.0), 0.00001)
        );
    }

    #[test]
    fn cycle_and_dimension_edits_are_atomic() {
        let mut scene = Scene::new();
        let a = scene.spawn_2d(Transform2D::default(), ());
        let b = scene.spawn_2d(Transform2D::default(), ());
        let c = scene.spawn_3d(Transform3D::default(), ());
        scene.set_parent(b, Some(a)).unwrap();
        assert!(matches!(
            scene.set_parent(a, Some(b)),
            Err(HierarchyError::Cycle(_))
        ));
        assert!(scene.world.get::<&Parent>(a).is_err());
        assert!(matches!(
            scene.set_parent(b, Some(c)),
            Err(HierarchyError::DimensionMismatch(_))
        ));
        assert_eq!(scene.world.get::<&Parent>(b).unwrap().0, a);
    }

    #[test]
    fn raw_cycles_and_stale_parents_are_reported() {
        let mut scene = Scene::new();
        let a = scene.spawn_3d(Transform3D::at(Vec3::X), ());
        let b = scene.spawn_3d(Transform3D::at(Vec3::Y), ());
        scene.world.insert_one(a, Parent(b)).unwrap();
        scene.world.insert_one(b, Parent(a)).unwrap();
        assert!(matches!(scene.propagate(), Err(HierarchyError::Cycle(_))));
        scene.world.despawn(b).unwrap();
        assert_eq!(scene.propagate(), Err(HierarchyError::MissingEntity(b)));
    }

    #[test]
    fn despawn_detaches_children_and_reused_ids_do_not_reparent() {
        let mut scene = Scene::new();
        let root = scene.spawn_2d(Transform2D::at(Vec2::splat(5.0)), ());
        let child = scene.spawn_2d(Transform2D::at(Vec2::ONE), ());
        scene.set_parent(child, Some(root)).unwrap();
        scene.despawn(root).unwrap();
        scene.spawn_2d(Transform2D::at(Vec2::splat(100.0)), ());
        scene.propagate().unwrap();
        assert_eq!(
            scene
                .world
                .get::<&GlobalTransform2D>(child)
                .unwrap()
                .0
                .transform_point2(Vec2::ZERO),
            Vec2::ONE
        );
    }

    #[test]
    fn deep_hierarchies_do_not_use_the_call_stack() {
        let mut scene = Scene::new();
        let mut parent = scene.spawn_3d(Transform3D::at(Vec3::X), ());
        // Insert directly to make setup linear; propagate must still validate it.
        for _ in 0..5000 {
            let child = scene.spawn_3d(Transform3D::at(Vec3::X), (Parent(parent),));
            parent = child;
        }
        scene.propagate().unwrap();
        assert_eq!(
            scene
                .world
                .get::<&GlobalTransform3D>(parent)
                .unwrap()
                .0
                .transform_point3(Vec3::ZERO)
                .x,
            5001.0
        );
    }
}
