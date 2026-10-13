//! 冷暖双通道 PWM 驱动（暖 TIM14_CH1/PA7，冷 TIM1_CH2/PA9，10 kHz）。
//!
//! 占空比计算走 [`light_logic::pwm`]，与 C 公式逐级截断一致。

use embedded_hal::PwmPin;
use light_logic::pwm::duty_cycles;
use stm32f0xx_hal as hal;

use hal::gpio::gpioa::{PA7, PA9};
use hal::gpio::{Floating, Input};
use hal::pac;
use hal::prelude::*;
use hal::pwm::{self, C1, C2, PwmChannels};
use hal::rcc::Rcc;
use hal::timers::Timer;

/// 双通道白光 PWM 控制器。
pub struct BspPwm {
    warm: PwmChannels<pac::TIM14, C1>,
    cold: PwmChannels<pac::TIM1, C2>,
}

impl BspPwm {
    /// 占用定时器与引脚，配置为 PWM 输出（PA7 暖 AF4 / PA9 冷 AF2）。
    pub fn new(
        tim1: Timer<pac::TIM1>,
        tim14: Timer<pac::TIM14>,
        pa7: PA7<Input<Floating>>,
        pa9: PA9<Input<Floating>>,
        rcc: &mut Rcc,
    ) -> Self {
        let tim1_raw = tim1.release();
        let tim14_raw = tim14.release();

        let (pa7_af4, pa9_af2) = cortex_m::interrupt::free(|cs| {
            (
                pa7.into_alternate_af4(cs), // TIM14_CH1（暖）
                pa9.into_alternate_af2(cs), // TIM1_CH2（冷）
            )
        });

        let cold = pwm::tim1(tim1_raw, pa9_af2, rcc, 10_000_u32.hz());
        let warm = pwm::tim14(tim14_raw, pa7_af4, rcc, 10_000_u32.hz());

        BspPwm { warm, cold }
    }

    /// 设置亮度与色温（0 = 全冷，100 = 全暖）。
    pub fn set(&mut self, bright: u8, cct: u8) {
        let max_duty = self.cold.get_max_duty();
        let (warm_duty, cold_duty) = duty_cycles(bright, cct, max_duty);
        self.warm.set_duty(warm_duty);
        self.cold.set_duty(cold_duty);
        self.warm.enable();
        self.cold.enable();
    }

    /// 关闭输出。
    pub fn off(&mut self) {
        self.warm.set_duty(0);
        self.cold.set_duty(0);
        self.warm.disable();
        self.cold.disable();
    }
}
