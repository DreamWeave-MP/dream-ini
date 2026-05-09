// SPDX-License-Identifier: GPL-3.0-only

use std::thread;
use std::time::{Duration, Instant};

use super::FbVarScreeninfo;

pub(super) const REFRESH_ENV_VAR: &str = "DREAM_INI_PORTMASTER_REFRESH_HZ";
pub(super) const DEFAULT_REFRESH_HZ: u32 = 60;
pub(super) const MIN_REFRESH_HZ: u32 = 15;
pub(super) const MAX_REFRESH_HZ: u32 = 120;
pub(super) const NANOS_PER_SECOND: u64 = 1_000_000_000;
pub(super) const PICOS_PER_SECOND: u128 = 1_000_000_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SelectedRefreshRate {
    pub(super) hz: u32,
    pub(super) source: RefreshRateSource,
}

impl SelectedRefreshRate {
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
    var: &FbVarScreeninfo,
) -> SelectedRefreshRate {
    if let Some(hz) = parse_refresh_env_value(env_value) {
        return SelectedRefreshRate {
            hz,
            source: RefreshRateSource::Environment,
        };
    }
    if let Some(hz) = framebuffer_refresh_hz(var) {
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

pub(super) fn parse_refresh_env_value(value: Option<&str>) -> Option<u32> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    let parsed = value.parse::<u32>().ok()?;
    Some(parsed.clamp(MIN_REFRESH_HZ, MAX_REFRESH_HZ))
}

pub(super) fn framebuffer_refresh_hz(var: &FbVarScreeninfo) -> Option<u32> {
    if var.pixclock == 0 {
        return None;
    }
    let htotal =
        checked_timing_total(var.xres, [var.left_margin, var.right_margin, var.hsync_len])?;
    let vtotal = checked_timing_total(
        var.yres,
        [var.upper_margin, var.lower_margin, var.vsync_len],
    )?;

    let frame_picos = u128::from(var.pixclock)
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

pub(super) fn checked_timing_total(required: u32, optional: [u32; 3]) -> Option<u32> {
    if required == 0 {
        return None;
    }
    let total = optional.into_iter().try_fold(required, u32::checked_add)?;
    (total != 0).then_some(total)
}

pub(super) fn frame_pacing_after_frame(
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
