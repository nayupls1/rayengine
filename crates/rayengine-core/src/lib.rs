//! Display-independent primitives shared by 2D and 3D rayengine games.
//!
//! This crate deliberately has no raylib dependency. Simulations and tests can
//! run without a C compiler, window, display server, or graphics context.

pub use glam;
pub use hecs::{Bundle, Entity, World};

pub mod audio;
pub mod camera;
pub mod collision;
pub mod events;
pub mod first_person;
pub mod input;
pub mod jobs;
pub mod manifest;
pub mod mesh;
pub mod pathfinding;
pub mod physics;
pub mod placement;
pub mod quality;
pub mod save;
pub mod scene;
pub mod skeletal;
pub mod spatial;
pub mod sprite;
pub mod time;
pub mod transform;
pub mod tween;
pub mod ui;
pub mod viewport;

/// Common imports for display-independent game code.
pub mod prelude {
    pub use crate::audio::{AudioBuses, AudioError, AudioSettings, BusId, BusSettings, GainFade};
    pub use crate::camera::{Camera2D, Camera3D};
    pub use crate::collision::{
        Aabb2, Aabb3, Body2D, Body3D, Circle, Sector2, SectorError, Sphere,
    };
    pub use crate::events::Events;
    pub use crate::first_person::{
        FirstPersonActions, FirstPersonConfig, FirstPersonController, FirstPersonError,
        FirstPersonInput,
    };
    pub use crate::input::{Action, Axis, Input, InputView};
    pub use crate::jobs::{Cancellation, Completion, JobHandle, JobId, JobOutcome, JobPool};
    pub use crate::mesh::{MeshData, MeshError, MeshInfo};
    pub use crate::pathfinding::{
        AgentId, AgentState, ClearanceGrid, CostGrid, DiagonalRule, DistanceField, GridFn,
        GridLayout, LinkId, NavFinder, NavGrid, NavLink, NavOptions, NavPoint, NavStep,
        NavTopology, Neighborhood, PathError, PathFinder, PathFollower, PathOptions, PathStatus,
        Route, Traffic, TrafficEvent, TrafficOptions,
    };
    pub use crate::physics::{
        BodyId, BodyKind, CastHit2D, CastHit3D, CollisionFilter, OverlapHit2D, OverlapHit3D,
        Penetration2D, Penetration3D, PhysicsBody2D, PhysicsBody3D, PhysicsWorld2D, PhysicsWorld3D,
        QueryError, QueryFilter, Shape2D, Shape3D, StepReport, TriggerEvent, TriggerPhase,
        UniformGrid2D, UniformGrid3D, WorldCastHit2D, WorldCastHit3D,
    };
    pub use crate::placement::{
        Footprint, PlacedObject, PlacementChange, PlacementError, PlacementGrid, PlacementId,
        PlacementNav, PlacementPose, QuarterTurn,
    };
    pub use crate::quality::{AntiAliasing, RenderPlan, RenderQuality};
    pub use crate::scene::Scene;
    pub use crate::skeletal::{
        ClipCompleted, ClipTiming, KeyframePlayer, KeyframeRate, SkeletalError,
    };
    pub use crate::spatial::{
        Frustum2D, Frustum3D, Ray2, Ray3, RayHit2, RayHit3, SpatialError, SpatialHit2, SpatialHit3,
        SpatialIndex2D, SpatialIndex3D,
    };
    pub use crate::sprite::{
        AnimationClip, AnimationCompleted, AnimationPlayer, PlaybackMode, SpriteError, SpriteFrame,
        SpriteRegion, SpriteTransform,
    };
    pub use crate::time::{
        FixedClock, FramePlan, InvalidSimulationSpeed, SimulationClock, Tick, Timer,
    };
    pub use crate::transform::{
        GlobalTransform2D, GlobalTransform3D, Parent, Transform2D, Transform3D,
    };
    pub use crate::tween::{
        Animate, Ease, Parallel, Sequence, Shake, ShakeConfig, Tween, TweenCompleted, TweenId,
        TweenMode, Tweenable,
    };
    pub use crate::ui::{
        UiActions, UiButton, UiCapture, UiClip, UiId, UiInput, UiLayout, UiRect, UiRegion,
        UiResponse, UiScrollState, UiState,
    };
    pub use crate::viewport::{ScaleMode, Viewport};
    pub use glam::{IVec2, Quat, UVec2, Vec2, Vec3, Vec4};
    pub use hecs::{Entity, World};
}
