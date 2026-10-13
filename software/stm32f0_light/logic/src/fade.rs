//! 亮度渐变（FR-10）：1 s 线性，仅亮度维度。

/// 渐变时长（ms）。
pub const FADE_DURATION_MS: u32 = 1_000;

/// 渐变状态机。
pub struct FadeState {
    start_value: u8,
    target_value: u8,
    start_ms: u32,
    active: bool,
}

impl FadeState {
    /// 创建未激活状态。
    pub const fn new() -> Self {
        FadeState {
            start_value: 0,
            target_value: 0,
            start_ms: 0,
            active: false,
        }
    }

    /// 发起从 `from` 到 `to` 的 1 s 线性渐变。
    pub fn start(&mut self, from: u8, to: u8, now_ms: u32) {
        self.start_value = from.min(100);
        self.target_value = to.min(100);
        self.start_ms = now_ms;
        self.active = true;
    }

    /// 推进渐变；完成后返回目标值并保持非激活。
    pub fn update(&mut self, now_ms: u32) -> u8 {
        if !self.active {
            return self.target_value;
        }
        let elapsed = now_ms.wrapping_sub(self.start_ms);
        if elapsed >= FADE_DURATION_MS {
            self.active = false;
            return self.target_value;
        }
        let from = self.start_value as i32;
        let to = self.target_value as i32;
        (from + (to - from) * elapsed as i32 / FADE_DURATION_MS as i32).clamp(0, 100) as u8
    }

    /// 渐变是否进行中。
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// 渐变目标值。
    pub fn target(&self) -> u8 {
        self.target_value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_up_and_completion() {
        let mut f = FadeState::new();
        f.start(0, 100, 1_000);
        assert!(f.is_active());
        assert_eq!(f.update(1_000), 0);
        assert_eq!(f.update(1_500), 50);
        assert_eq!(f.update(1_999), 99);
        assert_eq!(f.update(2_000), 100);
        assert!(!f.is_active());
        // 完成后保持目标值。
        assert_eq!(f.update(3_000), 100);
    }

    #[test]
    fn linear_down() {
        let mut f = FadeState::new();
        f.start(80, 20, 0);
        assert_eq!(f.update(500), 50);
        assert_eq!(f.update(1_000), 20);
        assert!(!f.is_active());
    }

    #[test]
    fn zero_to_zero_stays_zero() {
        let mut f = FadeState::new();
        f.start(0, 0, 0);
        assert_eq!(f.update(10), 0);
    }

    #[test]
    fn restart_overrides_previous() {
        let mut f = FadeState::new();
        f.start(0, 100, 0);
        assert_eq!(f.update(500), 50);
        f.start(50, 0, 500);
        assert_eq!(f.update(1_000), 25);
        assert_eq!(f.update(1_500), 0);
    }
}
