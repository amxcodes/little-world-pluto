//! Motion primitives shared by the Windows shell and the native preview.
//!
//! The current tuning and its provenance are documented in `docs/PROVENANCE.md`.
//! The motion probe is diagnostic and does not represent a separate product UI.

use std::f64::consts::PI;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    pub stiffness: f64,
    pub damping: f64,
    pub mass: f64,
}

/// A small frame-driven damped spring used by native UI hosts.
///
/// Retargeting preserves velocity, which allows an interrupted open/close
/// transition to reverse naturally instead of restarting from rest.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpringMotion {
    value: f64,
    velocity: f64,
    target: f64,
    spring: Spring,
}

impl SpringMotion {
    pub const fn new(value: f64, spring: Spring) -> Self {
        Self {
            value,
            velocity: 0.0,
            target: value,
            spring,
        }
    }

    pub fn retarget(&mut self, target: f64, spring: Spring) -> bool {
        if !target.is_finite()
            || !spring.stiffness.is_finite()
            || spring.stiffness <= 0.0
            || !spring.damping.is_finite()
            || spring.damping < 0.0
            || !spring.mass.is_finite()
            || spring.mass <= 0.0
        {
            return false;
        }

        self.target = target;
        self.spring = spring;
        true
    }

    /// Retarget while discarding velocity that points away from the new
    /// destination. This is useful for shell contours: reversing an in-flight
    /// open/close should respond immediately instead of carrying old momentum
    /// past the new endpoint and producing a last-frame wobble.
    pub fn retarget_without_reversal(&mut self, target: f64, spring: Spring) -> bool {
        if !self.retarget(target, spring) {
            return false;
        }
        let remaining = self.target - self.value;
        if remaining == 0.0 || self.velocity.signum() != remaining.signum() {
            self.velocity = 0.0;
        }
        true
    }

    /// Move to a target immediately and discard the current spring velocity.
    /// Use for initial state, reduced motion, or host state restoration.
    pub fn snap_to(&mut self, value: f64) -> bool {
        if !value.is_finite() {
            return false;
        }

        self.value = value;
        self.velocity = 0.0;
        self.target = value;
        true
    }

    /// Integrate at most 50 ms, using bounded substeps to stay stable after a
    /// delayed frame. Returns `true` while another animation frame is needed.
    pub fn advance(&mut self, elapsed: Duration) -> bool {
        if self.is_settled() {
            self.value = self.target;
            self.velocity = 0.0;
            return false;
        }

        let elapsed = elapsed.as_secs_f64().clamp(0.0, 0.05);
        let steps = (elapsed / (1.0 / 120.0)).ceil().max(1.0) as u32;
        let dt = elapsed / f64::from(steps);

        for _ in 0..steps {
            let displacement = self.value - self.target;
            let acceleration = (-self.spring.stiffness * displacement
                - self.spring.damping * self.velocity)
                / self.spring.mass;
            self.velocity += acceleration * dt;
            self.value += self.velocity * dt;
        }

        if self.is_settled() {
            self.value = self.target;
            self.velocity = 0.0;
            false
        } else {
            true
        }
    }

    pub const fn value(self) -> f64 {
        self.value
    }

    pub const fn target(self) -> f64 {
        self.target
    }

    pub const fn velocity(self) -> f64 {
        self.velocity
    }

    pub fn is_settled(self) -> bool {
        (self.value - self.target).abs() <= 0.05 && self.velocity.abs() <= 0.05
    }
}

pub const OPEN_SPRING: Spring = Spring {
    stiffness: 700.0,
    damping: 40.0,
    mass: 0.8,
};

pub const COLLAPSE_SPRING: Spring = Spring {
    stiffness: 800.0,
    damping: 43.0,
    mass: 0.8,
};

pub const LIQUID_EXPANDED_SPRING: Spring = Spring {
    stiffness: 260.0,
    damping: 18.0,
    mass: 1.0,
};

pub const LIQUID_FOLD_STIFFNESS: f64 = 340.0;
pub const LIQUID_FOLD_MASS: f64 = 1.0;
pub const LIQUID_FOLD_WIDTH_THRESHOLD_PX: f64 = 16.0;
pub const LIQUID_FOLD_HEIGHT_THRESHOLD_PX: f64 = 7.0;

/// Hover-delay defaults and reference-compatible settings bounds. These are
/// input-intent delays, not spring durations.
// Match Phase 0's immediate transition start. The reference's 150 ms intent
// delay remains user-selectable through the settings control.
pub const HOVER_OPEN_DELAY: Duration = Duration::ZERO;
pub const HOVER_CLOSE_DELAY: Duration = Duration::from_millis(350);
pub const AUTO_TUCK_DELAY: Duration = Duration::from_millis(700);
pub const CONTENT_ENTER_DELAY: Duration = Duration::from_millis(70);
pub const CONTENT_ENTER_DURATION: Duration = Duration::from_millis(240);
pub const CONTENT_EXIT_DURATION: Duration = Duration::from_millis(60);
pub const HOVER_OPEN_DELAY_MIN_MS: u32 = 0;
pub const HOVER_OPEN_DELAY_MAX_MS: u32 = 600;
pub const HOVER_OPEN_DELAY_STEP_MS: u32 = 25;
pub const HOVER_CLOSE_DELAY_MIN_MS: u32 = 100;
pub const HOVER_CLOSE_DELAY_MAX_MS: u32 = 1_500;
pub const HOVER_CLOSE_DELAY_STEP_MS: u32 = 50;

/// Derive the liquid-mode fold damping from travel distance and the reference
/// settling threshold. Returns `None` for invalid geometry input.
pub fn liquid_fold_spring(distance_px: f64, threshold_px: f64) -> Option<Spring> {
    if !distance_px.is_finite()
        || !threshold_px.is_finite()
        || distance_px < 0.0
        || threshold_px <= 0.0
    {
        return None;
    }

    let normalized_error = (threshold_px / distance_px.max(1.0)).min(0.113);
    // Extremely small thresholds or very large travel distances can underflow
    // the ratio to zero. Reject that geometry instead of returning NaN damping.
    if !normalized_error.is_finite() || normalized_error <= 0.0 {
        return None;
    }

    let damping_ratio = -normalized_error.ln() / PI;
    let ratio = damping_ratio / (1.0 + damping_ratio * damping_ratio).sqrt();
    let damping = 2.0 * LIQUID_FOLD_STIFFNESS.sqrt() * ratio;

    Some(Spring {
        stiffness: LIQUID_FOLD_STIFFNESS,
        damping,
        mass: LIQUID_FOLD_MASS,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_springs_and_delays_are_kept_as_tuning_inputs() {
        assert_eq!(OPEN_SPRING.stiffness, 700.0);
        assert_eq!(COLLAPSE_SPRING.damping, 43.0);
        assert_eq!(LIQUID_EXPANDED_SPRING.mass, 1.0);
        assert_eq!(HOVER_OPEN_DELAY, Duration::ZERO);
        assert_eq!(HOVER_CLOSE_DELAY, Duration::from_millis(350));
        assert_eq!(AUTO_TUCK_DELAY, Duration::from_millis(700));
        assert_eq!(CONTENT_ENTER_DELAY, Duration::from_millis(70));
        assert_eq!(CONTENT_ENTER_DURATION, Duration::from_millis(240));
        assert_eq!(CONTENT_EXIT_DURATION, Duration::from_millis(60));
    }

    #[test]
    fn liquid_fold_damping_is_finite_and_increases_with_travel() {
        let short = liquid_fold_spring(1.0, LIQUID_FOLD_WIDTH_THRESHOLD_PX).unwrap();
        let long = liquid_fold_spring(1_000.0, LIQUID_FOLD_WIDTH_THRESHOLD_PX).unwrap();
        let upper_bound = 2.0 * LIQUID_FOLD_STIFFNESS.sqrt();

        assert!(short.damping.is_finite());
        assert!(short.damping > 0.0);
        assert!(long.damping > short.damping);
        assert!(long.damping < upper_bound);
    }

    #[test]
    fn liquid_fold_rejects_invalid_geometry() {
        assert!(liquid_fold_spring(f64::NAN, 16.0).is_none());
        assert!(liquid_fold_spring(12.0, f64::INFINITY).is_none());
        assert!(liquid_fold_spring(-1.0, 16.0).is_none());
        assert!(liquid_fold_spring(12.0, 0.0).is_none());
    }

    #[test]
    fn spring_retargets_without_discarding_velocity_and_settles() {
        let mut spring = SpringMotion::new(0.0, OPEN_SPRING);
        assert!(spring.retarget(100.0, OPEN_SPRING));
        for _ in 0..8 {
            assert!(spring.advance(Duration::from_millis(16)));
        }
        let velocity_before_reverse = spring.velocity();
        assert!(velocity_before_reverse > 0.0);

        assert!(spring.retarget(0.0, COLLAPSE_SPRING));
        assert_eq!(spring.velocity(), velocity_before_reverse);
        for _ in 0..400 {
            if !spring.advance(Duration::from_millis(16)) {
                break;
            }
        }

        assert!(spring.is_settled());
        assert_eq!(spring.value(), 0.0);
    }

    #[test]
    fn spring_rejects_invalid_targets_and_bounds_long_frames() {
        let mut spring = SpringMotion::new(0.0, OPEN_SPRING);
        assert!(!spring.retarget(f64::NAN, OPEN_SPRING));
        assert!(!spring.retarget(
            10.0,
            Spring {
                stiffness: 0.0,
                ..OPEN_SPRING
            }
        ));
        assert!(spring.retarget(10.0, OPEN_SPRING));
        assert!(spring.advance(Duration::from_secs(10)));
        assert!(spring.value().is_finite());
        assert!(spring.velocity().is_finite());
    }

    #[test]
    fn snap_to_clears_motion_and_rejects_non_finite_values() {
        let mut spring = SpringMotion::new(0.0, OPEN_SPRING);
        assert!(spring.retarget(40.0, OPEN_SPRING));
        assert!(spring.advance(Duration::from_millis(16)));

        assert!(!spring.snap_to(f64::INFINITY));
        assert!(spring.snap_to(12.0));
        assert_eq!(spring.value(), 12.0);
        assert_eq!(spring.target(), 12.0);
        assert_eq!(spring.velocity(), 0.0);
        assert!(!spring.advance(Duration::from_millis(16)));
    }
}
