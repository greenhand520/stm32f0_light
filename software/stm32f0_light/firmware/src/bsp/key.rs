//! 三按键驱动（PA4 FUN / PA5 CTRL− / PA6 CTRL+，低有效，内部上拉）。
//!
//! 每 10 ms 调用 [`BspKeys::scan`]；事件识别走 [`light_logic::key`]。

use core::convert::Infallible;

use embedded_hal::digital::v2::InputPin;
use light_logic::key::{KeyFsm, KeyEvent};
use stm32f0xx_hal as hal;

use hal::gpio::gpioa::{PA4, PA5, PA6};
use hal::gpio::{Input, PullUp};

/// 三个按键的集合。
pub struct BspKeys {
    fun_pin: PA4<Input<PullUp>>,
    ctrl_m_pin: PA5<Input<PullUp>>,
    ctrl_p_pin: PA6<Input<PullUp>>,
    fun: KeyFsm,
    ctrl_m: KeyFsm,
    ctrl_p: KeyFsm,
}

impl BspKeys {
    /// 由三个上拉输入引脚构造。
    pub fn new(
        fun_pin: PA4<Input<PullUp>>,
        ctrl_m_pin: PA5<Input<PullUp>>,
        ctrl_p_pin: PA6<Input<PullUp>>,
    ) -> Self {
        BspKeys {
            fun_pin,
            ctrl_m_pin,
            ctrl_p_pin,
            fun: KeyFsm::new(),
            ctrl_m: KeyFsm::new(),
            ctrl_p: KeyFsm::new(),
        }
    }

    /// 扫描一次（每 10 ms）。
    pub fn scan(&mut self) {
        let (f, m, p) = self.read();
        self.fun.scan(f);
        self.ctrl_m.scan(m);
        self.ctrl_p.scan(p);
    }

    /// 取出并清零三键事件（FUN, CTRL−, CTRL+）。
    pub fn take_events(&mut self) -> (KeyEvent, KeyEvent, KeyEvent) {
        (
            self.fun.take_event(),
            self.ctrl_m.take_event(),
            self.ctrl_p.take_event(),
        )
    }

    fn read(&mut self) -> (bool, bool, bool) {
        let f = Self::pressed(&mut self.fun_pin);
        let m = Self::pressed(&mut self.ctrl_m_pin);
        let p = Self::pressed(&mut self.ctrl_p_pin);
        (f, m, p)
    }

    fn pressed<Pin: InputPin<Error = Infallible>>(pin: &mut Pin) -> bool {
        pin.is_low().unwrap_or(false)
    }
}
