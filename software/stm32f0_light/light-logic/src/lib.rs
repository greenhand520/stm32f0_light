//! `light-logic` — 夜灯固件纯逻辑层（no_std，主机可测）。
//!
//! 模块划分见 `docs/design.md` §2：
//! - [`key`]：按键状态机（去抖/单击/双击/长按/长按重复）
//! - [`pwm`]：冷暖双通道占空比与亮度/色温钳位
//! - [`sensor`]：电池电压、NTC 阻值换算与电量分档
//! - [`settings`]：Flash 参数记录编解码（与 C 固件字节级兼容）
//! - [`fade`]：1 s 线性亮度渐变
//! - [`app`]：状态机、过热/低电保护、RGB 指示决策
#![cfg_attr(not(test), no_std)]

pub mod app;
pub mod fade;
pub mod key;
pub mod pwm;
pub mod sensor;
pub mod settings;
