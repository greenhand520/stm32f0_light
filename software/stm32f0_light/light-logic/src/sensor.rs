//! 传感器换算与阈值（FR-6/FR-7）。
//!
//! - 电池：PA1，100K/47K 分压，$V_{bat} = ADC \\times 3.3 / 4095 \\times 3.13$；
//! - NTC：PA3，100K 固定电阻上拉 + 100K B3950 下拉，
//!   $R_{NTC} = ADC \\times 100000 / (4095 - ADC)$（Ω）；
//! - 光敏：PA2，暗环境判定阈值。

/// 暗环境 ADC 阈值（高于即判暗，与 C `LIGHT_THRESHOLD` 一致）。
pub const LIGHT_THRESHOLD: u16 = 4000;

/// 电量分档阈值（V）：>3.7 绿，>3.3 黄，>2.7 红，<2.7 低电（FR-6.2、§11 决议）。
pub const BAT_GREEN: f32 = 3.7;
pub const BAT_YELLOW: f32 = 3.3;
pub const BAT_RED: f32 = 2.7;
/// 低电闪烁解除阈值（V，D2 迟滞）。
pub const BAT_LOW_CLEAR: f32 = 3.1;

/// NTC 过热阈值（Ω，≤ 判过热 ≈75 ℃，Q4 修复单位）。
pub const NTC_OVERHEAT_OHM: u32 = 15_000;
/// NTC 恢复阈值（Ω，≥ 判正常 ≈60 ℃）。
pub const NTC_RECOVER_OHM: u32 = 25_000;

/// ADC 满量程。
const ADC_FULL: u16 = 4095;
/// NTC 换算开路上限（C 版返回值）。
const NTC_OPEN_OHM: u32 = 999_999;

/// 电量档位。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BatteryLevel {
    /// > 3.7 V。
    Green,
    /// > 3.3 V。
    Yellow,
    /// > 2.7 V。
    Red,
    /// < 2.7 V（低电流程）。
    Critical,
}

/// ADC 均值 → 电池电压（V）。
pub fn battery_voltage(adc: u16) -> f32 {
    let pin = adc as f32 * 3.3 / ADC_FULL as f32;
    pin * ((100.0 + 47.0) / 47.0)
}

/// ADC 均值 → NTC 阻值（Ω）。边界与 C 一致：0 → 0，满量程 → 999999。
pub fn ntc_resistance(adc: u16) -> u32 {
    if adc >= ADC_FULL {
        return NTC_OPEN_OHM;
    }
    if adc == 0 {
        return 0;
    }
    adc as u32 * 100_000 / (ADC_FULL - adc) as u32
}

/// 光敏 ADC 均值 → 是否暗环境。
pub fn is_dark(adc: u16) -> bool {
    adc > LIGHT_THRESHOLD
}

/// 电压 → 电量档位（档位间严格 `>`；2.7 V 恰值归 Red，仅 `< 2.7 V` 触发低电，D2）。
pub fn battery_level(v: f32) -> BatteryLevel {
    if v > BAT_GREEN {
        BatteryLevel::Green
    } else if v > BAT_YELLOW {
        BatteryLevel::Yellow
    } else if v >= BAT_RED {
        BatteryLevel::Red
    } else {
        BatteryLevel::Critical
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voltage_conversion_known_points() {
        // ADC=4095 → 3.3 V 引脚 ×3.13 ≈ 10.33 V（量程上限）。
        let v = battery_voltage(4095);
        assert!((v - 3.3 * 3.13).abs() < 0.01, "{}", v);
        // ADC=2048 ≈ 半量程。
        let v = battery_voltage(2048);
        assert!((v - 2048.0 * 3.3 / 4095.0 * (147.0 / 47.0)).abs() < 0.001, "{}", v);
        assert_eq!(battery_voltage(0), 0.0);
    }

    #[test]
    fn ntc_resistance_boundaries() {
        assert_eq!(ntc_resistance(0), 0);
        assert_eq!(ntc_resistance(4095), 999_999);
        // 已知点：ADC=1024 → 1024*100000/3071 ≈ 33344 Ω。
        assert_eq!(ntc_resistance(1024), 1024 * 100_000 / 3071);
        // B3950 75 ℃ ≈ 15 kΩ 落在合理 ADC 区间（约 30% 分压）。
        let adc_15k = (15_000u32 * 4095 / (100_000 + 15_000)) as u16;
        assert!(ntc_resistance(adc_15k) > 14_000 && ntc_resistance(adc_15k) < 16_000);
    }

    #[test]
    fn dark_threshold() {
        assert!(!is_dark(LIGHT_THRESHOLD));
        assert!(is_dark(LIGHT_THRESHOLD + 1));
    }

    #[test]
    fn battery_levels_at_thresholds() {
        assert_eq!(battery_level(4.0), BatteryLevel::Green);
        assert_eq!(battery_level(3.71), BatteryLevel::Green);
        assert_eq!(battery_level(3.7), BatteryLevel::Yellow);
        assert_eq!(battery_level(3.31), BatteryLevel::Yellow);
        assert_eq!(battery_level(3.3), BatteryLevel::Red);
        assert_eq!(battery_level(2.71), BatteryLevel::Red);
        assert_eq!(battery_level(2.7), BatteryLevel::Red);
        assert_eq!(battery_level(2.69), BatteryLevel::Critical);
    }
}
