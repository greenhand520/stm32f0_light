//! 按键状态机（纯逻辑，10 ms 调用一次）。
//!
//! 语义（`docs/design.md` §2.1）：
//! - 去抖：电平连续 [`DEBOUNCE_TICKS`] 个 tick 相同才确认；
//! - [`KeyEvent::LongPress`]：按住第 [`LONG_PRESS_TICKS`] 个确认 tick 产生一次；
//! - [`KeyEvent::LongHold`]：长按后每 [`LONG_HOLD_REPEAT_TICKS`] 个 tick 重复一次，直至释放；
//! - [`KeyEvent::Click`]：短按释放后等双击窗口 [`DOUBLE_CLICK_TICKS`] 超时才上报；
//! - [`KeyEvent::DoubleClick`]：窗口内第二次短按释放，产生 [`KeyEvent::DoubleClick`]
//!   （不附带 [`KeyEvent::Click`]）。
//!
//! 同一 tick 多个事件时后产生的覆盖先产生的（与 C 固件单事件寄存器一致）。

/// 扫描周期（ms）。
pub const SCAN_PERIOD_MS: u32 = 10;
/// 去抖：连续相同电平 tick 数。
pub const DEBOUNCE_TICKS: u16 = 3;
/// 长按判定：按住 tick 数（800 ms）。
pub const LONG_PRESS_TICKS: u16 = 80;
/// 长按后重复事件间隔 tick 数（200 ms）。
pub const LONG_HOLD_REPEAT_TICKS: u16 = 20;
/// 双击窗口 tick 数（350 ms）。
pub const DOUBLE_CLICK_TICKS: u16 = 35;

/// 按键事件。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyEvent {
    /// 无待处理事件。
    None,
    /// 单击（双击窗口超时后确认）。
    Click,
    /// 双击（窗口内两次短按）。
    DoubleClick,
    /// 长按达成（按住第 800 ms）。
    LongPress,
    /// 长按期间重复（每 200 ms）。
    LongHold,
}

/// 单按键状态机。
pub struct KeyFsm {
    /// 上一次原始电平（true = 按下）。
    prev_raw: bool,
    /// 当前连续相同电平计数（去抖）。
    debounce_cnt: u16,
    /// 已确认电平（true = 按下）。
    confirmed: bool,
    /// 已确认按下的 tick 计数。
    press_ticks: u16,
    /// 长按重复倒计时。
    hold_repeat_cnt: u16,
    /// 双击窗口剩余 tick。
    click_wait: u16,
    /// 待处理事件。
    event: KeyEvent,
}

impl KeyFsm {
    /// 创建初始状态（未按下、无事件）。
    pub const fn new() -> Self {
        KeyFsm {
            prev_raw: false,
            debounce_cnt: 0,
            confirmed: false,
            press_ticks: 0,
            hold_repeat_cnt: 0,
            click_wait: 0,
            event: KeyEvent::None,
        }
    }

    /// 喂入一次原始电平采样（每 10 ms 一次）。
    pub fn scan(&mut self, raw_pressed: bool) {
        // 去抖：统计连续相同电平，达标后确认。
        if raw_pressed == self.prev_raw {
            if self.debounce_cnt < DEBOUNCE_TICKS {
                self.debounce_cnt += 1;
            }
        } else {
            self.prev_raw = raw_pressed;
            self.debounce_cnt = 1;
        }
        if self.debounce_cnt >= DEBOUNCE_TICKS {
            self.confirmed = raw_pressed;
        }

        let mut window_started = false;

        if self.confirmed {
            // 按下：累计时长，产生长按/长按重复。
            self.press_ticks = self.press_ticks.saturating_add(1);
            if self.press_ticks == LONG_PRESS_TICKS {
                self.event = KeyEvent::LongPress;
                self.hold_repeat_cnt = LONG_HOLD_REPEAT_TICKS;
            } else if self.press_ticks > LONG_PRESS_TICKS {
                self.hold_repeat_cnt -= 1;
                if self.hold_repeat_cnt == 0 {
                    self.event = KeyEvent::LongHold;
                    self.hold_repeat_cnt = LONG_HOLD_REPEAT_TICKS;
                }
            }
        } else if self.press_ticks > 0 {
            // 释放（确实按过）。
            if self.press_ticks < LONG_PRESS_TICKS {
                if self.click_wait > 0 {
                    self.event = KeyEvent::DoubleClick;
                    self.click_wait = 0;
                } else {
                    self.click_wait = DOUBLE_CLICK_TICKS;
                    window_started = true;
                }
            }
            // 长按释放不再产生事件。
            self.press_ticks = 0;
        }

        // 双击窗口递减（按下期间到期同样补发 Click）。
        if !window_started && self.click_wait > 0 {
            self.click_wait -= 1;
            if self.click_wait == 0 {
                self.event = KeyEvent::Click;
            }
        }
    }

    /// 取出并清除待处理事件。
    pub fn take_event(&mut self) -> KeyEvent {
        let ev = self.event;
        self.event = KeyEvent::None;
        ev
    }

    /// 查看待处理事件（不清除）。
    pub fn peek_event(&self) -> KeyEvent {
        self.event
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 扫描 n 次相同电平。
    fn feed(fsm: &mut KeyFsm, pressed: bool, n: usize) {
        for _ in 0..n {
            fsm.scan(pressed);
        }
    }

    /// 消费事件序列中第一个非 None 事件。
    fn drain(fsm: &mut KeyFsm) -> KeyEvent {
        fsm.take_event()
    }

    #[test]
    fn bounce_below_debounce_emits_nothing() {
        let mut k = KeyFsm::new();
        // 1–2 tick 抖动，不足以确认按下。
        k.scan(true);
        k.scan(false);
        k.scan(true);
        k.scan(false);
        assert_eq!(drain(&mut k), KeyEvent::None);
    }

    #[test]
    fn click_fires_only_after_double_click_window() {
        let mut k = KeyFsm::new();
        feed(&mut k, true, 5); // 按下 5 tick（含 3 tick 去抖）
        feed(&mut k, false, 4); // 释放并确认
        // 窗口未超时：无事件。
        assert_eq!(drain(&mut k), KeyEvent::None);
        // 补足窗口（35 tick，释放确认已消耗若干 tick）。
        feed(&mut k, false, DOUBLE_CLICK_TICKS as usize);
        assert_eq!(drain(&mut k), KeyEvent::Click);
        assert_eq!(drain(&mut k), KeyEvent::None);
    }

    #[test]
    fn double_click_suppresses_click() {
        let mut k = KeyFsm::new();
        feed(&mut k, true, 5);
        feed(&mut k, false, 4); // 第一次短按释放，窗口开启
        feed(&mut k, true, 5); // 第二次按下
        feed(&mut k, false, 4); // 第二次释放在窗口内
        assert_eq!(drain(&mut k), KeyEvent::DoubleClick);
        // 窗口已清，不再补发 Click。
        feed(&mut k, false, DOUBLE_CLICK_TICKS as usize + 5);
        assert_eq!(drain(&mut k), KeyEvent::None);
    }

    #[test]
    fn long_press_fires_exactly_at_80_ticks() {
        let mut k = KeyFsm::new();
        feed(&mut k, true, 3); // 去抖确认
        assert_eq!(drain(&mut k), KeyEvent::None);
        for i in 1..LONG_PRESS_TICKS {
            k.scan(true);
            let ev = drain(&mut k);
            if i < LONG_PRESS_TICKS - 1 {
                assert_eq!(ev, KeyEvent::None, "tick {} 不应触发长按", i);
            } else {
                assert_eq!(ev, KeyEvent::LongPress);
            }
        }
    }

    #[test]
    fn long_hold_repeats_every_200ms_and_release_quiet() {
        let mut k = KeyFsm::new();
        feed(&mut k, true, 3); // 确认按下
        // 逐 tick 扫描直到 LongPress。
        let mut guard = 0;
        loop {
            k.scan(true);
            guard += 1;
            if drain(&mut k) == KeyEvent::LongPress {
                break;
            }
            assert!(guard < 200, "长按应于 800 ms 左右触发");
        }
        // 之后恰好每 20 tick 一个 LongHold。
        let mut hold_at = 0;
        for i in 1..=LONG_HOLD_REPEAT_TICKS {
            k.scan(true);
            match drain(&mut k) {
                KeyEvent::LongHold => hold_at = i,
                KeyEvent::None => {}
                other => panic!("unexpected {:?}", other),
            }
        }
        assert_eq!(hold_at, LONG_HOLD_REPEAT_TICKS);
        // 释放：不再产生任何事件（修正 C 版重复触发）。
        feed(&mut k, false, 10);
        assert_eq!(drain(&mut k), KeyEvent::None);
    }

    #[test]
    fn press_duration_saturates_without_panic() {
        let mut k = KeyFsm::new();
        feed(&mut k, true, 70_000); // 远超 u16::MAX
        // 长按重复持续产生，不 panic、不归零。
        let mut seen = 0;
        for _ in 0..100 {
            k.scan(true);
            if drain(&mut k) == KeyEvent::LongHold {
                seen += 1;
            }
        }
        assert!(seen >= 4, "饱和后长按重复仍应工作，got {}", seen);
    }

    #[test]
    fn click_pending_then_long_press_still_clicks() {
        let mut k = KeyFsm::new();
        feed(&mut k, true, 5);
        feed(&mut k, false, 4); // 短按 → 窗口开启
        // 紧接长按；每 tick 消费事件（固件主循环即如此），两类事件都应可见。
        let mut saw_click = false;
        let mut saw_long = false;
        for _ in 0..LONG_PRESS_TICKS + 20 {
            k.scan(true);
            match drain(&mut k) {
                KeyEvent::Click => saw_click = true,
                KeyEvent::LongPress => saw_long = true,
                KeyEvent::None | KeyEvent::LongHold => {}
                other => panic!("unexpected {:?}", other),
            }
        }
        assert!(saw_click, "第一击应在窗口到期后补发 Click");
        assert!(saw_long, "长按应触发 LongPress");
    }
}
