//! RGB 状态灯驱动（PB1 红 / PF0 绿 / PF1 蓝，低电平点亮）。

use light_logic::app::RgbColor;
use stm32f0xx_hal as hal;

use hal::gpio::gpiof::{PF0, PF1};
use hal::gpio::gpiob::PB1;
use hal::gpio::{Floating, Input, Output, PushPull};

/// 状态灯控制器（active-low）。
pub struct BspLed {
    red: PB1<Output<PushPull>>,
    green: PF0<Output<PushPull>>,
    blue: PF1<Output<PushPull>>,
}

impl BspLed {
    /// 由原始引脚构造，初始全灭。
    pub fn new(
        red: PB1<Input<Floating>>,
        green: PF0<Input<Floating>>,
        blue: PF1<Input<Floating>>,
    ) -> Self {
        let (red, green, blue) = cortex_m::interrupt::free(|cs| {
            (
                red.into_push_pull_output(cs),
                green.into_push_pull_output(cs),
                blue.into_push_pull_output(cs),
            )
        });
        let mut led = BspLed { red, green, blue };
        led.set(RgbColor::Off);
        led
    }

    /// 设置颜色（逻辑电平，内部换算为低有效）。
    pub fn set(&mut self, color: RgbColor) {
        let (r, g, b) = match color {
            RgbColor::Off => (false, false, false),
            RgbColor::Red => (true, false, false),
            RgbColor::Green => (false, true, false),
            RgbColor::Blue => (false, false, true),
            RgbColor::Yellow => (true, true, false),
            RgbColor::Purple => (true, false, true),
            RgbColor::White => (true, true, true),
        };
        Self::drive(&mut self.red, r);
        Self::drive(&mut self.green, g);
        Self::drive(&mut self.blue, b);
    }

    fn drive<Pin>(pin: &mut Pin, on: bool)
    where
        Pin: embedded_hal::digital::v2::OutputPin,
    {
        if on {
            let _ = pin.set_low();
        } else {
            let _ = pin.set_high();
        }
    }
}
