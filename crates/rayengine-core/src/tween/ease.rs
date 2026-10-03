//! Standard easing curves as pure functions of normalized progress.
//!
//! Every function clamps its input to `[0, 1]` (NaN counts as zero) and returns
//! exactly `0.0` at the start and `1.0` at the end. Back and elastic curves
//! deliberately overshoot between the endpoints.

use std::f32::consts::PI;

const BACK: f32 = 1.70158;
const BACK_IN_OUT: f32 = BACK * 1.525;

/// Clamps progress and pins both endpoints so curves never drift at boundaries.
#[inline]
fn bounded(t: f32, curve: impl FnOnce(f32) -> f32) -> f32 {
    if t.is_nan() || t <= 0.0 {
        0.0
    } else if t >= 1.0 {
        1.0
    } else {
        curve(t)
    }
}

/// Constant rate.
pub fn linear(t: f32) -> f32 {
    bounded(t, |t| t)
}

/// Quadratic acceleration from rest.
pub fn quad_in(t: f32) -> f32 {
    bounded(t, |t| t * t)
}
/// Quadratic deceleration to rest.
pub fn quad_out(t: f32) -> f32 {
    bounded(t, |t| 1.0 - (1.0 - t) * (1.0 - t))
}
/// Quadratic acceleration, then deceleration.
pub fn quad_in_out(t: f32) -> f32 {
    bounded(t, |t| {
        if t < 0.5 {
            2.0 * t * t
        } else {
            1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
        }
    })
}

/// Cubic acceleration from rest.
pub fn cubic_in(t: f32) -> f32 {
    bounded(t, |t| t * t * t)
}
/// Cubic deceleration to rest.
pub fn cubic_out(t: f32) -> f32 {
    bounded(t, |t| 1.0 - (1.0 - t).powi(3))
}
/// Cubic acceleration, then deceleration.
pub fn cubic_in_out(t: f32) -> f32 {
    bounded(t, |t| {
        if t < 0.5 {
            4.0 * t * t * t
        } else {
            1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
        }
    })
}

/// Quarter-sine acceleration from rest.
pub fn sine_in(t: f32) -> f32 {
    bounded(t, |t| 1.0 - (t * PI / 2.0).cos())
}
/// Quarter-sine deceleration to rest.
pub fn sine_out(t: f32) -> f32 {
    bounded(t, |t| (t * PI / 2.0).sin())
}
/// Half-cosine acceleration, then deceleration.
pub fn sine_in_out(t: f32) -> f32 {
    bounded(t, |t| -((PI * t).cos() - 1.0) / 2.0)
}

/// Exponential acceleration (base 2, ten doublings).
pub fn expo_in(t: f32) -> f32 {
    bounded(t, |t| 2f32.powf(10.0 * t - 10.0))
}
/// Exponential deceleration (base 2, ten halvings).
pub fn expo_out(t: f32) -> f32 {
    bounded(t, |t| 1.0 - 2f32.powf(-10.0 * t))
}
/// Exponential acceleration, then deceleration.
pub fn expo_in_out(t: f32) -> f32 {
    bounded(t, |t| {
        if t < 0.5 {
            2f32.powf(20.0 * t - 10.0) / 2.0
        } else {
            (2.0 - 2f32.powf(-20.0 * t + 10.0)) / 2.0
        }
    })
}

/// Pulls back below zero before accelerating.
pub fn back_in(t: f32) -> f32 {
    bounded(t, |t| (BACK + 1.0) * t * t * t - BACK * t * t)
}
/// Overshoots above one before settling.
pub fn back_out(t: f32) -> f32 {
    bounded(t, |t| {
        let u = t - 1.0;
        1.0 + (BACK + 1.0) * u * u * u + BACK * u * u
    })
}
/// Pulls back at the start and overshoots at the end.
pub fn back_in_out(t: f32) -> f32 {
    bounded(t, |t| {
        if t < 0.5 {
            (2.0 * t).powi(2) * ((BACK_IN_OUT + 1.0) * 2.0 * t - BACK_IN_OUT) / 2.0
        } else {
            ((2.0 * t - 2.0).powi(2) * ((BACK_IN_OUT + 1.0) * (t * 2.0 - 2.0) + BACK_IN_OUT) + 2.0)
                / 2.0
        }
    })
}

/// Growing oscillation before release.
pub fn elastic_in(t: f32) -> f32 {
    bounded(t, |t| {
        -2f32.powf(10.0 * t - 10.0) * ((t * 10.0 - 10.75) * (2.0 * PI / 3.0)).sin()
    })
}
/// Decaying oscillation around the target.
pub fn elastic_out(t: f32) -> f32 {
    bounded(t, |t| {
        2f32.powf(-10.0 * t) * ((t * 10.0 - 0.75) * (2.0 * PI / 3.0)).sin() + 1.0
    })
}
/// Growing, then decaying oscillation.
pub fn elastic_in_out(t: f32) -> f32 {
    bounded(t, |t| {
        let wave = ((20.0 * t - 11.125) * (2.0 * PI / 4.5)).sin();
        if t < 0.5 {
            -(2f32.powf(20.0 * t - 10.0) * wave) / 2.0
        } else {
            2f32.powf(-20.0 * t + 10.0) * wave / 2.0 + 1.0
        }
    })
}

/// Bounces that grow before leaving the start.
pub fn bounce_in(t: f32) -> f32 {
    bounded(t, |t| 1.0 - bounce(1.0 - t))
}
/// Decaying bounces against the target.
pub fn bounce_out(t: f32) -> f32 {
    bounded(t, bounce)
}
/// Growing bounces, then decaying bounces.
pub fn bounce_in_out(t: f32) -> f32 {
    bounded(t, |t| {
        if t < 0.5 {
            (1.0 - bounce(1.0 - 2.0 * t)) / 2.0
        } else {
            (1.0 + bounce(2.0 * t - 1.0)) / 2.0
        }
    })
}

fn bounce(t: f32) -> f32 {
    const N: f32 = 7.5625;
    const D: f32 = 2.75;
    if t < 1.0 / D {
        N * t * t
    } else if t < 2.0 / D {
        let t = t - 1.5 / D;
        N * t * t + 0.75
    } else if t < 2.5 / D {
        let t = t - 2.25 / D;
        N * t * t + 0.9375
    } else {
        let t = t - 2.625 / D;
        N * t * t + 0.984375
    }
}

/// A copyable selection of one easing function, stored by tweens without boxing.
///
/// ```
/// use rayengine_core::tween::Ease;
/// assert_eq!(Ease::CubicOut.apply(1.0), 1.0);
/// assert_eq!(Ease::Linear.apply(0.25), 0.25);
/// assert!(Ease::BackOut.apply(0.6) > 1.0); // overshoots before settling
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum Ease {
    #[default]
    Linear,
    QuadIn,
    QuadOut,
    QuadInOut,
    CubicIn,
    CubicOut,
    CubicInOut,
    SineIn,
    SineOut,
    SineInOut,
    ExpoIn,
    ExpoOut,
    ExpoInOut,
    BackIn,
    BackOut,
    BackInOut,
    ElasticIn,
    ElasticOut,
    ElasticInOut,
    BounceIn,
    BounceOut,
    BounceInOut,
}

impl Ease {
    /// Every curve, in declaration order.
    pub const ALL: [Ease; 22] = [
        Ease::Linear,
        Ease::QuadIn,
        Ease::QuadOut,
        Ease::QuadInOut,
        Ease::CubicIn,
        Ease::CubicOut,
        Ease::CubicInOut,
        Ease::SineIn,
        Ease::SineOut,
        Ease::SineInOut,
        Ease::ExpoIn,
        Ease::ExpoOut,
        Ease::ExpoInOut,
        Ease::BackIn,
        Ease::BackOut,
        Ease::BackInOut,
        Ease::ElasticIn,
        Ease::ElasticOut,
        Ease::ElasticInOut,
        Ease::BounceIn,
        Ease::BounceOut,
        Ease::BounceInOut,
    ];

    /// The pure function for this curve.
    pub fn function(self) -> fn(f32) -> f32 {
        match self {
            Ease::Linear => linear,
            Ease::QuadIn => quad_in,
            Ease::QuadOut => quad_out,
            Ease::QuadInOut => quad_in_out,
            Ease::CubicIn => cubic_in,
            Ease::CubicOut => cubic_out,
            Ease::CubicInOut => cubic_in_out,
            Ease::SineIn => sine_in,
            Ease::SineOut => sine_out,
            Ease::SineInOut => sine_in_out,
            Ease::ExpoIn => expo_in,
            Ease::ExpoOut => expo_out,
            Ease::ExpoInOut => expo_in_out,
            Ease::BackIn => back_in,
            Ease::BackOut => back_out,
            Ease::BackInOut => back_in_out,
            Ease::ElasticIn => elastic_in,
            Ease::ElasticOut => elastic_out,
            Ease::ElasticInOut => elastic_in_out,
            Ease::BounceIn => bounce_in,
            Ease::BounceOut => bounce_out,
            Ease::BounceInOut => bounce_in_out,
        }
    }

    /// Eases clamped progress; exactly `0.0` at the start and `1.0` at the end.
    pub fn apply(self, t: f32) -> f32 {
        self.function()(t)
    }
}
