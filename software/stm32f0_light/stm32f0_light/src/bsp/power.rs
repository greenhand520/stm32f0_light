//! 电源管理（FR-1）：复位源处理与 Standby 进入。
//!
//! 修正原型错误：STM32F0 的 PWR 时钟在 **RCC_APBENR** bit28（不是 AHBENR）。

use stm32f0xx_hal as hal;

use hal::pac;
use hal::pac::RCC;

/// 使能 PWR 外设时钟（访问 PWR 寄存器的前提；RM0360：RCC_APB1ENR bit28）。
pub fn enable_pwr_clock() {
    // SAFETY: RCC 单例寄存器，读改写一个独立位。
    unsafe {
        (*RCC::PTR).apb1enr.modify(|_, w| w.pwren().set_bit());
    }
}

/// 上电/唤醒后的复位源处理（FR-1.1）。
///
/// - 从 Standby 唤醒（SB 置位）：清 SB，禁用 WKUP 引脚（下次睡前再使能）；
/// - 正常上电：仅使能 PWR 时钟；
/// - 两种路径均清 WUF。
pub fn pre_setup(pwr: &pac::PWR) {
    enable_pwr_clock();

    if pwr.csr.read().sbf().bit_is_set() {
        // Standby 唤醒：清 SB 标志，禁用 WKUP1。
        pwr.cr.modify(|_, w| w.csbf().set_bit());
        pwr.csr.modify(|_, w| w.ewup1().clear_bit());
    }

    // 清唤醒标志。
    pwr.cr.modify(|_, w| w.cwuf().set_bit());
}

/// 进入 Standby（FR-1.2 后半段：EWUP1 使能 → 清标志 → PDDS → SLEEPDEEP → WFI）。
///
/// 仅 PA0 上升沿唤醒；唤醒等效复位。此函数不返回。
pub fn enter_standby(pwr: &pac::PWR) -> ! {
    enable_pwr_clock();

    // 使能 PA0 WKUP1。
    pwr.csr.modify(|_, w| w.ewup1().set_bit());
    // 清 WUF/SBF。
    pwr.cr.modify(|_, w| w.cwuf().set_bit().csbf().set_bit());
    // 选择 Standby（PDDS=1）。
    pwr.cr.modify(|_, w| w.pdds().set_bit());
    // SLEEPDEEP（SCB 单例寄存器位操作）。
    // SAFETY: SCB 单例寄存器，仅置位 SLEEPDEEP（SCR bit2）。
    unsafe {
        (*cortex_m::peripheral::SCB::PTR).scr.modify(|v| v | (1 << 2));
    }

    loop {
        cortex_m::asm::wfi();
    }
}
