// SPDX-License-Identifier: GPL-3.0-only

use std::thread;
use std::time::{Duration, Instant};

pub(super) const REFRESH_ENV_VAR: &str = "DREAM_INI_PORTMASTER_REFRESH_HZ";
const DEFAULT_REFRESH_HZ: u32 = 60;
const MIN_REFRESH_HZ: u32 = 15;
const MAX_REFRESH_HZ: u32 = 120;
const NANOS_PER_SECOND: u64 = 1_000_000_000;
const PICOS_PER_SECOND: u128 = 1_000_000_000_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct DisplayTiming {
    pixclock: u32,
    xres: u32,
    yres: u32,
    left_margin: u32,
    right_margin: u32,
    hsync_len: u32,
    upper_margin: u32,
    lower_margin: u32,
    vsync_len: u32,
}

impl DisplayTiming {
    pub(super) const fn new(
        pixclock: u32,
        xres: u32,
        yres: u32,
        horizontal: [u32; 3],
        vertical: [u32; 3],
    ) -> Self {
        Self {
            pixclock,
            xres,
            yres,
            left_margin: horizontal[0],
            right_margin: horizontal[1],
            hsync_len: horizontal[2],
            upper_margin: vertical[0],
            lower_margin: vertical[1],
            vsync_len: vertical[2],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SelectedRefreshRate {
    hz: u32,
    pub(super) source: RefreshRateSource,
}

impl SelectedRefreshRate {
    pub(super) const fn hz(self) -> u32 {
        self.hz
    }

    pub(super) fn frame_interval(self) -> Duration {
        Duration::from_nanos(NANOS_PER_SECOND / u64::from(self.hz))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RefreshRateSource {
    Environment,
    Framebuffer,
    Default,
}

impl RefreshRateSource {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Environment => "environment",
            Self::Framebuffer => "framebuffer",
            Self::Default => "default",
        }
    }
}

pub(super) fn select_refresh_rate(
    env_value: Option<&str>,
    timing: DisplayTiming,
) -> SelectedRefreshRate {
    if let Some(hz) = parse_refresh_env_value(env_value) {
        return SelectedRefreshRate {
            hz,
            source: RefreshRateSource::Environment,
        };
    }
    if let Some(hz) = framebuffer_refresh_hz(timing) {
        return SelectedRefreshRate {
            hz,
            source: RefreshRateSource::Framebuffer,
        };
    }
    SelectedRefreshRate {
        hz: DEFAULT_REFRESH_HZ,
        source: RefreshRateSource::Default,
    }
}

fn parse_refresh_env_value(value: Option<&str>) -> Option<u32> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    let parsed = value.parse::<u32>().ok()?;
    Some(parsed.clamp(MIN_REFRESH_HZ, MAX_REFRESH_HZ))
}

fn framebuffer_refresh_hz(timing: DisplayTiming) -> Option<u32> {
    if timing.pixclock == 0 {
        return None;
    }
    let htotal = checked_timing_total(
        timing.xres,
        [timing.left_margin, timing.right_margin, timing.hsync_len],
    )?;
    let vtotal = checked_timing_total(
        timing.yres,
        [timing.upper_margin, timing.lower_margin, timing.vsync_len],
    )?;

    let frame_picos = u128::from(timing.pixclock)
        .checked_mul(u128::from(htotal))?
        .checked_mul(u128::from(vtotal))?;
    let rounded = PICOS_PER_SECOND
        .checked_add(frame_picos / 2)?
        .checked_div(frame_picos)?;
    let hz = u32::try_from(rounded).ok()?;
    (MIN_REFRESH_HZ..=MAX_REFRESH_HZ)
        .contains(&hz)
        .then_some(hz)
}

fn checked_timing_total(required: u32, optional: [u32; 3]) -> Option<u32> {
    if required == 0 {
        return None;
    }
    let total = optional.into_iter().try_fold(required, u32::checked_add)?;
    (total != 0).then_some(total)
}

fn frame_pacing_after_frame(
    now: Instant,
    previous_deadline: Instant,
    interval: Duration,
) -> (Instant, Option<Duration>) {
    let deadline = previous_deadline.checked_add(interval).unwrap_or(now);
    if deadline > now {
        (deadline, Some(deadline.duration_since(now)))
    } else {
        (now, None)
    }
}

pub(super) fn sleep_after_frame(
    now: Instant,
    previous_deadline: Instant,
    interval: Duration,
) -> Instant {
    let (next_deadline, sleep_for) = frame_pacing_after_frame(now, previous_deadline, interval);
    if let Some(sleep_for) = sleep_for {
        thread::sleep(sleep_for);
    }
    next_deadline
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_env_parser_accepts_clamps_and_ignores_invalid_values() {
        assert_eq!(parse_refresh_env_value(Some("75")), Some(75));
        assert_eq!(parse_refresh_env_value(Some("1")), Some(MIN_REFRESH_HZ));
        assert_eq!(parse_refresh_env_value(Some("240")), Some(MAX_REFRESH_HZ));
        assert_eq!(parse_refresh_env_value(Some("-1")), None);
        assert_eq!(parse_refresh_env_value(Some("fast")), None);
        assert_eq!(parse_refresh_env_value(Some("")), None);
    }

    #[test]
    fn framebuffer_refresh_calculation_rounds_representative_timing_to_60_hz() {
        let timing = refresh_test_timing(39_721);

        assert_eq!(framebuffer_refresh_hz(timing), Some(60));
    }

    #[test]
    fn framebuffer_refresh_calculation_accepts_zero_margin_and_sync_lengths() {
        let timing = DisplayTiming {
            pixclock: 54_253,
            xres: 640,
            yres: 480,
            ..Default::default()
        };

        assert_eq!(framebuffer_refresh_hz(timing), Some(60));
    }

    #[test]
    fn framebuffer_refresh_calculation_ignores_unusable_values() {
        let zero_pixclock = refresh_test_timing(0);
        let out_of_range = refresh_test_timing(1);

        assert_eq!(framebuffer_refresh_hz(zero_pixclock), None);
        assert_eq!(
            framebuffer_refresh_hz(DisplayTiming {
                xres: 0,
                ..refresh_test_timing(39_721)
            }),
            None
        );
        assert_eq!(
            framebuffer_refresh_hz(DisplayTiming {
                yres: 0,
                ..refresh_test_timing(39_721)
            }),
            None
        );
        assert_eq!(framebuffer_refresh_hz(out_of_range), None);
    }

    #[test]
    fn refresh_selection_priority_uses_env_framebuffer_then_default() {
        let framebuffer_timing = refresh_test_timing(39_721);
        let unusable_timing = DisplayTiming::default();

        assert_eq!(
            select_refresh_rate(Some("30"), framebuffer_timing),
            SelectedRefreshRate {
                hz: 30,
                source: RefreshRateSource::Environment,
            }
        );
        assert_eq!(
            select_refresh_rate(Some("fast"), framebuffer_timing),
            SelectedRefreshRate {
                hz: 60,
                source: RefreshRateSource::Framebuffer,
            }
        );
        assert_eq!(
            select_refresh_rate(None, unusable_timing),
            SelectedRefreshRate {
                hz: DEFAULT_REFRESH_HZ,
                source: RefreshRateSource::Default,
            }
        );
    }

    #[test]
    fn refresh_frame_interval_for_60_hz_uses_integer_nanoseconds() {
        let selected = SelectedRefreshRate {
            hz: 60,
            source: RefreshRateSource::Default,
        };

        assert_eq!(selected.frame_interval(), Duration::from_nanos(16_666_666));
    }

    #[test]
    fn frame_pacing_sleeps_when_ahead_and_resets_when_late() {
        let previous_deadline = Instant::now();
        let interval = Duration::from_millis(16);
        let ahead_now = previous_deadline + Duration::from_millis(10);

        let (next_deadline, sleep_for) =
            frame_pacing_after_frame(ahead_now, previous_deadline, interval);

        assert_eq!(next_deadline, previous_deadline + interval);
        assert_eq!(sleep_for, Some(Duration::from_millis(6)));

        let late_now = previous_deadline + Duration::from_millis(20);
        let (next_deadline, sleep_for) =
            frame_pacing_after_frame(late_now, previous_deadline, interval);

        assert_eq!(next_deadline, late_now);
        assert_eq!(sleep_for, None);
    }

    fn refresh_test_timing(pixclock: u32) -> DisplayTiming {
        DisplayTiming {
            pixclock,
            xres: 640,
            left_margin: 48,
            right_margin: 16,
            hsync_len: 96,
            yres: 480,
            upper_margin: 10,
            lower_margin: 33,
            vsync_len: 2,
        }
    }
}
