//! Small, deterministic shell reducer. Platform timers carry the generation
//! returned here so stale hover callbacks cannot reverse a newer interaction.

use std::time::{Duration, Instant};

use crate::motion::{
    HOVER_CLOSE_DELAY, HOVER_CLOSE_DELAY_MAX_MS, HOVER_CLOSE_DELAY_MIN_MS, HOVER_OPEN_DELAY,
    HOVER_OPEN_DELAY_MAX_MS,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellMode {
    Resting,
    HoverPending,
    Expanded,
    TextEntry,
    Tucked,
    Pinned,
    Suspended,
    Recovering,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellTimerTarget {
    Expanded,
    Resting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShellTimer {
    pub generation: u64,
    pub deadline: Instant,
    pub target: ShellTimerTarget,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShellSnapshot {
    pub mode: ShellMode,
    pub timer: Option<ShellTimer>,
    pub fullscreen: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellEvent {
    PointerEntered,
    PointerLeft,
    Open,
    Close,
    ToggleOpen,
    BeginTextEntry,
    EndTextEntry,
    Tuck,
    Untuck,
    Pin,
    Unpin,
    FullscreenChanged(bool),
    PowerSuspended,
    PowerResumed,
    HostFailed,
    HostReady,
    TimerElapsed { generation: u64 },
}

pub struct ShellReducer {
    mode: ShellMode,
    timer: Option<ShellTimer>,
    generation: u64,
    fullscreen: bool,
    text_return_mode: ShellMode,
    suspend_return_mode: ShellMode,
    power_suspended: bool,
    host_failed: bool,
    hover_open_delay: Duration,
    hover_close_delay: Duration,
    click_to_open: bool,
    close_on_outside_click: bool,
}

impl Default for ShellReducer {
    fn default() -> Self {
        Self {
            mode: ShellMode::Resting,
            timer: None,
            generation: 0,
            fullscreen: false,
            text_return_mode: ShellMode::Expanded,
            suspend_return_mode: ShellMode::Resting,
            power_suspended: false,
            host_failed: false,
            hover_open_delay: HOVER_OPEN_DELAY,
            hover_close_delay: HOVER_CLOSE_DELAY,
            click_to_open: false,
            close_on_outside_click: false,
        }
    }
}

impl ShellReducer {
    /// Set delays for future pointer transitions. An already pending timer is
    /// left alone so changing a setting cannot unexpectedly reverse the shell.
    pub fn set_hover_delays(&mut self, open_delay: Duration, close_delay: Duration) {
        self.hover_open_delay = open_delay.clamp(
            Duration::ZERO,
            Duration::from_millis(u64::from(HOVER_OPEN_DELAY_MAX_MS)),
        );
        self.hover_close_delay = close_delay.clamp(
            Duration::from_millis(u64::from(HOVER_CLOSE_DELAY_MIN_MS)),
            Duration::from_millis(u64::from(HOVER_CLOSE_DELAY_MAX_MS)),
        );
    }

    pub fn set_interaction_behavior(&mut self, click_to_open: bool, close_on_outside_click: bool) {
        self.click_to_open = click_to_open;
        self.close_on_outside_click = close_on_outside_click;
        if self.mode == ShellMode::HoverPending {
            let closing = self.timer.is_some_and(|timer| timer.target == ShellTimerTarget::Resting);
            self.cancel_timer();
            self.mode = if closing { ShellMode::Expanded } else { ShellMode::Resting };
        }
    }

    pub fn snapshot(&self) -> ShellSnapshot {
        ShellSnapshot {
            mode: self.mode,
            timer: self.timer,
            fullscreen: self.fullscreen,
        }
    }

    /// Applies one event and returns true if the observable shell snapshot changed.
    pub fn dispatch(&mut self, event: ShellEvent, now: Instant) -> bool {
        let before = self.snapshot();
        match event {
            ShellEvent::PointerEntered if !self.fullscreen => match self.mode {
                ShellMode::HoverPending if self.click_to_open
                    && self.timer.is_some_and(|timer| timer.target == ShellTimerTarget::Resting) => {
                    self.cancel_timer();
                    self.mode = ShellMode::Expanded;
                }
                _ if self.click_to_open => {}
                ShellMode::Resting => {
                    self.schedule(ShellTimerTarget::Expanded, now, self.hover_open_delay)
                }
                ShellMode::Tucked => {
                    self.schedule(ShellTimerTarget::Expanded, now, self.hover_open_delay)
                }
                ShellMode::HoverPending
                    if self
                        .timer
                        .is_some_and(|timer| timer.target == ShellTimerTarget::Resting) =>
                {
                    self.schedule(ShellTimerTarget::Expanded, now, self.hover_open_delay)
                }
                _ => {}
            },
            ShellEvent::PointerLeft if !self.fullscreen => match self.mode {
                ShellMode::HoverPending
                    if self
                        .timer
                        .is_some_and(|timer| timer.target == ShellTimerTarget::Expanded) =>
                {
                    self.cancel_timer();
                    self.mode = ShellMode::Resting;
                }
                // A focused input should not pin a hover-open Island in its
                // expanded state after the pointer leaves the surface. Keep
                // pinned text entry persistent; ordinary text entry follows
                // the same close delay as every other expanded page.
                ShellMode::TextEntry if self.text_return_mode == ShellMode::Expanded
                    && !(self.click_to_open && self.close_on_outside_click) => {
                    self.schedule(ShellTimerTarget::Resting, now, self.hover_close_delay)
                }
                ShellMode::Expanded if !(self.click_to_open && self.close_on_outside_click) => {
                    self.schedule(ShellTimerTarget::Resting, now, self.hover_close_delay)
                }
                _ => {}
            },
            ShellEvent::Open if !self.fullscreen => {
                if matches!(
                    self.mode,
                    ShellMode::Resting | ShellMode::HoverPending | ShellMode::Tucked
                ) {
                    self.cancel_timer();
                    self.mode = ShellMode::Expanded;
                }
            }
            ShellEvent::Close if !self.fullscreen => match self.mode {
                ShellMode::Expanded | ShellMode::Pinned | ShellMode::TextEntry | ShellMode::HoverPending => {
                    self.cancel_timer();
                    self.mode = ShellMode::Resting;
                }
                _ => {}
            },
            ShellEvent::ToggleOpen if !self.fullscreen => match self.mode {
                ShellMode::Resting | ShellMode::HoverPending | ShellMode::Tucked => {
                    self.cancel_timer();
                    self.mode = ShellMode::Expanded;
                }
                ShellMode::Expanded | ShellMode::Pinned => {
                    self.cancel_timer();
                    self.mode = ShellMode::Resting;
                }
                ShellMode::TextEntry | ShellMode::Suspended | ShellMode::Recovering => {}
            },
            ShellEvent::BeginTextEntry if !self.fullscreen => {
                if matches!(self.mode, ShellMode::Expanded | ShellMode::Pinned) {
                    self.text_return_mode = self.mode;
                    self.cancel_timer();
                    self.mode = ShellMode::TextEntry;
                }
            }
            ShellEvent::EndTextEntry => {
                if self.mode == ShellMode::TextEntry {
                    self.mode = self.text_return_mode;
                }
            }
            ShellEvent::Tuck
                if !matches!(self.mode, ShellMode::Suspended | ShellMode::Recovering) =>
            {
                self.cancel_timer();
                self.mode = ShellMode::Tucked;
            }
            ShellEvent::Untuck if self.mode == ShellMode::Tucked && !self.fullscreen => {
                self.mode = ShellMode::Resting;
            }
            ShellEvent::Pin if self.mode == ShellMode::Expanded && !self.fullscreen => {
                self.mode = ShellMode::Pinned;
            }
            ShellEvent::Unpin if self.mode == ShellMode::Pinned && !self.fullscreen => {
                self.mode = ShellMode::Expanded;
            }
            ShellEvent::FullscreenChanged(true) if !self.fullscreen => {
                self.fullscreen = true;
                self.suspend_return_mode = if self.mode == ShellMode::Pinned {
                    ShellMode::Pinned
                } else {
                    ShellMode::Resting
                };
                self.power_suspended = false;
                self.cancel_timer();
                self.mode = ShellMode::Suspended;
            }
            ShellEvent::FullscreenChanged(false) if self.fullscreen => {
                self.fullscreen = false;
                self.mode = if self.host_failed {
                    ShellMode::Recovering
                } else if self.mode == ShellMode::Suspended {
                    self.suspend_return_mode
                } else {
                    self.mode
                };
            }
            ShellEvent::PowerSuspended if !self.fullscreen => {
                self.suspend_return_mode = if self.mode == ShellMode::Pinned {
                    ShellMode::Pinned
                } else {
                    ShellMode::Resting
                };
                self.cancel_timer();
                self.power_suspended = true;
                self.mode = ShellMode::Suspended;
            }
            ShellEvent::PowerResumed if self.power_suspended => {
                self.power_suspended = false;
                self.mode = if self.fullscreen {
                    ShellMode::Suspended
                } else if self.host_failed {
                    ShellMode::Recovering
                } else {
                    self.suspend_return_mode
                };
            }
            ShellEvent::HostFailed => {
                self.cancel_timer();
                self.host_failed = true;
                self.mode = ShellMode::Recovering;
            }
            ShellEvent::HostReady if self.host_failed => {
                self.host_failed = false;
                self.mode = if self.fullscreen || self.power_suspended {
                    ShellMode::Suspended
                } else {
                    ShellMode::Resting
                };
            }
            ShellEvent::TimerElapsed { generation } => {
                if let Some(timer) = self.timer
                    && timer.generation == generation
                    && now >= timer.deadline
                {
                    self.timer = None;
                    self.mode = match timer.target {
                        ShellTimerTarget::Expanded => ShellMode::Expanded,
                        ShellTimerTarget::Resting => ShellMode::Resting,
                    };
                }
            }
            _ => {}
        }
        before != self.snapshot()
    }

    fn schedule(&mut self, target: ShellTimerTarget, now: Instant, delay: Duration) {
        self.generation = self.generation.wrapping_add(1);
        self.timer = Some(ShellTimer {
            generation: self.generation,
            deadline: now + delay,
            target,
        });
        self.mode = ShellMode::HoverPending;
    }

    fn cancel_timer(&mut self) {
        if self.timer.take().is_some() {
            self.generation = self.generation.wrapping_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{ShellEvent, ShellMode, ShellReducer, ShellTimerTarget};
    use crate::motion::{HOVER_CLOSE_DELAY, HOVER_OPEN_DELAY};

    #[test]
    fn hover_open_uses_reference_delay_and_stale_timer_cannot_reopen() {
        let start = Instant::now();
        let mut shell = ShellReducer::default();
        assert!(shell.dispatch(ShellEvent::PointerEntered, start));
        let timer = shell.snapshot().timer.unwrap();
        assert_eq!(timer.target, ShellTimerTarget::Expanded);
        assert_eq!(timer.deadline, start + HOVER_OPEN_DELAY);

        assert!(shell.dispatch(ShellEvent::PointerLeft, start));
        assert_eq!(shell.snapshot().mode, ShellMode::Resting);
        assert!(!shell.dispatch(
            ShellEvent::TimerElapsed {
                generation: timer.generation,
            },
            start + HOVER_OPEN_DELAY,
        ));
        assert_eq!(shell.snapshot().mode, ShellMode::Resting);
    }

    #[test]
    fn click_mode_ignores_hover_and_obeys_the_selected_close_behavior() {
        let now = Instant::now();
        let mut shell = ShellReducer::default();
        shell.set_interaction_behavior(true, true);
        assert!(!shell.dispatch(ShellEvent::PointerEntered, now));
        assert_eq!(shell.snapshot().mode, ShellMode::Resting);

        shell.dispatch(ShellEvent::Open, now);
        assert!(!shell.dispatch(ShellEvent::PointerLeft, now));
        assert_eq!(shell.snapshot().mode, ShellMode::Expanded);
        shell.dispatch(ShellEvent::Close, now);
        assert_eq!(shell.snapshot().mode, ShellMode::Resting);

        shell.set_interaction_behavior(true, false);
        shell.dispatch(ShellEvent::Open, now);
        shell.dispatch(ShellEvent::PointerLeft, now);
        assert_eq!(shell.snapshot().timer.unwrap().target, ShellTimerTarget::Resting);
        shell.dispatch(ShellEvent::PointerEntered, now);
        assert_eq!(shell.snapshot().mode, ShellMode::Expanded);
        assert_eq!(shell.snapshot().timer, None);
    }

    #[test]
    fn close_hysteresis_is_cancelled_if_pointer_reenters() {
        let start = Instant::now();
        let mut shell = ShellReducer::default();
        shell.dispatch(ShellEvent::Open, start);
        shell.dispatch(ShellEvent::PointerLeft, start);
        let close_timer = shell.snapshot().timer.unwrap();
        assert_eq!(close_timer.target, ShellTimerTarget::Resting);
        assert_eq!(close_timer.deadline, start + HOVER_CLOSE_DELAY);

        shell.dispatch(
            ShellEvent::PointerEntered,
            start + Duration::from_millis(80),
        );
        assert_eq!(
            shell.snapshot().timer.unwrap().target,
            ShellTimerTarget::Expanded
        );
        assert!(!shell.dispatch(
            ShellEvent::TimerElapsed {
                generation: close_timer.generation,
            },
            start + HOVER_CLOSE_DELAY,
        ));
        assert_eq!(shell.snapshot().mode, ShellMode::HoverPending);
    }

    #[test]
    fn configurable_hover_delays_are_bounded_and_apply_to_future_events() {
        let start = Instant::now();
        let mut shell = ShellReducer::default();
        shell.set_hover_delays(Duration::ZERO, Duration::from_millis(2_000));
        shell.dispatch(ShellEvent::PointerEntered, start);
        let open_timer = shell.snapshot().timer.unwrap();
        assert_eq!(open_timer.deadline, start);
        shell.dispatch(
            ShellEvent::TimerElapsed {
                generation: open_timer.generation,
            },
            start,
        );

        shell.dispatch(ShellEvent::PointerLeft, start);
        let close_timer = shell.snapshot().timer.unwrap();
        assert_eq!(close_timer.deadline, start + Duration::from_millis(1_500));

        // Updating settings leaves the active close timer unchanged, but the
        // next pointer transition uses the newly clamped 600 ms open delay.
        shell.set_hover_delays(Duration::from_millis(1_000), Duration::ZERO);
        assert_eq!(shell.snapshot().timer, Some(close_timer));
        let reentered = start + Duration::from_millis(100);
        shell.dispatch(ShellEvent::PointerEntered, reentered);
        assert_eq!(
            shell.snapshot().timer.unwrap().deadline,
            reentered + Duration::from_millis(600)
        );
    }

    #[test]
    fn fullscreen_and_power_resume_return_to_a_safe_state_without_old_timers() {
        let start = Instant::now();
        let mut shell = ShellReducer::default();
        shell.dispatch(ShellEvent::PointerEntered, start);
        let old_timer = shell.snapshot().timer.unwrap();
        shell.dispatch(ShellEvent::FullscreenChanged(true), start);
        assert_eq!(shell.snapshot().mode, ShellMode::Suspended);
        assert_eq!(shell.snapshot().timer, None);
        shell.dispatch(ShellEvent::FullscreenChanged(false), start);
        assert_eq!(shell.snapshot().mode, ShellMode::Resting);
        assert!(!shell.dispatch(
            ShellEvent::TimerElapsed {
                generation: old_timer.generation,
            },
            start + HOVER_OPEN_DELAY,
        ));

        shell.dispatch(ShellEvent::Open, start);
        shell.dispatch(ShellEvent::PowerSuspended, start);
        shell.dispatch(ShellEvent::PowerResumed, start);
        assert_eq!(shell.snapshot().mode, ShellMode::Resting);
    }

    #[test]
    fn text_entry_and_pin_preserve_intended_open_state() {
        let now = Instant::now();
        let mut shell = ShellReducer::default();
        shell.dispatch(ShellEvent::Open, now);
        shell.dispatch(ShellEvent::Pin, now);
        shell.dispatch(ShellEvent::BeginTextEntry, now);
        assert_eq!(shell.snapshot().mode, ShellMode::TextEntry);
        shell.dispatch(ShellEvent::EndTextEntry, now);
        assert_eq!(shell.snapshot().mode, ShellMode::Pinned);
        shell.dispatch(ShellEvent::FullscreenChanged(true), now);
        shell.dispatch(ShellEvent::FullscreenChanged(false), now);
        assert_eq!(shell.snapshot().mode, ShellMode::Pinned);
    }

    #[test]
    fn tucked_shell_returns_on_edge_hover_and_uses_the_configured_delay() {
        let now = Instant::now();
        let mut shell = ShellReducer::default();
        shell.dispatch(ShellEvent::Open, now);
        shell.dispatch(ShellEvent::Tuck, now);
        assert_eq!(shell.snapshot().mode, ShellMode::Tucked);

        shell.dispatch(ShellEvent::PointerEntered, now);
        let timer = shell.snapshot().timer.unwrap();
        assert_eq!(timer.target, ShellTimerTarget::Expanded);
        assert_eq!(timer.deadline, now + HOVER_OPEN_DELAY);
        assert_eq!(shell.snapshot().mode, ShellMode::HoverPending);
        shell.dispatch(
            ShellEvent::TimerElapsed {
                generation: timer.generation,
            },
            timer.deadline,
        );
        assert_eq!(shell.snapshot().mode, ShellMode::Expanded);
    }

    #[test]
    fn toggle_open_closes_pinned_shell_and_preserves_text_entry_and_suspension() {
        let now = Instant::now();
        let mut shell = ShellReducer::default();
        shell.dispatch(ShellEvent::Open, now);
        shell.dispatch(ShellEvent::Pin, now);
        assert!(shell.dispatch(ShellEvent::ToggleOpen, now));
        assert_eq!(shell.snapshot().mode, ShellMode::Resting);

        shell.dispatch(ShellEvent::Open, now);
        shell.dispatch(ShellEvent::Pin, now);
        shell.dispatch(ShellEvent::Close, now);
        assert_eq!(shell.snapshot().mode, ShellMode::Resting);

        shell.dispatch(ShellEvent::Open, now);
        shell.dispatch(ShellEvent::BeginTextEntry, now);
        assert!(!shell.dispatch(ShellEvent::ToggleOpen, now));
        assert_eq!(shell.snapshot().mode, ShellMode::TextEntry);

        shell.dispatch(ShellEvent::PowerSuspended, now);
        assert!(!shell.dispatch(ShellEvent::ToggleOpen, now));
        assert_eq!(shell.snapshot().mode, ShellMode::Suspended);
    }

    #[test]
    fn failed_host_suspends_until_recovery_succeeds() {
        let now = Instant::now();
        let mut shell = ShellReducer::default();
        shell.dispatch(ShellEvent::HostFailed, now);
        assert_eq!(shell.snapshot().mode, ShellMode::Recovering);
        assert!(!shell.dispatch(ShellEvent::Open, now));
        shell.dispatch(ShellEvent::FullscreenChanged(true), now);
        shell.dispatch(ShellEvent::FullscreenChanged(false), now);
        assert_eq!(shell.snapshot().mode, ShellMode::Recovering);
        shell.dispatch(ShellEvent::HostReady, now);
        assert_eq!(shell.snapshot().mode, ShellMode::Resting);
    }
}
