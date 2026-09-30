//! Optional first-person movement over Body3D. Games own worlds, spawning,
//! checkpoints, menus, interaction and input-edge consumption.

use crate::{
    camera::Camera3D,
    collision::{Aabb3, Body3D},
    input::{Action, Input, InputView},
};
use glam::{Vec2, Vec3};
use std::fmt;

/// Game-chosen action IDs, independent of physical keyboard/mouse bindings.
#[derive(Clone, Copy, Debug)]
pub struct FirstPersonActions {
    /// Strafe left.
    pub left: Action,
    /// Strafe right.
    pub right: Action,
    /// Walk forward.
    pub forward: Action,
    /// Walk backward.
    pub back: Action,
    /// Jump on a press edge.
    pub jump: Action,
    /// Optional held sprint action.
    pub sprint: Option<Action>,
    /// Optional keyboard look left.
    pub turn_left: Option<Action>,
    /// Optional keyboard look right.
    pub turn_right: Option<Action>,
}

/// One fixed tick's input. Movement/turn axes must be finite and in [-1, 1].
/// Mouse motion is accumulated displacement, not velocity; never multiply by dt.
#[derive(Clone, Copy, Debug, Default)]
pub struct FirstPersonInput {
    /// Right/back positive; forward is negative Y. Diagonals are normalized.
    pub movement: Vec2,
    /// Relative logical-window pointer displacement, positive Y looks down.
    pub look_delta: Vec2,
    /// Keyboard yaw axis, positive turns right. This is multiplied by dt.
    pub turn_axis: f32,
    /// Held sprint state.
    pub sprint: bool,
    /// Jump press edge, supplied once per fixed update sequence.
    pub jump_pressed: bool,
}
impl FirstPersonInput {
    /// Samples the game's bindings. The caller consumes Input edges after a tick.
    pub fn from_actions(input: &Input, actions: FirstPersonActions) -> Self {
        Self::from_view(input.routed(&[], false), actions)
    }
    /// Samples a routed view so UI can mask gameplay actions and mouse look.
    pub fn from_view(input: InputView<'_>, actions: FirstPersonActions) -> Self {
        Self {
            movement: Vec2::new(
                input.axis(actions.left, actions.right),
                input.axis(actions.forward, actions.back),
            ),
            look_delta: input.pointer_delta(),
            turn_axis: f32::from(actions.turn_right.is_some_and(|a| input.down(a)))
                - f32::from(actions.turn_left.is_some_and(|a| input.down(a))),
            sprint: actions.sprint.is_some_and(|a| input.down(a)),
            jump_pressed: input.pressed(actions.jump),
        }
    }
}

/// Validated once when creating/reconfiguring a controller. Positive Y is up.
#[derive(Clone, Copy, Debug)]
pub struct FirstPersonConfig {
    /// Walking speed, world units/second.
    pub walk_speed: f32,
    /// Sprinting speed, world units/second.
    pub sprint_speed: f32,
    /// Exponential horizontal response rate, per second. Zero selects immediate motion.
    pub response: f32,
    /// Initial jump velocity, world units/second.
    pub jump_speed: f32,
    /// Positive downward acceleration, world units/second squared.
    pub gravity: f32,
    /// Positive maximum falling speed, world units/second.
    pub max_fall_speed: f32,
    /// Radians per logical-window pointer unit, independent of viewport/DPI scaling.
    pub mouse_sensitivity: f32,
    /// Keyboard yaw speed, radians/second.
    pub keyboard_turn_speed: f32,
    /// Minimum pitch in radians, strictly above -PI/2.
    pub min_pitch: f32,
    /// Maximum pitch in radians, strictly below PI/2.
    pub max_pitch: f32,
    /// World-space eye offset relative to body center.
    pub eye_offset: Vec3,
    /// Vertical field of view in degrees, strictly between 0 and 180.
    pub vertical_fov: f32,
    /// Seconds after leaving ground in which jumping remains possible; zero disables grace.
    pub coyote_time: f32,
    /// Seconds to retain a jump edge before landing; zero requires an immediate jump.
    pub jump_buffer_time: f32,
}
impl Default for FirstPersonConfig {
    fn default() -> Self {
        Self {
            walk_speed: 6.0,
            sprint_speed: 10.0,
            response: 18.0,
            jump_speed: 11.0,
            gravity: 26.0,
            max_fall_speed: 40.0,
            mouse_sensitivity: 0.0025,
            keyboard_turn_speed: 1.8,
            min_pitch: -std::f32::consts::FRAC_PI_2 + 0.05,
            max_pitch: std::f32::consts::FRAC_PI_2 - 0.05,
            eye_offset: Vec3::new(0.0, 0.7, 0.0),
            vertical_fov: 75.0,
            coyote_time: 0.1,
            jump_buffer_time: 0.12,
        }
    }
}

/// Invalid controller configuration or explicit position/look change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FirstPersonError(pub &'static str);
impl fmt::Display for FirstPersonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for FirstPersonError {}
impl FirstPersonConfig {
    /// Rejects nonfinite values, negative rates and camera limits crossing the poles.
    pub fn validate(&self) -> Result<(), FirstPersonError> {
        let nonnegative = [
            self.walk_speed,
            self.sprint_speed,
            self.response,
            self.jump_speed,
            self.mouse_sensitivity,
            self.keyboard_turn_speed,
            self.coyote_time,
            self.jump_buffer_time,
        ];
        if nonnegative.into_iter().any(|v| !v.is_finite() || v < 0.0) {
            return Err(FirstPersonError(
                "speeds, response, sensitivity and grace times must be finite and nonnegative",
            ));
        }
        if !self.gravity.is_finite()
            || self.gravity <= 0.0
            || !self.max_fall_speed.is_finite()
            || self.max_fall_speed <= 0.0
        {
            return Err(FirstPersonError(
                "gravity and maximum falling speed must be finite and positive",
            ));
        }
        if !self.min_pitch.is_finite()
            || !self.max_pitch.is_finite()
            || self.min_pitch <= -std::f32::consts::FRAC_PI_2
            || self.max_pitch >= std::f32::consts::FRAC_PI_2
            || self.min_pitch > self.max_pitch
        {
            return Err(FirstPersonError(
                "pitch limits must be ordered and strictly inside (-PI/2, PI/2)",
            ));
        }
        if !self.eye_offset.is_finite()
            || !self.vertical_fov.is_finite()
            || !(0.0..180.0).contains(&self.vertical_fov)
            || self.vertical_fov == 0.0
        {
            return Err(FirstPersonError(
                "eye offset must be finite and vertical FOV must be in (0, 180)",
            ));
        }
        Ok(())
    }
}

/// Allocation-free optional helper. Spawn outside static solids; initial overlaps
/// are not depenetrated. Body movement resolves X, Z then Y, using Body3D sweeps.
#[derive(Clone, Copy, Debug)]
pub struct FirstPersonController {
    /// Game-accessible swept body. External edits must retain Body3D invariants.
    pub body: Body3D,
    /// Previous center for interpolation. Use teleport for discontinuous movement.
    pub previous: Vec3,
    config: FirstPersonConfig,
    yaw: f32,
    pitch: f32,
    coyote: f32,
    jump_buffer: f32,
}
impl FirstPersonController {
    /// Creates a controller from center/full body dimensions. Rejects invalid
    /// config and nonfinite/nonpositive body dimensions before constructing it.
    pub fn new(
        position: Vec3,
        size: Vec3,
        config: FirstPersonConfig,
    ) -> Result<Self, FirstPersonError> {
        config.validate()?;
        if !position.is_finite() || !size.is_finite() || size.min_element() <= 0.0 {
            return Err(FirstPersonError(
                "body position must be finite and dimensions finite and positive",
            ));
        }
        let body = Body3D::new(position, size);
        if !valid_bounds(body.position, body.half_size) {
            return Err(FirstPersonError(
                "body bounds must retain finite positive extent at the chosen coordinates",
            ));
        }
        Ok(Self {
            body,
            previous: position,
            config,
            yaw: 0.0,
            pitch: 0.0_f32.clamp(config.min_pitch, config.max_pitch),
            coyote: 0.0,
            jump_buffer: 0.0,
        })
    }
    /// Current validated configuration.
    pub fn config(&self) -> &FirstPersonConfig {
        &self.config
    }
    /// Reconfigures atomically, clamping pitch and clearing pending jump/grace state.
    pub fn set_config(&mut self, config: FirstPersonConfig) -> Result<(), FirstPersonError> {
        config.validate()?;
        self.config = config;
        self.pitch = self.pitch.clamp(config.min_pitch, config.max_pitch);
        self.coyote = 0.0;
        self.jump_buffer = 0.0;
        Ok(())
    }
    /// Horizontal yaw in radians, positive turns toward +X from the -Z heading.
    pub fn yaw(&self) -> f32 {
        self.yaw
    }
    /// Vertical pitch in radians, positive looks up.
    pub fn pitch(&self) -> f32 {
        self.pitch
    }
    /// Sets finite angles, wrapping yaw and clamping pitch. Failure changes nothing.
    pub fn set_look(&mut self, yaw: f32, pitch: f32) -> Result<(), FirstPersonError> {
        if !yaw.is_finite() || !pitch.is_finite() {
            return Err(FirstPersonError("look angles must be finite"));
        }
        self.yaw = wrap_yaw(f64::from(yaw));
        self.pitch = pitch.clamp(self.config.min_pitch, self.config.max_pitch);
        Ok(())
    }
    /// Resets movement, contact, interpolation and pending jumps at a game-chosen
    /// position. Keeps look angles; checkpoint/spawn validity remains game policy.
    pub fn teleport(&mut self, position: Vec3) -> Result<(), FirstPersonError> {
        if !valid_bounds(position, self.body.half_size) {
            return Err(FirstPersonError(
                "teleport bounds must retain finite positive extent",
            ));
        }
        self.body.position = position;
        self.previous = position;
        self.body.velocity = Vec3::ZERO;
        self.body.grounded = false;
        self.coyote = 0.0;
        self.jump_buffer = 0.0;
        Ok(())
    }
    /// Applies one fixed tick. Panics for invalid input or nonfinite/negative dt.
    /// A zero dt applies look only, retaining movement/contact/interpolation state.
    /// Consume press edges and relative motion once afterward, including catch-up.
    pub fn step(&mut self, input: FirstPersonInput, dt: f32, solids: &[Aabb3]) {
        assert!(dt.is_finite() && dt >= 0.0);
        assert!(
            input.movement.is_finite()
                && input.movement.abs().max_element() <= 1.0
                && input.look_delta.is_finite()
                && input.turn_axis.is_finite()
                && input.turn_axis.abs() <= 1.0
        );
        // f64 avoids overflow from large finite mouse displacements/sensitivities.
        let turn = f64::from(input.look_delta.x) * f64::from(self.config.mouse_sensitivity)
            + f64::from(input.turn_axis)
                * f64::from(dt)
                * f64::from(self.config.keyboard_turn_speed);
        if turn != 0.0 {
            self.yaw = wrap_yaw(f64::from(self.yaw) + turn);
        }
        self.pitch = (f64::from(self.pitch)
            - f64::from(input.look_delta.y) * f64::from(self.config.mouse_sensitivity))
        .clamp(
            f64::from(self.config.min_pitch),
            f64::from(self.config.max_pitch),
        ) as f32;
        if dt == 0.0 {
            return;
        }
        self.previous = self.body.position;
        let axis = input.movement.clamp_length_max(1.0);
        let (sin, cos) = self.yaw.sin_cos();
        let movement = Vec3::new(
            axis.x * cos - axis.y * sin,
            0.0,
            axis.x * sin + axis.y * cos,
        );
        let speed = if input.sprint {
            self.config.sprint_speed
        } else {
            self.config.walk_speed
        };
        let blend = if self.config.response == 0.0 {
            1.0
        } else {
            1.0 - (-self.config.response * dt).exp()
        };
        self.body.velocity.x += (movement.x * speed - self.body.velocity.x) * blend;
        self.body.velocity.z += (movement.z * speed - self.body.velocity.z) * blend;
        self.coyote = if self.body.grounded {
            self.config.coyote_time
        } else {
            (self.coyote - dt).max(0.0)
        };
        self.jump_buffer = if input.jump_pressed {
            self.config.jump_buffer_time
        } else {
            (self.jump_buffer - dt).max(0.0)
        };
        if (self.body.grounded || self.coyote > 0.0)
            && (input.jump_pressed || self.jump_buffer > 0.0)
        {
            self.body.velocity.y = self.config.jump_speed;
            self.body.grounded = false;
            self.coyote = 0.0;
            self.jump_buffer = 0.0;
        }
        self.body.velocity.y =
            (self.body.velocity.y - self.config.gravity * dt).max(-self.config.max_fall_speed);
        self.body.move_and_slide(dt, solids);
    }
    /// Eye camera with interpolated position and latest (uninterpolated) angles.
    /// Alpha must be finite in [0, 1]. Teleport removes interpolation across resets.
    pub fn camera(&self, alpha: f32) -> Camera3D {
        assert!(alpha.is_finite() && (0.0..=1.0).contains(&alpha));
        let position = self.previous.lerp(self.body.position, alpha) + self.config.eye_offset;
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        Camera3D {
            position,
            target: position + Vec3::new(sin_yaw * cos_pitch, sin_pitch, -cos_yaw * cos_pitch),
            up: Vec3::Y,
            vertical_fov: self.config.vertical_fov,
        }
    }
}
fn wrap_yaw(yaw: f64) -> f32 {
    ((yaw + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI) as f32
}

fn valid_bounds(position: Vec3, half_size: Vec3) -> bool {
    let (min, max) = (position - half_size, position + half_size);
    min.is_finite() && max.is_finite() && min.cmplt(max).all()
}

#[cfg(test)]
mod tests;
