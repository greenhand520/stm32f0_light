//! ADC 驱动：电池电压（CH1/PA1）、光敏（CH2/PA2）、NTC（CH3/PA3）。
//!
//! 每次读数 8 样本平均；换算与阈值走 [`light_logic::sensor`]。

use embedded_hal::adc::Channel;
use light_logic::sensor;
use stm32f0xx_hal as hal;

use hal::adc::Adc;
use hal::gpio::gpioa::{PA1, PA2, PA3};
use hal::gpio::Analog;
use hal::prelude::*;

/// 平均采样数（与 C 一致）。
const SAMPLES: u8 = 8;

/// ADC 采样器。
pub struct BspAdc {
    adc: Adc,
    bat_pin: PA1<Analog>,
    light_pin: PA2<Analog>,
    ntc_pin: PA3<Analog>,
}

impl BspAdc {
    /// 由 ADC 与三个模拟输入引脚构造。
    pub fn new(
        adc: Adc,
        pa1: PA1<hal::gpio::Input<hal::gpio::Floating>>,
        pa2: PA2<hal::gpio::Input<hal::gpio::Floating>>,
        pa3: PA3<hal::gpio::Input<hal::gpio::Floating>>,
    ) -> Self {
        let (bat_pin, light_pin, ntc_pin) = cortex_m::interrupt::free(|cs| {
            (pa1.into_analog(cs), pa2.into_analog(cs), pa3.into_analog(cs))
        });
        BspAdc {
            adc,
            bat_pin,
            light_pin,
            ntc_pin,
        }
    }

    /// 电池电压（V）。
    pub fn battery_voltage(&mut self) -> f32 {
        let raw = read_avg(&mut self.adc, &mut self.bat_pin);
        sensor::battery_voltage(raw)
    }

    /// 是否暗环境。
    pub fn is_dark(&mut self) -> bool {
        let raw = read_avg(&mut self.adc, &mut self.light_pin);
        sensor::is_dark(raw)
    }

    /// NTC 阻值（Ω）。
    pub fn ntc_resistance(&mut self) -> u32 {
        let raw = read_avg(&mut self.adc, &mut self.ntc_pin);
        sensor::ntc_resistance(raw)
    }
}

/// 8 样本平均读数（独立函数避免自引用借用冲突）。
fn read_avg<Pin>(adc: &mut Adc, pin: &mut Pin) -> u16
where
    Pin: Channel<Adc, ID = u8>,
{
    let mut sum: u32 = 0;
    for _ in 0..SAMPLES {
        sum += adc.read(pin).unwrap_or(0) as u32;
    }
    (sum / SAMPLES as u32) as u16
}
