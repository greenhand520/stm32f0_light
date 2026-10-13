# stm32f0_light

电池供电的冷暖色温可调 LED 夜灯/台灯固件。主控 STM32F030F4P6（Cortex-M0，16 KB Flash / 4 KB RAM），
冷暖双通道 PWM 调光调色温、PIR 夜灯模式、电量/温度保护、Standby 低功耗 + PA0 唤醒、设置掉电保存。

- 需求文档：[`docs/requirements.md`](docs/requirements.md)
- 设计文档：[`docs/design.md`](docs/design.md)

## 仓库结构

| 路径 | 内容 |
|---|---|
| `software/stm32f0_light/logic/` | `light-logic`：纯逻辑（状态机/按键/公式/编解码），主机可单测 |
| `software/stm32f0_light/firmware/` | `stm32f0_light`：no_std 固件（HAL + BSP + 主循环 + RTT 日志） |
| `hardware/datasheet/` | 器件规格书（AS312 PIR、NTC 100K B3950） |
| `docs/` | 需求/设计文档 |

## 构建与测试

```bash
cd software/stm32f0_light

# 主机单元测试（必须在 workspace 根目录运行）
cargo test -p light-logic

# 固件构建（thumbv6m-none-eabi，firmware/.cargo/config.toml 已固定目标）
cd firmware
cargo build                 # dev（已开尺寸优化 + LTO）
cargo build --release       # 发布：Flash 12 884 B / RAM 1 096 B（含 RTT）
cargo build --release --no-default-features   # 关闭 RTT：Flash 6 244 B / RAM 8 B
```

实测体积（`arm-none-eabi-size`）均满足 16 KB Flash / 4 KB RAM。

## 烧录与调试

```bash
# OpenOCD + ST-Link
openocd -f interface/stlink.cfg -f target/stm32f0x.cfg \
  -c "program target/thumbv6m-none-eabi/release/stm32f0_light verify reset exit"

# 或 probe-rs（cargo run 默认 runner）
cargo run --release
```

RTT 日志（feature `rtt`，默认开）：用 probe-rs/RTT Viewer 连接，输出启动参数、按键事件、
PWM 变更、1 s 电量/NTC 采样、状态切换与睡眠原因。

## 实机冒烟

清单见 `docs/design.md` §6.3（开/关灯渐变、亮度色温调节、夜灯触发与超时、PA0 唤醒、
NTC 过热降亮与恢复、低电闪烁睡眠、断电保持设置、RTT 日志）。**当前无探针在位，冒烟待硬件就绪后执行。**
