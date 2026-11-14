//! Thermal and battery policy.
//!
//! The core samples the machine's thermal state and power source and turns them into one policy that every part
//! of the app follows. The recording itself is never throttled: a dropped frame in a take is worse than a warm
//! laptop. Everything else (preview frame rate, background jobs, interface motion) gives way first.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The operating system's view of how hot the machine is.
/// Mirrors `NSProcessInfoThermalState` on macOS; Windows readings are mapped onto the same scale.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ThermalState {
    #[default]
    Nominal,
    Fair,
    Serious,
    Critical,
}

/// What the user chose in Settings › Performance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LowPowerSetting {
    /// Shotmaxxing decides from thermal state and battery.
    #[default]
    Automatic,
    AlwaysOn,
    AlwaysOff,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PowerInputs {
    pub thermal: ThermalState,
    pub on_battery: bool,
    /// 0 to 100, when known.
    pub battery_percent: Option<u8>,
    /// macOS Low Power Mode or Windows Energy Saver.
    pub os_low_power: bool,
    pub setting: LowPowerSetting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MotionBudget {
    /// The chosen motion level ("alive but calm").
    Full,
    /// Fades only.
    Fades,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PowerPolicy {
    pub low_power: bool,
    /// Why low power is on, shown in Settings and the status menu.
    pub reason: Option<PowerReason>,
    /// Highest frame rate the editor preview may draw at.
    pub preview_fps_cap: u32,
    /// Background analysis, thumbnails and proxies may run.
    pub background_jobs: bool,
    /// Proxy files may be generated for heavy sources.
    pub proxies: bool,
    pub motion: MotionBudget,
    /// Scale applied to the preview's render resolution (1.0 = full backing resolution).
    pub preview_resolution_scale: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PowerReason {
    Setting,
    Hot,
    VeryHot,
    LowBattery,
    OnBattery,
    SystemLowPower,
}

impl PowerReason {
    pub fn describe(self) -> &'static str {
        match self {
            Self::Setting => "Low power is on in Settings",
            Self::Hot => "Your computer is warm, so Shotmaxxing is going easy on it",
            Self::VeryHot => "Your computer is hot, so previews and background work are paused",
            Self::LowBattery => "Battery is low, so Shotmaxxing is saving power",
            Self::OnBattery => "On battery, so Shotmaxxing is saving power",
            Self::SystemLowPower => "Your system is in low power mode",
        }
    }
}

impl PowerPolicy {
    pub const FULL: Self = Self {
        low_power: false,
        reason: None,
        preview_fps_cap: 60,
        background_jobs: true,
        proxies: true,
        motion: MotionBudget::Full,
        preview_resolution_scale: 1.0,
    };

    pub fn decide(inputs: PowerInputs) -> Self {
        let reason = match inputs.setting {
            LowPowerSetting::AlwaysOff => None,
            LowPowerSetting::AlwaysOn => Some(PowerReason::Setting),
            LowPowerSetting::Automatic => {
                if inputs.thermal >= ThermalState::Critical || inputs.thermal == ThermalState::Serious {
                    Some(if inputs.thermal == ThermalState::Critical {
                        PowerReason::VeryHot
                    } else {
                        PowerReason::Hot
                    })
                } else if inputs.os_low_power {
                    Some(PowerReason::SystemLowPower)
                } else if inputs.on_battery && inputs.battery_percent.is_some_and(|p| p <= 20) {
                    Some(PowerReason::LowBattery)
                } else if inputs.on_battery {
                    Some(PowerReason::OnBattery)
                } else {
                    None
                }
            }
        };

        let Some(reason) = reason else {
            // Even with low power off, a fair thermal state trims the preview a little.
            let mut policy = Self::FULL;
            if inputs.thermal == ThermalState::Fair {
                policy.preview_fps_cap = 50;
            }
            return policy;
        };

        let severe = matches!(reason, PowerReason::VeryHot);
        Self {
            low_power: true,
            reason: Some(reason),
            preview_fps_cap: if severe { 24 } else { 30 },
            background_jobs: !severe && !matches!(reason, PowerReason::Hot | PowerReason::LowBattery),
            proxies: false,
            motion: MotionBudget::Fades,
            preview_resolution_scale: if severe { 0.5 } else { 0.75 },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugged_in_and_cool_is_full_speed() {
        assert_eq!(PowerPolicy::decide(PowerInputs::default()), PowerPolicy::FULL);
    }

    #[test]
    fn heat_pauses_background_work() {
        let p = PowerPolicy::decide(PowerInputs { thermal: ThermalState::Serious, ..Default::default() });
        assert!(p.low_power);
        assert!(!p.background_jobs);
        assert_eq!(p.motion, MotionBudget::Fades);
        assert_eq!(p.reason, Some(PowerReason::Hot));

        let critical =
            PowerPolicy::decide(PowerInputs { thermal: ThermalState::Critical, ..Default::default() });
        assert_eq!(critical.preview_fps_cap, 24);
        assert_eq!(critical.reason, Some(PowerReason::VeryHot));
    }

    #[test]
    fn battery_saves_power_but_keeps_jobs_unless_low() {
        let battery = PowerPolicy::decide(PowerInputs {
            on_battery: true,
            battery_percent: Some(80),
            ..Default::default()
        });
        assert!(battery.low_power && battery.background_jobs);
        let low = PowerPolicy::decide(PowerInputs {
            on_battery: true,
            battery_percent: Some(12),
            ..Default::default()
        });
        assert!(!low.background_jobs);
    }

    #[test]
    fn the_user_setting_wins() {
        let off = PowerPolicy::decide(PowerInputs {
            thermal: ThermalState::Critical,
            setting: LowPowerSetting::AlwaysOff,
            ..Default::default()
        });
        assert!(!off.low_power);
        let on =
            PowerPolicy::decide(PowerInputs { setting: LowPowerSetting::AlwaysOn, ..Default::default() });
        assert_eq!(on.reason, Some(PowerReason::Setting));
    }
}
