//! Flash 参数记录编解码（FR-9，与 C 固件字节级兼容）。
//!
//! 布局（小端，12 字节写入 / 9 字节读取）：
//!
//! | 偏移 | 大小 | 字段 |
//! |---|---|---|
//! | 0 | 4 | magic = 0xAA55AA55 |
//! | 4 | 1 | brightness（手动组） |
//! | 5 | 1 | cct（手动组） |
//! | 6 | 1 | dark_brightness（夜灯组） |
//! | 7 | 1 | dark_cct（夜灯组） |
//! | 8 | 1 | checksum = 前 8 字节 XOR |
//! | 9 | 3 | 填充 0（C 版整字写入的高位字节） |

/// 记录魔数。
pub const MAGIC: u32 = 0xAA55_AA55;

/// 灯光参数（手动组 + 夜灯组）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Settings {
    /// 手动组亮度（5..=100）。
    pub brightness: u8,
    /// 手动组色温（0..=100）。
    pub cct: u8,
    /// 夜灯组亮度（5..=100）。
    pub dark_brightness: u8,
    /// 夜灯组色温（0..=100）。
    pub dark_cct: u8,
}

impl Settings {
    /// 缺省值（FR-9.4）：手动 (5, 50)，夜灯 (10, 45)。
    pub const DEFAULT: Settings = Settings {
        brightness: 5,
        cct: 50,
        dark_brightness: 10,
        dark_cct: 45,
    };

    /// 编码为 12 字节（与 C `HAL_FLASH_Program` 逐字写入布局一致）。
    pub fn encode(&self) -> [u8; 12] {
        let mut out = [0u8; 12];
        out[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        out[4] = self.brightness;
        out[5] = self.cct;
        out[6] = self.dark_brightness;
        out[7] = self.dark_cct;
        out[8] = checksum(&out[0..8]);
        out
    }

    /// 从原始字节解码；魔数或校验不符返回 `None`（FR-9.4）。
    pub fn decode(raw: &[u8]) -> Option<Settings> {
        if raw.len() < 9 {
            return None;
        }
        if u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) != MAGIC {
            return None;
        }
        if raw[8] != checksum(&raw[0..8]) {
            return None;
        }
        Some(Settings {
            brightness: raw[4],
            cct: raw[5],
            dark_brightness: raw[6],
            dark_cct: raw[7],
        })
    }
}

/// 前 8 字节 XOR 校验（与 C `CalcChecksum` 一致）。
fn checksum(bytes: &[u8]) -> u8 {
    bytes.iter().take(8).fold(0u8, |a, b| a ^ b)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXPECTED: [u8; 12] = [
        0x55, 0xAA, 0x55, 0xAA, // magic LE
        5, 50, 10, 45, // values
        0x10, // checksum = 5^50^10^45（magic 字节 XOR 归零）
        0, 0, 0,
    ];

    #[test]
    fn c_layout_byte_compatible() {
        let s = Settings::DEFAULT;
        assert_eq!(s.encode(), EXPECTED);
    }

    #[test]
    fn round_trip() {
        let s = Settings {
            brightness: 77,
            cct: 3,
            dark_brightness: 100,
            dark_cct: 0,
        };
        assert_eq!(Settings::decode(&s.encode()), Some(s));
    }

    #[test]
    fn rejects_corruption() {
        let s = Settings::DEFAULT;
        let mut bad = s.encode();
        bad[4] = 6; // 改值不改校验
        assert_eq!(Settings::decode(&bad), None);
        let mut bad = s.encode();
        bad[0] ^= 0xFF; // 改 magic
        assert_eq!(Settings::decode(&bad), None);
        // 长度不足。
        assert_eq!(Settings::decode(&s.encode()[..8]), None);
    }

    #[test]
    fn accepts_legacy_c_firmware_record() {
        // C 版 `HAL_FLASH_Program(WORD, checksum)` 写出 [sum,0,0,0]，
        // 且整页擦除后高位为 0 —— 与 EXPECTED 一致，直接解码。
        assert_eq!(
            Settings::decode(&EXPECTED),
            Some(Settings {
                brightness: 5,
                cct: 50,
                dark_brightness: 10,
                dark_cct: 45,
            })
        );
        // 原型半字写入的 0xFF 填充同样可读（只读前 9 字节）。
        let mut legacy = EXPECTED;
        legacy[9] = 0xFF;
        legacy[10] = 0xFF;
        legacy[11] = 0xFF;
        assert!(Settings::decode(&legacy).is_some());
    }
}
