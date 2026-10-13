//! 主循环集成（`docs/design.md` §3.3）。
//!
//! 结构：SysTick 10 ms 节拍（仅递增计数）→ 主循环每 tick 扫描按键、每 1 s 节流
//! 采样电量/NTC（FR-6.3）、调用 [`App::tick`]、应用输出（PWM/状态灯/睡眠序列）、
//! 输出 RTT 日志（FR-12）。
#![no_std]
#![no_main]

use panic_halt as _;

use core::sync::atomic::{AtomicU32, Ordering};
use cortex_m_rt::entry;

use embedded_hal::digital::v2::{InputPin, OutputPin};
use light_logic::app::{App, RgbColor, SleepReason, State, TickInput};
use light_logic::key::KeyEvent;
use stm32f0_light::bsp;
use stm32f0_light::hal::{self, HalPeripherals};
use stm32f0xx_hal as hal_crate;

use hal_crate::gpio::gpioa::PA10;
use hal_crate::gpio::{Output, PushPull};
use hal_crate::pac;

/// SysTick 节拍计数（10 ms/tick；ISR 写、主循环读）。
static TICKS: AtomicU32 = AtomicU32::new(0);

/// 电量/NTC 采样节流：每 100 tick = 1 s。
const MONITOR_TICKS: u32 = 100;

#[cortex_m_rt::exception]
fn SysTick() {
    // M0 无 CAS，不能 fetch_add；本 ISR 是唯一写者，读改写安全。
    let now = TICKS.load(Ordering::Relaxed);
    TICKS.store(now.wrapping_add(1), Ordering::Relaxed);
}

/// RTT 日志宏（feature `rtt` 关闭时编译为空）。
#[cfg(feature = "rtt")]
macro_rules! log {
    ($($arg:tt)*) => {
        rtt_target::rprintln!($($arg)*)
    };
}

#[cfg(not(feature = "rtt"))]
macro_rules! log {
    ($($arg:tt)*) => {};
}

/// 睡眠序列（FR-1.2）：保存设置 → PWM/LED off → PA10 低（PFM）→ Standby。不返回。
fn sleep_seq(
    pwm: &mut bsp::pwm::BspPwm,
    led: &mut bsp::led::BspLed,
    flash: &mut bsp::flash::BspFlash,
    mode_pin: &mut PA10<Output<PushPull>>,
    pwr: &pac::PWR,
    app: &App,
    reason: SleepReason,
) -> ! {
    // no-rtt 构建中 reason 仅用于日志，显式消费避免未使用告警。
    #[cfg(not(feature = "rtt"))]
    let _ = reason;
    log!("[sleep] reason={:?} settings={:?}", reason, app.settings());
    flash.save(&app.settings());
    pwm.off();
    led.set(RgbColor::Off);
    let _ = mode_pin.set_low(); // TPS63802 → PFM
    bsp::power::enter_standby(pwr)
}

#[entry]
fn main() -> ! {
    // RTT 初始化（调试日志，FR-12）。
    #[cfg(feature = "rtt")]
    rtt_target::rtt_init_print!();

    let HalPeripherals {
        mut rcc,
        gpioa,
        gpiob,
        gpiof,
        tim1,
        tim14,
        adc,
        flash,
        pwr,
        mut sys_tick,
    } = hal::init();

    // 复位源处理（FR-1.1）。
    bsp::power::pre_setup(&pwr);

    // SysTick：10 ms（48 MHz × 0.01 s = 480 000 tick）。
    sys_tick.set_reload(480_000 - 1);
    sys_tick.clear_current();
    sys_tick.enable_interrupt();
    sys_tick.enable_counter();

    // 引脚模式配置。
    let (pir_pin, mut mode_pin, fun_pin, minus_pin, plus_pin) =
        cortex_m::interrupt::free(|cs| {
            (
                gpioa.pa0.into_pull_down_input(cs),   // PA0 PIR（高有效，下拉）
                gpioa.pa10.into_push_pull_output(cs), // PA10 TPS63802 模式
                gpioa.pa4.into_pull_up_input(cs),     // FUN
                gpioa.pa5.into_pull_up_input(cs),     // CTRL−
                gpioa.pa6.into_pull_up_input(cs),     // CTRL+
            )
        });
    // C 现状保持：PA10 全程 PFM（初始化拉低后不再拉高）。
    let _ = mode_pin.set_low();

    let mut keys = bsp::key::BspKeys::new(fun_pin, minus_pin, plus_pin);
    let mut pwm = bsp::pwm::BspPwm::new(tim1, tim14, gpioa.pa7, gpioa.pa9, &mut rcc);
    let mut led = bsp::led::BspLed::new(gpiob.pb1, gpiof.pf0, gpiof.pf1);
    let mut adc = bsp::adc::BspAdc::new(adc, gpioa.pa1, gpioa.pa2, gpioa.pa3);
    let mut flash = bsp::flash::BspFlash::new(flash);

    // 启动流程（FR-4.1）：加载设置 → 光敏判定 → 电量检查。
    let settings = flash.load();
    let mut app = App::new(
        (settings.brightness, settings.cct),
        (settings.dark_brightness, settings.dark_cct),
    );
    let is_dark = adc.is_dark();
    let mut battery_v = adc.battery_voltage();
    let mut ntc_resistance = adc.ntc_resistance();
    log!(
        "[boot] dark={} battery_mV={} ntc_ohm={} settings={:?}",
        is_dark,
        (battery_v * 1000.0) as u32,
        ntc_resistance,
        settings
    );

    let mut last_state: State = {
        let out = app.setup(is_dark, battery_v, ticks_ms());
        if let Some((b, c)) = out.light {
            pwm.set(b, c);
        }
        led.set(out.rgb);
        if let Some(reason) = out.sleep {
            sleep_seq(&mut pwm, &mut led, &mut flash, &mut mode_pin, &pwr, &app, reason);
        }
        out.state
    };

    // 主循环：每 10 ms tick 一轮。
    let mut last_tick = u32::MAX;
    loop {
        let ticks = TICKS.load(Ordering::Relaxed);
        if ticks == last_tick {
            cortex_m::asm::wfi();
            continue;
        }
        last_tick = ticks;

        // 按键扫描与事件读取。
        keys.scan();
        let (fun, minus, plus) = keys.take_events();
        log_keys(fun, minus, plus);

        // 1 s 节流采样（FR-6.3）。
        if ticks % MONITOR_TICKS == 0 {
            battery_v = adc.battery_voltage();
            ntc_resistance = adc.ntc_resistance();
            log!(
                "[monitor] battery_mV={} ntc_ohm={}",
                (battery_v * 1000.0) as u32,
                ntc_resistance
            );
        }

        let out = app.tick(TickInput {
            now_ms: ticks_ms(),
            fun,
            minus,
            plus,
            pir_active: pir_pin.is_high().unwrap_or(false),
            battery_v,
            ntc_resistance,
        });

        if let Some((b, c)) = out.light {
            pwm.set(b, c);
            log!("[pwm] bright={} cct={}", b, c);
        }
        led.set(out.rgb);

        if out.state != last_state {
            log!("[state] {:?} -> {:?}", last_state, out.state);
            last_state = out.state;
        }

        if let Some(reason) = out.sleep {
            sleep_seq(&mut pwm, &mut led, &mut flash, &mut mode_pin, &pwr, &app, reason);
        }
    }
}

/// 当前时间（ms）。
fn ticks_ms() -> u32 {
    TICKS.load(Ordering::Relaxed).wrapping_mul(10)
}

/// 非空按键事件日志。
fn log_keys(fun: KeyEvent, minus: KeyEvent, plus: KeyEvent) {
    if fun != KeyEvent::None {
        log!("[key] FUN {:?}", fun);
    }
    if minus != KeyEvent::None {
        log!("[key] - {:?}", minus);
    }
    if plus != KeyEvent::None {
        log!("[key] + {:?}", plus);
    }
}
