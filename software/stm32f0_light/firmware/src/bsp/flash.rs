//! 参数持久化（内部 Flash 末页 0x08003C00，FR-9）。
//!
//! 编解码走 [`light_logic::settings`]（与 C 固件字节级兼容）；本模块只做
//! 擦除/编程。STM32F0 要求半字（16 位）编程。

use light_logic::settings::Settings;
use stm32f0xx_hal as hal;

use hal::pac;

/// 末页基址（16 KB Flash 的最后 1 KB 页）。
const PAGE_ADDR: u32 = 0x0800_3C00;
/// 记录长度（12 字节 = 6 个半字）。
const RECORD_LEN: usize = 12;

/// Flash 解锁键（RM0360 §3.4.2）。
const KEY1: u32 = 0x4567_0123;
const KEY2: u32 = 0xCDEF_89AB;

/// 参数存储。
pub struct BspFlash {
    flash: pac::FLASH,
}

impl BspFlash {
    /// 占用 FLASH 外设。
    pub fn new(flash: pac::FLASH) -> Self {
        BspFlash { flash }
    }

    /// 加载参数；魔数/校验不符时返回 [`Settings::DEFAULT`]（FR-9.4）。
    pub fn load(&self) -> Settings {
        let mut raw = [0u8; RECORD_LEN];
        let ptr = PAGE_ADDR as *const u8;
        for (i, b) in raw.iter_mut().enumerate() {
            *b = unsafe { core::ptr::read_volatile(ptr.add(i)) };
        }
        Settings::decode(&raw).unwrap_or(Settings::DEFAULT)
    }

    /// 保存两组参数（擦除整页后重写，FR-9.3）。
    pub fn save(&mut self, settings: &Settings) {
        let bytes = settings.encode();
        self.unlock();
        self.erase_page();
        for i in 0..RECORD_LEN / 2 {
            let hw = u16::from_le_bytes([bytes[i * 2], bytes[i * 2 + 1]]);
            self.program_halfword(PAGE_ADDR + (i * 2) as u32, hw);
        }
        self.lock();
    }

    fn unlock(&self) {
        self.flash.keyr.write(|w| unsafe { w.bits(KEY1) });
        self.flash.keyr.write(|w| unsafe { w.bits(KEY2) });
    }

    fn lock(&self) {
        self.flash.cr.modify(|_, w| w.lock().set_bit());
    }

    fn wait_not_busy(&self) {
        while self.flash.sr.read().bsy().bit_is_set() {}
    }

    fn erase_page(&self) {
        self.wait_not_busy();
        self.flash.cr.modify(|_, w| w.per().set_bit());
        self.flash.ar.write(|w| unsafe { w.bits(PAGE_ADDR) });
        self.flash.cr.modify(|_, w| w.strt().set_bit());
        self.wait_not_busy();
        self.flash.cr.modify(|_, w| w.per().clear_bit());
        self.flash.sr.modify(|_, w| w.eop().set_bit());
    }

    fn program_halfword(&self, addr: u32, data: u16) {
        self.wait_not_busy();
        self.flash.cr.modify(|_, w| w.pg().set_bit());
        // 地址为 Flash 区域且 2 字节对齐。
        unsafe { core::ptr::write_volatile(addr as *mut u16, data) };
        self.wait_not_busy();
        self.flash.cr.modify(|_, w| w.pg().clear_bit());
        self.flash.sr.modify(|_, w| w.eop().set_bit());
    }
}
