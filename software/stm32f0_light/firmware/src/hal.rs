//! 时钟与外设初始化（`docs/design.md` §3.1）。
//!
//! 时钟树：HSI 8 MHz → PLL ×12 = 48 MHz SYSCLK（Flash 1 wait state），与 C
//! `SystemClock_Config` 一致。定时器基频 10 kHz（48 MHz / 4800）。
//! 引脚模式在各 bsp 构造时配置。

use stm32f0xx_hal as hal;

use hal::adc::Adc;
use hal::gpio::{gpioa, gpiob, gpiof};
use hal::pac;
use hal::prelude::*;
use hal::rcc::Rcc;
use hal::timers::Timer;

/// 初始化后的外设集合。
pub struct HalPeripherals {
    /// RCC 句柄（PWM 定时器配置等需要）。
    pub rcc: Rcc,
    /// GPIOA 分组（PA0 PIR/唤醒，PA1–3 ADC，PA4–6 按键，PA7/PA9 PWM，PA10 电源模式）。
    pub gpioa: gpioa::Parts,
    /// GPIOB 分组（PB1 状态灯红）。
    pub gpiob: gpiob::Parts,
    /// GPIOF 分组（PF0 绿、PF1 蓝）。
    pub gpiof: gpiof::Parts,
    /// TIM1（冷光 PWM 基定时器，10 kHz）。
    pub tim1: Timer<pac::TIM1>,
    /// TIM14（暖光 PWM 基定时器，10 kHz）。
    pub tim14: Timer<pac::TIM14>,
    /// ADC（12 位，软件触发）。
    pub adc: Adc,
    /// FLASH（参数保存）。
    pub flash: pac::FLASH,
    /// PWR（Standby/WKUP 管理）。
    pub pwr: pac::PWR,
    /// SysTick（10 ms 系统节拍）。
    pub sys_tick: cortex_m::peripheral::SYST,
}

/// 初始化全部外设。
///
/// # Panics
///
/// PAC 或 Cortex-M 外设已被取走时 panic。
pub fn init() -> HalPeripherals {
    let mut dp = pac::Peripherals::take().expect("PAC peripherals already taken");
    let cp = cortex_m::Peripherals::take().expect("cortex-m peripherals already taken");

    // HSI 8 MHz × PLL12 = 48 MHz SYSCLK。
    let mut rcc = dp.RCC.configure().sysclk(48.mhz()).freeze(&mut dp.FLASH);

    let gpioa = dp.GPIOA.split(&mut rcc);
    let gpiob = dp.GPIOB.split(&mut rcc);
    let gpiof = dp.GPIOF.split(&mut rcc);

    let tim1 = Timer::tim1(dp.TIM1, 10_000_u32.hz(), &mut rcc);
    let tim14 = Timer::tim14(dp.TIM14, 10_000_u32.hz(), &mut rcc);

    let adc = Adc::new(dp.ADC, &mut rcc);

    HalPeripherals {
        rcc,
        gpioa,
        gpiob,
        gpiof,
        tim1,
        tim14,
        adc,
        flash: dp.FLASH,
        pwr: dp.PWR,
        sys_tick: cp.SYST,
    }
}
