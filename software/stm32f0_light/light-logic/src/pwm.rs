//! 冷暖双通道 PWM 映射与亮度/色温钳位（FR-2）。
//!
//! 暖光 = TIM14_CH1（PA7），冷光 = TIM1_CH2（PA9）。
//! 公式与 C 固件逐级整数截断完全一致：
//! `warm = bright*cct/100*max/100`，`cold = bright*(100-cct)/100*max/100`。

/// 定时器重装值（ARR），48 MHz / 4800 = 10 kHz。
pub const PWM_PERIOD: u16 = 4799;

/// 亮度下限/上限（FR-2.2）。
pub const BRIGHTNESS_MIN: u8 = 5;
pub const BRIGHTNESS_MAX: u8 = 100;

/// 计算（暖, 冷）占空比。
///
/// `bright` 钳位到 0..=100，`cct` 钳位到 0..=100（0 = 全冷，100 = 全暖）。
/// 截断顺序与 C 版一致，保证同一输入产生同一占空比。
pub fn duty_cycles(bright: u8, cct: u8, max_duty: u16) -> (u16, u16) {
    let b = bright.min(100) as u32;
    let c = cct.min(100) as u32;
    let m = max_duty as u32;
    let warm = b * c / 100 * m / 100;
    let cold = b * (100 - c) / 100 * m / 100;
    (warm as u16, cold as u16)
}

/// 亮度钳位（5..=100，Q3 修复后与色温分开设定范围）。
pub fn clamp_brightness(v: i16) -> u8 {
    v.clamp(BRIGHTNESS_MIN as i16, BRIGHTNESS_MAX as i16) as u8
}

/// 色温钳位（0..=100，Q3 修复：不再被下限 5 截断）。
pub fn clamp_cct(v: i16) -> u8 {
    v.clamp(0, 100) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 与 C 公式逐级截断一致的参考实现（测试内独立推导）。
    fn c_ref(b: u8, c: u8, max: u16) -> (u32, u32) {
        let b = b as u32;
        let c = c as u32;
        let max = max as u32;
        ((b * c / 100) * max / 100, (b * (100 - c) / 100) * max / 100)
    }

    #[test]
    fn matches_c_truncation() {
        for &(b, c) in &[(5u8, 50u8), (37, 61), (100, 100), (100, 0), (5, 100), (99, 33)] {
            let (w, k) = duty_cycles(b, c, PWM_PERIOD);
            let (wr, kr) = c_ref(b, c, PWM_PERIOD);
            assert_eq!((w as u32, k as u32), (wr, kr), "b={} c={}", b, c);
        }
        // 已知样例：b=5, c=50 → 5*50/100=2；2*4799/100=95。
        assert_eq!(duty_cycles(5, 50, PWM_PERIOD), (95, 95));
    }

    #[test]
    fn full_warm_and_full_cold() {
        let (w, k) = duty_cycles(100, 100, PWM_PERIOD);
        assert_eq!(k, 0);
        assert_eq!(w, 4799);
        let (w, k) = duty_cycles(100, 0, PWM_PERIOD);
        assert_eq!(w, 0);
        assert_eq!(k, 4799);
    }

    #[test]
    fn out_of_range_inputs_clamped() {
        // bright > 100 / cct > 100 不应 panic 或下溢。
        assert_eq!(duty_cycles(255, 255, PWM_PERIOD), duty_cycles(100, 100, PWM_PERIOD));
    }

    #[test]
    fn clamps() {
        assert_eq!(clamp_brightness(-3), BRIGHTNESS_MIN);
        assert_eq!(clamp_brightness(0), BRIGHTNESS_MIN);
        assert_eq!(clamp_brightness(101), BRIGHTNESS_MAX);
        assert_eq!(clamp_brightness(7), 7);
        // 色温允许到 0（Q3 修复点）。
        assert_eq!(clamp_cct(0), 0);
        assert_eq!(clamp_cct(-5), 0);
        assert_eq!(clamp_cct(101), 100);
    }
}
