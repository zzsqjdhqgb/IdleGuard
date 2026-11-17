//! Pure timing rules shared by the background timer and the overlay UI.
//!
//! The module intentionally has no Win32 dependencies so the product rules can
//! be tested on every development platform.

use std::time::Duration;

use crate::config::Settings;

/// What the floating overlay should show for the current idle duration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OverlayPhase {
    /// No overlay.
    Hidden,
    /// The reminder threshold has been reached, but the countdown has not.
    Reminder,
    /// The countdown has started. `remaining_seconds` is rounded up so the UI
    /// never displays 00:00 before shutdown has actually been initiated.
    Countdown {
        remaining_seconds: u64,
        progress: f32,
    },
}

/// Result of evaluating the configured thresholds against an idle duration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimerState {
    pub phase: OverlayPhase,
    pub shutdown_due: bool,
}

/// The timer starts at application launch, not at the last input that may have
/// happened before the application was started. Once the user interacts again,
/// the real last-input time is newer than the launch time and becomes the
/// origin automatically.
pub fn effective_idle(raw_idle: Duration, since_launch: Duration) -> Duration {
    raw_idle.min(since_launch)
}

/// Compute the visible phase and whether shutdown is due.
///
/// Rule precedence matters: once the countdown has started it stays visible
/// even if the reminder threshold is reached later. Checking the countdown
/// first implements that ordering. A reminder threshold equal to or earlier
/// than the countdown start simply shows the reminder first and switches to the
/// countdown when the countdown threshold is reached.
pub fn evaluate(settings: &Settings, idle: Duration) -> TimerState {
    let shutdown_ms = settings.shutdown_seconds.saturating_mul(1000);
    let countdown_ms = settings.countdown_seconds.saturating_mul(1000);
    let reminder_ms = settings.reminder_seconds.saturating_mul(1000);
    let idle_ms = u64::try_from(idle.as_millis()).unwrap_or(u64::MAX);

    if idle_ms >= shutdown_ms {
        let phase = if idle_ms >= shutdown_ms.saturating_sub(countdown_ms) {
            OverlayPhase::Countdown {
                remaining_seconds: 0,
                progress: 0.0,
            }
        } else {
            OverlayPhase::Reminder
        };
        return TimerState {
            phase,
            shutdown_due: true,
        };
    }

    let remaining_ms = shutdown_ms - idle_ms;
    let countdown_start_ms = shutdown_ms.saturating_sub(countdown_ms);

    let phase = if idle_ms >= countdown_start_ms {
        let progress = if countdown_ms == 0 {
            0.0
        } else {
            (remaining_ms as f64 / countdown_ms as f64).clamp(0.0, 1.0) as f32
        };
        OverlayPhase::Countdown {
            remaining_seconds: remaining_ms.div_ceil(1000),
            progress,
        }
    } else if idle_ms >= reminder_ms {
        OverlayPhase::Reminder
    } else {
        OverlayPhase::Hidden
    };

    TimerState {
        phase,
        shutdown_due: false,
    }
}

/// Time until the next configured threshold, used to avoid sleeping through a
/// short reminder/countdown window while the overlay is hidden.
pub fn next_deadline_delay(settings: &Settings, idle: Duration) -> Option<Duration> {
    let idle_ms = u64::try_from(idle.as_millis()).unwrap_or(u64::MAX);
    let shutdown_ms = settings.shutdown_seconds.saturating_mul(1000);
    let countdown_start_ms =
        shutdown_ms.saturating_sub(settings.countdown_seconds.saturating_mul(1000));
    let reminder_ms = settings.reminder_seconds.saturating_mul(1000);
    let next = [reminder_ms, countdown_start_ms, shutdown_ms]
        .into_iter()
        .filter(|&deadline| deadline > idle_ms)
        .min()?;
    Some(Duration::from_millis(next - idle_ms))
}

/// Format seconds as `HH:MM:SS` (hours may exceed two digits).
pub fn format_hms(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    format!("{hours:02}:{minutes:02}:{secs:02}")
}

/// Format a countdown compactly: `M:SS` below an hour, otherwise `H:MM:SS`.
pub fn format_countdown(seconds: u64) -> String {
    if seconds >= 3600 {
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        let secs = seconds % 60;
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{:02}:{:02}", seconds / 60, seconds % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(shutdown: u64, reminder: u64, countdown: u64) -> Settings {
        Settings {
            shutdown_seconds: shutdown,
            reminder_seconds: reminder,
            countdown_seconds: countdown,
            auto_start: false,
        }
    }

    #[test]
    fn hidden_before_reminder() {
        let state = evaluate(&settings(60, 20, 10), Duration::from_secs(19));
        assert_eq!(state.phase, OverlayPhase::Hidden);
        assert!(!state.shutdown_due);
    }

    #[test]
    fn reminder_appears_at_its_threshold() {
        let state = evaluate(&settings(60, 20, 10), Duration::from_secs(20));
        assert_eq!(state.phase, OverlayPhase::Reminder);
        assert!(!state.shutdown_due);
    }

    #[test]
    fn countdown_starts_at_shutdown_minus_countdown() {
        let state = evaluate(&settings(60, 20, 10), Duration::from_secs(50));
        assert_eq!(
            state.phase,
            OverlayPhase::Countdown {
                remaining_seconds: 10,
                progress: 1.0
            }
        );
    }

    #[test]
    fn countdown_takes_precedence_over_a_later_reminder() {
        // countdown starts at 50s, reminder only at 55s.
        let at_countdown = evaluate(&settings(60, 55, 10), Duration::from_secs(50));
        assert!(matches!(
            at_countdown.phase,
            OverlayPhase::Countdown {
                remaining_seconds: 10,
                ..
            }
        ));
        let after_reminder = evaluate(&settings(60, 55, 10), Duration::from_secs(56));
        assert!(matches!(
            after_reminder.phase,
            OverlayPhase::Countdown {
                remaining_seconds: 4,
                ..
            }
        ));
    }

    #[test]
    fn countdown_appears_before_a_later_reminder() {
        let state = evaluate(&settings(60, 50, 30), Duration::from_secs(29));
        assert_eq!(state.phase, OverlayPhase::Hidden);
        let state = evaluate(&settings(60, 50, 30), Duration::from_secs(30));
        assert!(matches!(
            state.phase,
            OverlayPhase::Countdown {
                remaining_seconds: 30,
                ..
            }
        ));
    }

    #[test]
    fn shutdown_is_due_at_the_shutdown_threshold() {
        let state = evaluate(&settings(60, 20, 10), Duration::from_secs(60));
        assert!(state.shutdown_due);
        assert_eq!(
            state.phase,
            OverlayPhase::Countdown {
                remaining_seconds: 0,
                progress: 0.0
            }
        );
    }

    #[test]
    fn remaining_seconds_round_up() {
        let state = evaluate(&settings(60, 1, 30), Duration::from_millis(50_500));
        assert!(matches!(
            state.phase,
            OverlayPhase::Countdown {
                remaining_seconds: 10,
                ..
            }
        ));
    }

    #[test]
    fn next_deadline_tracks_short_windows() {
        let settings = settings(60, 20, 10);
        assert_eq!(
            next_deadline_delay(&settings, Duration::from_secs(19)),
            Some(Duration::from_secs(1))
        );
        assert_eq!(
            next_deadline_delay(&settings, Duration::from_secs(49)),
            Some(Duration::from_secs(1))
        );
        assert_eq!(
            next_deadline_delay(&settings, Duration::from_secs(0)),
            Some(Duration::from_secs(20))
        );
    }

    #[test]
    fn effective_idle_starts_at_launch() {
        assert_eq!(
            effective_idle(Duration::from_secs(3600), Duration::from_secs(2)),
            Duration::from_secs(2)
        );
        assert_eq!(
            effective_idle(Duration::from_secs(1), Duration::from_secs(20)),
            Duration::from_secs(1)
        );
    }

    #[test]
    fn formats_durations() {
        assert_eq!(format_hms(0), "00:00:00");
        assert_eq!(format_hms(3661), "01:01:01");
        assert_eq!(format_hms(100 * 3600), "100:00:00");
        assert_eq!(format_countdown(65), "01:05");
        assert_eq!(format_countdown(3661), "1:01:01");
    }
}
