//! 应用状态机与保护逻辑（`docs/design.md` §2.6）。
//!
//! tick 执行顺序：保护状态更新 → 按键事件（渐变/待睡眠期间忽略，FR-10.2）→
//! 夜灯超时 → 低电闪烁 → 渐变推进与 PWM 目标 → 睡眠出口 → RGB 决策（D1 层叠）。

use crate::fade::FadeState;
use crate::key::KeyEvent;
use crate::pwm::{clamp_brightness, clamp_cct, BRIGHTNESS_MIN};
use crate::sensor::{battery_level, BatteryLevel, BAT_LOW_CLEAR, NTC_OVERHEAT_OHM, NTC_RECOVER_OHM};

/// 调节步进（FR-4.3）。
pub const STEP: i16 = 5;
/// 夜灯自动睡眠超时（ms，FR-5.2）。
pub const DARK_TIMEOUT_MS: u32 = 60_000;
/// 过热降亮起始（ms，FR-7.2）。
pub const OVERHEAT_START_MS: u32 = 60_000;
/// 过热每分钟降亮档数（FR-7.2）。
pub const DERATE_STEP: u8 = 10;
/// 低电闪烁总时长（ms，FR-11.2）。
pub const LOW_BATTERY_BLINK_MS: u32 = 20_000;
/// 低电闪烁半周期（ms，1 Hz）。
pub const BLINK_HALF_PERIOD_MS: u32 = 500;
/// 模式指示保持时长（ms，FR-8.2 优先级 3）。
pub const MODE_INDICATOR_MS: u32 = 2_000;

/// 系统状态。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    /// 待机（Standby）。
    Sleep,
    /// 灯灭，等待 FUN 长按开机。
    LightIdleNormal,
    /// 手动模式点亮。
    LightOn,
    /// PIR 夜灯模式点亮。
    LightOnDark,
}

/// 调节维度。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CtrlMode {
    /// 亮度。
    Brightness,
    /// 色温。
    ColorTemp,
}

impl CtrlMode {
    /// 切换维度（FUN 单击）。
    pub fn toggle(&mut self) {
        *self = match *self {
            CtrlMode::Brightness => CtrlMode::ColorTemp,
            CtrlMode::ColorTemp => CtrlMode::Brightness,
        };
    }
}

/// 状态灯颜色（active-low 在固件侧换算）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RgbColor {
    /// 全灭。
    Off,
    /// 红。
    Red,
    /// 绿。
    Green,
    /// 蓝。
    Blue,
    /// 黄（红+绿）。
    Yellow,
    /// 紫（红+蓝）。
    Purple,
    /// 白（红+绿+蓝）。
    White,
}

/// 睡眠原因。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SleepReason {
    /// FUN 长按关机。
    UserPowerOff,
    /// 夜灯超时。
    DarkTimeout,
    /// 低电保护。
    LowBattery,
}

/// 一次 tick 的输入。
pub struct TickInput {
    /// 当前时间（ms，允许回绕）。
    pub now_ms: u32,
    /// FUN 键事件。
    pub fun: KeyEvent,
    /// CTRL− 键事件。
    pub minus: KeyEvent,
    /// CTRL+ 键事件。
    pub plus: KeyEvent,
    /// PIR 是否触发（高电平）。
    pub pir_active: bool,
    /// 电池电压（V，1 s 节流后的采样值）。
    pub battery_v: f32,
    /// NTC 阻值（Ω，1 s 节流后的采样值）。
    pub ntc_resistance: u32,
}

/// 一次 tick 的输出。
pub struct Output {
    /// 当前状态。
    pub state: State,
    /// PWM 目标（亮度, 色温）；有变化时给出。
    pub light: Option<(u8, u8)>,
    /// 状态灯颜色（D1 层叠优先级决策结果）。
    pub rgb: RgbColor,
    /// 睡眠请求（渐灭完成后给出，固件执行睡眠序列）。
    pub sleep: Option<SleepReason>,
}

/// 应用状态。
pub struct App {
    state: State,
    mode: CtrlMode,
    /// 手动组 [亮度, 色温]。
    level: [u8; 2],
    /// 夜灯组 [亮度, 色温]。
    dark: [u8; 2],
    fade: FadeState,
    /// 夜灯计时起点（PIR 触发即复位，Q6）。
    dark_start_ms: u32,
    /// 过热起点（Some = 过热中）。
    overheat_since: Option<u32>,
    /// 低电闪烁起点（Some = 低电中）。
    low_battery_since: Option<u32>,
    /// 模式指示起点（FUN 单击后 2 s 内显示，FR-8.2）。
    mode_indicator_at: Option<u32>,
    /// 已请求的睡眠（渐灭完成后出口）。
    pending_sleep: Option<SleepReason>,
    /// 上次输出的 PWM（变化检测）。
    last_light: (u8, u8),
}

impl App {
    /// 创建应用状态（参数来自 Flash 加载或 [`crate::settings::Settings::DEFAULT`]）。
    pub fn new(level: (u8, u8), dark: (u8, u8)) -> Self {
        App {
            state: State::LightIdleNormal,
            mode: CtrlMode::Brightness,
            level: [level.0, level.1],
            dark: [dark.0, dark.1],
            fade: FadeState::new(),
            dark_start_ms: 0,
            overheat_since: None,
            low_battery_since: None,
            mode_indicator_at: None,
            pending_sleep: None,
            last_light: (0, 0),
        }
    }

    /// 上电/唤醒初始化（FR-4.1）：暗环境直接进入夜灯模式（渐亮），否则待机待开。
    pub fn setup(&mut self, is_dark: bool, battery_v: f32, now_ms: u32) -> Output {
        if is_dark {
            self.state = State::LightOnDark;
            self.dark_start_ms = now_ms;
            self.fade.start(0, self.dark[0], now_ms);
        } else {
            self.state = State::LightIdleNormal;
        }
        self.handle_battery(battery_v, now_ms);
        self.finish_tick(battery_v, now_ms)
    }

    /// 推进一个 10 ms tick。
    pub fn tick(&mut self, input: TickInput) -> Output {
        let now = input.now_ms;

        // 1) 保护状态更新（始终推进）。
        self.handle_temperature(input.ntc_resistance, now);
        self.handle_battery(input.battery_v, now);

        // 2) 按键事件（渐变/待睡眠期间忽略，FR-10.2）。
        if !self.fade.is_active() && self.pending_sleep.is_none() {
            self.handle_keys(input.fun, input.minus, input.plus, now);
        }

        // 3) 夜灯超时（PIR 触发复位计时，Q6）。
        if self.state == State::LightOnDark && self.pending_sleep.is_none() {
            if input.pir_active {
                self.dark_start_ms = now;
            } else if now.wrapping_sub(self.dark_start_ms) >= DARK_TIMEOUT_MS {
                self.begin_sleep(SleepReason::DarkTimeout, now);
            }
        }

        // 4) 低电闪烁计时。
        self.handle_low_battery(now);

        self.finish_tick(input.battery_v, now)
    }

    /// 当前参数组（用于睡眠前保存）。
    pub fn settings(&self) -> crate::settings::Settings {
        crate::settings::Settings {
            brightness: self.level[0],
            cct: self.level[1],
            dark_brightness: self.dark[0],
            dark_cct: self.dark[1],
        }
    }

    /// 当前状态。
    pub fn state(&self) -> State {
        self.state
    }

    // -----------------------------------------------------------------
    // 内部：事件处理
    // -----------------------------------------------------------------

    fn handle_keys(&mut self, fun: KeyEvent, minus: KeyEvent, plus: KeyEvent, now: u32) {
        match self.state {
            State::LightIdleNormal => {
                // Q1 修复：长按开机并进入 LightOn。
                if fun == KeyEvent::LongPress {
                    self.state = State::LightOn;
                    self.fade.start(0, self.level[0], now);
                }
            }
            State::LightOn => {
                if fun == KeyEvent::LongPress {
                    self.begin_sleep(SleepReason::UserPowerOff, now);
                    return;
                }
                if fun == KeyEvent::Click {
                    self.mode.toggle();
                    self.mode_indicator_at = Some(now);
                }
                self.handle_adjust(minus, plus);
            }
            State::LightOnDark => {
                if fun == KeyEvent::LongPress {
                    // 夜灯 → 手动：亮度过渡（FR-10.1）。
                    self.state = State::LightOn;
                    self.fade.start(self.last_light.0, self.level[0], now);
                    return;
                }
                if fun == KeyEvent::Click {
                    self.mode.toggle();
                    self.mode_indicator_at = Some(now);
                }
                self.handle_adjust(minus, plus);
            }
            State::Sleep => {}
        }
    }

    fn handle_adjust(&mut self, minus: KeyEvent, plus: KeyEvent) {
        let dark = self.state == State::LightOnDark;
        let adjust = |app: &mut App, delta: i16| {
            let g = if dark { &mut app.dark } else { &mut app.level };
            match app.mode {
                CtrlMode::Brightness => g[0] = clamp_brightness(g[0] as i16 + delta),
                CtrlMode::ColorTemp => g[1] = clamp_cct(g[1] as i16 + delta),
            }
        };
        let set_bright = |app: &mut App, v: u8| {
            // Q3 修复：双击直达同步存储值。
            let g = if dark { &mut app.dark } else { &mut app.level };
            g[0] = clamp_brightness(v as i16);
        };

        if matches!(minus, KeyEvent::Click | KeyEvent::LongHold) {
            adjust(self, -STEP);
        }
        if minus == KeyEvent::DoubleClick {
            set_bright(self, BRIGHTNESS_MIN);
        }
        if matches!(plus, KeyEvent::Click | KeyEvent::LongHold) {
            adjust(self, STEP);
        }
        if plus == KeyEvent::DoubleClick {
            set_bright(self, 100);
        }
    }

    // -----------------------------------------------------------------
    // 内部：保护逻辑
    // -----------------------------------------------------------------

    fn handle_temperature(&mut self, resistance: u32, now: u32) {
        if resistance <= NTC_OVERHEAT_OHM {
            if self.overheat_since.is_none() {
                self.overheat_since = Some(now);
            }
        } else if resistance >= NTC_RECOVER_OHM {
            if self.overheat_since.take().is_some() && self.pending_sleep.is_none() {
                // Q5 修复：恢复到当前模式组亮度，渐变（FR-10.1）。
                let target = self.target_brightness();
                self.fade.start(self.last_light.0, target, now);
            }
        }
    }

    fn handle_battery(&mut self, voltage: f32, now: u32) {
        if battery_level(voltage) == BatteryLevel::Critical {
            if self.low_battery_since.is_none() {
                self.low_battery_since = Some(now);
            }
        } else if voltage >= BAT_LOW_CLEAR {
            // D2 迟滞解除；已请求的低电睡眠一并撤销。
            self.low_battery_since = None;
            if self.pending_sleep == Some(SleepReason::LowBattery) {
                self.pending_sleep = None;
                let target = self.target_brightness();
                self.fade.start(self.last_light.0, target, now);
            }
        }
    }

    fn handle_low_battery(&mut self, now: u32) {
        if let Some(t0) = self.low_battery_since {
            if now.wrapping_sub(t0) >= LOW_BATTERY_BLINK_MS {
                self.begin_sleep(SleepReason::LowBattery, now);
            }
        }
    }

    fn begin_sleep(&mut self, reason: SleepReason, now: u32) {
        if self.pending_sleep.is_none() {
            self.pending_sleep = Some(reason);
            self.fade.start(self.last_light.0, 0, now);
        }
    }

    // -----------------------------------------------------------------
    // 内部：输出
    // -----------------------------------------------------------------

    /// 目标亮度（未考虑渐变/过热降亮）。
    fn target_brightness(&self) -> u8 {
        if self.state == State::LightIdleNormal {
            0
        } else if self.state == State::LightOnDark {
            self.dark[0]
        } else {
            self.level[0]
        }
    }

    /// 当前输出亮度与色温。
    fn desired_light(&mut self, now: u32) -> (u8, u8) {
        let cct = if self.state == State::LightOnDark {
            self.dark[1]
        } else {
            self.level[1]
        };
        if self.fade.is_active() {
            return (self.fade.update(now), cct);
        }
        let mut b = self.target_brightness();
        if self.state != State::LightIdleNormal {
            if let Some(t0) = self.overheat_since {
                let dur = now.wrapping_sub(t0);
                if dur >= OVERHEAT_START_MS {
                    // Q5 修复：按持续时长每 60 s 降 DERATE_STEP，下限 5。
                    let drop = (DERATE_STEP as u32)
                        .saturating_mul(dur / OVERHEAT_START_MS)
                        .min(u8::MAX as u32) as u8;
                    b = b.saturating_sub(drop).max(BRIGHTNESS_MIN);
                }
            }
        }
        (b, cct)
    }

    /// 状态灯颜色（D1：低电闪烁 > 过热 > 模式指示 > 电池色）。
    fn decide_rgb(&self, battery_v: f32, now: u32) -> RgbColor {
        if let Some(t0) = self.low_battery_since {
            let e = now.wrapping_sub(t0);
            if e >= LOW_BATTERY_BLINK_MS {
                return RgbColor::Off;
            }
            return if (e / BLINK_HALF_PERIOD_MS) % 2 == 0 {
                RgbColor::Red
            } else {
                RgbColor::Off
            };
        }
        if self.overheat_since.is_some() {
            return RgbColor::Purple;
        }
        if let Some(t0) = self.mode_indicator_at {
            if now.wrapping_sub(t0) < MODE_INDICATOR_MS {
                return match (self.state == State::LightOnDark, self.mode) {
                    (false, CtrlMode::Brightness) => RgbColor::White,
                    (false, CtrlMode::ColorTemp) => RgbColor::Yellow,
                    (true, CtrlMode::Brightness) => RgbColor::Purple,
                    (true, CtrlMode::ColorTemp) => RgbColor::Blue,
                };
            }
        }
        match battery_level(battery_v) {
            BatteryLevel::Green => RgbColor::Green,
            BatteryLevel::Yellow => RgbColor::Yellow,
            // Critical 由低电闪烁分支覆盖，此处兜底红。
            BatteryLevel::Red | BatteryLevel::Critical => RgbColor::Red,
        }
    }

    /// 组装输出（setup 与 tick 共用）。
    fn finish_tick(&mut self, battery_v: f32, now: u32) -> Output {
        let light = self.desired_light(now);
        let changed = light != self.last_light;
        self.last_light = light;
        let sleep = if self.pending_sleep.is_some() && !self.fade.is_active() {
            self.pending_sleep
        } else {
            None
        };
        Output {
            state: self.state,
            light: if changed { Some(light) } else { None },
            rgb: self.decide_rgb(battery_v, now),
            sleep,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BAT_OK: f32 = 4.0;
    const NTC_OK: u32 = 30_000;

    fn idle_input(now: u32) -> TickInput {
        TickInput {
            now_ms: now,
            fun: KeyEvent::None,
            minus: KeyEvent::None,
            plus: KeyEvent::None,
            pir_active: false,
            battery_v: BAT_OK,
            ntc_resistance: NTC_OK,
        }
    }

    fn tick(app: &mut App, now: u32) -> Output {
        app.tick(idle_input(now))
    }

    fn tick_ev(app: &mut App, now: u32, fun: KeyEvent, minus: KeyEvent, plus: KeyEvent) -> Output {
        let mut i = idle_input(now);
        i.fun = fun;
        i.minus = minus;
        i.plus = plus;
        app.tick(i)
    }

    /// 手动模式开机后的 App。
    fn app_on(now: u32) -> App {
        let mut app = App::new((5, 50), (10, 45));
        app.setup(false, BAT_OK, now);
        tick_ev(&mut app, now + 10, KeyEvent::LongPress, KeyEvent::None, KeyEvent::None);
        // 渐亮完成。
        tick(&mut app, now + 2_000);
        app
    }

    #[test]
    fn boot_dark_enters_night_light_with_fade() {
        let mut app = App::new((5, 50), (10, 45));
        let out = app.setup(true, BAT_OK, 0);
        assert_eq!(out.state, State::LightOnDark);
        assert_eq!(out.light, Some((0, 45)));
        // 渐亮到夜灯组亮度。
        let out = tick(&mut app, 1_000);
        assert_eq!(out.light, Some((10, 45)));
    }

    #[test]
    fn boot_light_waits_for_long_press_then_fades_on() {
        let mut app = App::new((5, 50), (10, 45));
        let out = app.setup(false, BAT_OK, 0);
        assert_eq!(out.state, State::LightIdleNormal);
        assert_eq!(out.light, Some((0, 50)));
        // Q1 修复：长按 → LightOn + 渐亮。
        let out = tick_ev(&mut app, 10, KeyEvent::LongPress, KeyEvent::None, KeyEvent::None);
        assert_eq!(out.state, State::LightOn);
        let out = tick(&mut app, 1_010);
        assert_eq!(out.light, Some((5, 50)));
    }

    #[test]
    fn keys_ignored_during_fade() {
        let mut app = App::new((5, 50), (10, 45));
        app.setup(false, BAT_OK, 0);
        tick_ev(&mut app, 10, KeyEvent::LongPress, KeyEvent::None, KeyEvent::None);
        // 渐变中按 +：被忽略。
        tick_ev(&mut app, 100, KeyEvent::None, KeyEvent::None, KeyEvent::Click);
        let out = tick(&mut app, 1_010);
        assert_eq!(out.light, Some((5, 50)), "渐变期间步进不应生效");
    }

    #[test]
    fn adjust_clamps_and_tracks_mode() {
        let mut app = app_on(0);
        // 切到色温模式，连按 − 10 次：50 → 0（Q3：下限 0 而非 5）。
        tick_ev(&mut app, 2_100, KeyEvent::Click, KeyEvent::None, KeyEvent::None);
        let mut out = None;
        for _ in 0..10 {
            out = Some(tick_ev(
                &mut app,
                2_110,
                KeyEvent::None,
                KeyEvent::Click,
                KeyEvent::None,
            ));
        }
        assert_eq!(out.unwrap().light, Some((5, 0)));
        assert_eq!(app.settings().cct, 0);
        // 继续 −：钳位不动。
        let out = tick_ev(
            &mut app,
            2_120,
            KeyEvent::None,
            KeyEvent::Click,
            KeyEvent::None,
        );
        assert_eq!(out.light, None);
        // 切回亮度模式，− 两次：已到下限 5 不再降。
        tick_ev(&mut app, 2_300, KeyEvent::Click, KeyEvent::None, KeyEvent::None);
        tick_ev(&mut app, 2_310, KeyEvent::None, KeyEvent::Click, KeyEvent::None);
        let out = tick_ev(
            &mut app,
            2_320,
            KeyEvent::None,
            KeyEvent::Click,
            KeyEvent::None,
        );
        assert_eq!(out.light, None, "亮度已在下限，无变化");
        assert_eq!(app.settings().brightness, BRIGHTNESS_MIN);
    }

    #[test]
    fn double_click_syncs_stored_value() {
        let mut app = app_on(0);
        // + 双击 → 亮度 100（Q3：同步存储）。
        let out = tick_ev(
            &mut app,
            2_100,
            KeyEvent::None,
            KeyEvent::None,
            KeyEvent::DoubleClick,
        );
        assert_eq!(out.light, Some((100, 50)));
        // - 单击：从 100 降，而不是从 5 跳变。
        let out = tick_ev(&mut app, 2_300, KeyEvent::None, KeyEvent::Click, KeyEvent::None);
        assert_eq!(out.light, Some((95, 50)));
    }

    #[test]
    fn user_power_off_fades_then_sleeps() {
        let mut app = app_on(0);
        let out = tick_ev(&mut app, 3_000, KeyEvent::LongPress, KeyEvent::None, KeyEvent::None);
        assert_eq!(out.sleep, None, "渐灭完成前不应睡眠");
        // 渐灭中（5 → 0 线性，500 ms 处 5 + (−5×500/1000) = 3，向零截断）。
        let out = tick(&mut app, 3_500);
        assert_eq!(out.light, Some((3, 50)));
        assert_eq!(out.sleep, None);
        // 渐灭完成 → 睡眠请求。
        let out = tick(&mut app, 4_000);
        assert_eq!(out.sleep, Some(SleepReason::UserPowerOff));
    }

    #[test]
    fn dark_timeout_sleeps_and_pir_resets_timer() {
        let mut app = App::new((5, 50), (10, 45));
        app.setup(true, BAT_OK, 0);
        tick(&mut app, 1_000); // 渐亮完成
        // 59 s：未超时。
        let out = tick(&mut app, 59_000);
        assert_eq!(out.sleep, None);
        // PIR 触发复位计时（Q6）。
        let mut i = idle_input(60_500);
        i.pir_active = true;
        app.tick(i);
        let out = tick(&mut app, 119_000);
        assert_eq!(out.sleep, None, "PIR 复位后 60 s 重新计");
        // PIR 持续低 60 s → 超时，渐灭后睡眠。
        let out = tick(&mut app, 121_000);
        assert_eq!(out.sleep, None, "渐灭完成前不睡眠");
        let out = tick(&mut app, 122_000);
        assert_eq!(out.sleep, Some(SleepReason::DarkTimeout));
    }

    #[test]
    fn overheat_derates_and_recovers_with_correct_group() {
        let mut app = App::new((100, 50), (10, 45));
        app.setup(false, BAT_OK, 0);
        tick_ev(&mut app, 10, KeyEvent::LongPress, KeyEvent::None, KeyEvent::None);
        tick(&mut app, 1_010); // 点亮 100
        // 过热开始（<15 kΩ）。
        let mut i = idle_input(2_000);
        i.ntc_resistance = 14_000;
        let out = app.tick(i);
        assert_eq!(out.rgb, RgbColor::Purple);
        // 30 s：未降亮。
        let mut i = idle_input(32_000);
        i.ntc_resistance = 14_000;
        let out = app.tick(i);
        assert_eq!(out.light, None);
        // 60 s：降 10（100 → 90）。
        let mut i = idle_input(62_000);
        i.ntc_resistance = 14_000;
        let out = app.tick(i);
        assert_eq!(out.light, Some((90, 50)));
        // 120 s：降 20。
        let mut i = idle_input(122_000);
        i.ntc_resistance = 14_000;
        let out = app.tick(i);
        assert_eq!(out.light, Some((80, 50)));
        // 恢复（>25 kΩ）：渐变回 100（Q5：当前模式组）。
        let mut i = idle_input(123_000);
        i.ntc_resistance = 26_000;
        let out = app.tick(i);
        assert_eq!(out.rgb, RgbColor::Green);
        let out = tick(&mut app, 124_000);
        assert_eq!(out.light, Some((100, 50)));
    }

    #[test]
    fn overheat_derates_dark_group_in_night_mode() {
        let mut app = App::new((100, 50), (60, 45));
        app.setup(true, BAT_OK, 0);
        tick(&mut app, 1_010); // 夜灯 60
        let mut i = idle_input(2_000);
        i.ntc_resistance = 14_000;
        i.pir_active = true; // PIR 持续触发，避免夜灯超时干扰
        app.tick(i);
        let mut i = idle_input(62_000);
        i.ntc_resistance = 14_000;
        i.pir_active = true;
        let out = app.tick(i);
        // Q5：夜灯模式按夜灯组降亮（60 → 50），而非手动组。
        assert_eq!(out.light, Some((50, 45)));
    }

    #[test]
    fn low_battery_blinks_then_sleeps_and_hysteresis_clears() {
        let mut app = app_on(0);
        // <2.7 V：红灯闪烁（D2）。
        let mut i = idle_input(5_000);
        i.battery_v = 2.6;
        let out = app.tick(i);
        assert_eq!(out.rgb, RgbColor::Red);
        let mut i = idle_input(5_500);
        i.battery_v = 2.6;
        let out = app.tick(i);
        assert_eq!(out.rgb, RgbColor::Off);
        // 20 s 闪烁满 → 渐灭后睡眠。
        let mut i = idle_input(25_000);
        i.battery_v = 2.6;
        let out = app.tick(i);
        assert_eq!(out.sleep, None, "渐灭完成前不睡眠");
        let mut i = idle_input(26_000);
        i.battery_v = 2.6;
        let out = app.tick(i);
        assert_eq!(out.sleep, Some(SleepReason::LowBattery));

        // 迟滞：3.0 V 不解除、3.1 V 解除。
        let mut app = app_on(0);
        let mut i = idle_input(5_000);
        i.battery_v = 2.6;
        app.tick(i);
        let mut i = idle_input(6_000);
        i.battery_v = 3.0;
        let out = app.tick(i);
        assert!(matches!(out.rgb, RgbColor::Red | RgbColor::Off), "3.0 V 不解除");
        let mut i = idle_input(6_500);
        i.battery_v = 3.1;
        let out = app.tick(i);
        assert_eq!(out.rgb, RgbColor::Red, "3.1 V 解除闪烁，电量色红");
        let mut i = idle_input(6_600);
        i.battery_v = 3.8;
        let out = app.tick(i);
        assert_eq!(out.rgb, RgbColor::Green);
    }

    #[test]
    fn low_battery_recovered_cancels_pending_sleep() {
        let mut app = app_on(0);
        // 进入低电并走完闪烁（20 s）→ 渐灭中。
        let mut i = idle_input(5_000);
        i.battery_v = 2.6;
        app.tick(i);
        let mut i = idle_input(25_000);
        i.battery_v = 2.6;
        app.tick(i);
        let mut i = idle_input(25_250);
        i.battery_v = 2.6;
        app.tick(i); // 渐灭中（亮度降到 4）
        // 渐灭中电压恢复 → 撤销睡眠并渐变回手动组。
        let mut i = idle_input(25_500);
        i.battery_v = 3.2;
        let out = app.tick(i);
        assert_eq!(out.sleep, None);
        let out = tick(&mut app, 26_500);
        assert_eq!(out.sleep, None);
        assert_eq!(out.light, Some((5, 50)), "恢复后回到手动组亮度");
    }

    #[test]
    fn rgb_priority_layers() {
        let mut app = app_on(0);
        // 电池色基础层。
        let out = tick(&mut app, 2_100);
        assert_eq!(out.rgb, RgbColor::Green);
        // 模式指示覆盖电池色 2 s：首击切色温 → 黄；再击切亮度 → 白。
        let out = tick_ev(&mut app, 2_200, KeyEvent::Click, KeyEvent::None, KeyEvent::None);
        assert_eq!(out.rgb, RgbColor::Yellow, "色温模式指示");
        let out = tick_ev(&mut app, 2_300, KeyEvent::Click, KeyEvent::None, KeyEvent::None);
        assert_eq!(out.rgb, RgbColor::White);
        let out = tick(&mut app, 4_400);
        assert_eq!(out.rgb, RgbColor::Green, "2 s 后回电池色");
        // 过热覆盖模式指示。
        tick_ev(&mut app, 4_500, KeyEvent::Click, KeyEvent::None, KeyEvent::None);
        let mut i = idle_input(4_600);
        i.ntc_resistance = 14_000;
        let out = app.tick(i);
        assert_eq!(out.rgb, RgbColor::Purple);
        // 低电闪烁覆盖过热。
        let mut i = idle_input(4_700);
        i.ntc_resistance = 14_000;
        i.battery_v = 2.6;
        let out = app.tick(i);
        assert_eq!(out.rgb, RgbColor::Red);
    }

    #[test]
    fn night_mode_sub_mode_indicator() {
        let mut app = App::new((5, 50), (10, 45));
        app.setup(true, BAT_OK, 0);
        tick(&mut app, 1_010);
        let out = tick_ev(&mut app, 1_100, KeyEvent::Click, KeyEvent::None, KeyEvent::None);
        assert_eq!(out.rgb, RgbColor::Blue, "夜灯色温子模式 = 蓝");
        let out = tick_ev(&mut app, 1_200, KeyEvent::Click, KeyEvent::None, KeyEvent::None);
        assert_eq!(out.rgb, RgbColor::Purple, "夜灯亮度子模式 = 紫");
    }

    #[test]
    fn night_to_manual_switches_group() {
        let mut app = App::new((5, 50), (10, 45));
        app.setup(true, BAT_OK, 0);
        tick(&mut app, 1_010);
        // FUN 长按 → 手动组（渐变 10 → 5）。
        let out = tick_ev(&mut app, 2_000, KeyEvent::LongPress, KeyEvent::None, KeyEvent::None);
        assert_eq!(out.state, State::LightOn);
        let out = tick(&mut app, 3_000);
        assert_eq!(out.light, Some((5, 50)));
    }

    #[test]
    fn settings_reflect_adjustments() {
        let mut app = app_on(0);
        tick_ev(&mut app, 2_100, KeyEvent::None, KeyEvent::None, KeyEvent::DoubleClick);
        let s = app.settings();
        assert_eq!((s.brightness, s.cct), (100, 50));
        assert_eq!((s.dark_brightness, s.dark_cct), (10, 45));
    }
}
