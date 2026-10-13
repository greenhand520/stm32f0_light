# stm32f0_light 设计文档（Rust 固件）

- 依据：`docs/requirements.md`（已确认，2026-10-13）
- 目标：STM32F030F4P6 冷暖色温夜灯 Rust 固件；行为 = C 固件（Q1–Q6 修复）+ 增强 FR-10/11/12、FR-8.3、FR-6.3
- 流程：本设计确认后开始实现，每阶段 git 提交

---

## 1. 总体架构

```mermaid
graph TD
    subgraph firmware["firmware（bin，thumbv6m-none-eabi）"]
        MAIN[main.rs 主循环<br/>10ms tick 驱动]
        HAL[hal.rs 时钟/外设初始化]
        BP[pwm]  BK[key]  BA[adc]  BL[led]  BF[flash]  BW[power]
        RTT[rtt 日志 feature rtt]
    end
    subgraph logic["logic（lib，no_std，主机可测）"]
        K[key.rs 按键 FSM]
        P[pwm.rs 占空比/钳位]
        S[sensor.rs 换算/阈值]
        C[settings.rs 记录编解码]
        F[fade.rs 渐变]
        A[app.rs 状态机/保护/指示]
    end
    MAIN --> A
    A --> K & P & S & C & F
    MAIN --> BP & BK & BA & BL & BF & BW & RTT
    BK --> K
    BP --> P
    BA --> S
    BF --> C
```

- Cargo workspace：`software/stm32f0_light/{logic, firmware}`（`logic` = crate `light-logic`；`firmware` = crate `stm32f0_light`）。
- 依赖（与原型一致，已验证可编译）：`cortex-m 0.7`、`cortex-m-rt 0.7`、`embedded-hal 0.2`、`stm32f0xx-hal 0.18`（`rt`, `stm32f030x4`）、`panic-halt`、`rtt-target 0.4`（feature `rtt`）。
- 硬件访问只发生在 `firmware/src/bsp/`；`logic` 不含寄存器操作，可在 x86 主机跑 `cargo test`。

## 2. `logic` 模块设计

### 2.1 key.rs — 按键状态机

```rust
pub enum KeyEvent { None, Click, DoubleClick, LongPress, LongHold }
pub struct KeyFsm { /* 内部计数器 */ }
impl KeyFsm {
    pub const fn new() -> Self;
    pub fn scan(&mut self, pressed: bool);   // 每 10 ms 一次
    pub fn take_event(&mut self) -> KeyEvent;
}
```

时序常量（10 ms tick）：去抖 3、长按 80（800 ms）、LongHold 重复 20（200 ms）、双击窗口 35（350 ms）。

| 规则 | 行为 |
|---|---|
| 去抖 | 电平连续 3 tick 相同才接受状态变化 |
| 长按 | 按住第 80 tick 产生一次 `LongPress`；之后每 20 tick 产生 `LongHold` 直至释放 |
| 释放 | 长按释放**不再**产生事件（修正 C 版重复触发） |
| 单击 | 短按释放后不立即上报；双击窗口 35 tick 超时后产生 `Click` |
| 双击 | 窗口内第二次短按释放 → `DoubleClick`（不附带 `Click`） |
| 计数 | 时长计数饱和不溢出；窗口递减每个 scan 都进行（按下期间到期则补发 `Click`） |

### 2.2 pwm.rs — 占空比与钳位

```rust
pub const PWM_PERIOD: u16 = 4799;
pub fn duty_cycles(bright: u8, cct: u8, max_duty: u16) -> (u16, u16); // (暖, 冷)
pub fn clamp_brightness(v: i16) -> u8; // 5..=100
pub fn clamp_cct(v: i16) -> u8;        // 0..=100（Q3 修复）
```

公式与 C 逐级整数截断一致：`warm = bright*cct/100*max/100`，`cold = bright*(100-cct)/100*max/100`。暖 = TIM14_CH1/PA7，冷 = TIM1_CH2/PA9（Q7 决议）。

### 2.3 sensor.rs — 换算与阈值

```rust
pub fn battery_voltage(adc: u16) -> f32;   // adc*3.3/4095*3.13
pub fn ntc_resistance(adc: u16) -> u32;    // adc*100_000/(4095-adc)，边界 0/999999
pub fn is_dark(adc: u16) -> bool;          // adc > 4000
pub fn battery_level(v: f32) -> BatteryLevel; // High/Mid/Low/Critical
```

常量：`LIGHT_THRESHOLD = 4000`；电量档 `3.7 / 3.3 / 2.7` V；NTC `15_000 / 25_000` Ω（Q4 修复）；低电触发 `2.7` V、解除 `3.1` V（D2）。

### 2.4 settings.rs — Flash 记录编解码

```rust
pub struct Settings { pub brightness: u8, pub cct: u8, pub dark_brightness: u8, pub dark_cct: u8 }
impl Settings {
    pub const DEFAULT: Self;                      // (5, 50, 10, 45)
    pub fn encode(&self) -> [u8; 12];             // 3×32bit 字布局（与 C 字节级兼容）
    pub fn decode(raw: &[u8]) -> Option<Self>;    // magic 0xAA55AA55 + XOR 校验
}
```

布局不变（FR-9.2）：magic u32 LE @0、四值 @4–7、XOR 校验 @8、@9–11 填充。旧 C 固件数据可直接读取。

### 2.5 fade.rs — 渐变（FR-10）

```rust
pub struct FadeState { /* start, target, start_ms */ }
impl FadeState {
    pub fn start(&mut self, from: u8, to: u8, now_ms: u32); // 1000 ms 线性
    pub fn update(&mut self, now_ms: u32) -> u8;            // 完成后返回 target
    pub fn is_active(&self) -> bool;
}
```

仅作用于亮度维度；色温瞬时生效。触发点（FR-10.1）：开机点亮、关机/睡眠渐灭（**渐灭完成后**保存并 Standby）、夜灯↔手动亮度过渡、过热恢复。

### 2.6 app.rs — 状态机与保护逻辑

```rust
pub enum State { Sleep, LightIdleNormal, LightOn, LightOnDark }
pub enum CtrlMode { Brightness, ColorTemp }
pub enum RgbColor { Off, Red, Green, Blue, Yellow, Purple, White }
pub enum SleepReason { UserPowerOff, DarkTimeout, LowBattery }

pub struct TickInput {
    pub now_ms: u32,
    pub fun: KeyEvent, pub minus: KeyEvent, pub plus: KeyEvent,
    pub pir_active: bool,
    pub battery_v: f32,        // 1 s 节流后的值
    pub ntc_resistance: u32,   // 1 s 节流后的值
}

pub struct Output {
    pub state: State,
    pub light: Option<(u8, u8)>,     // Some → 更新 PWM
    pub rgb: RgbColor,
    pub sleep: Option<SleepReason>,  // Some → 固件执行睡眠序列
}
```

`App::new(settings)`、`App::setup(is_dark, battery_v, now_ms) -> Output`、`App::tick(input) -> Output`、`App::settings() -> Settings`。

**tick 执行顺序**（对应 C 主循环顺序）：

1. 渐变推进（`fade.is_active` 时输出亮度、**忽略按键**，FR-10.2）；
2. 状态机事件处理（§6 矩阵；Q1 修复：Idle 长按进 LightOn）；
3. 温度保护：过热（≤15 kΩ）记时/紫色；持续 ≥60 s 按公式降亮（Q5 修复：`base − 10×(dur/60 s)`，下限 5，`base` 取当前模式组）；恢复（≥25 kΩ）→ 渐变回当前模式组（Q5）；
4. 电量处理：档位色（基础层）；<2.7 V 进低电流程（红 1 Hz 闪烁 20 s → `SleepReason::LowBattery`），≥3.1 V 解除；
5. RGB 按 D1 层叠优先级输出：低电闪烁 > 过热紫 > 模式指示（FUN 单击后 2 s，手动白/黄、夜灯紫/蓝）> 电池色；
6. 夜灯超时：`now − dark_start ≥ 60 s && !pir_active` → 渐灭 → `SleepReason::DarkTimeout`；PIR 高电平随时重置 `dark_start`（Q6）。

**睡眠路径**：FUN 长按关机（`UserPowerOff`）/ 夜灯超时 / 低电，统一为：渐灭（低电路径闪烁结束后同样渐灭）→ `Output.sleep = Some(..)`。

## 3. `firmware` 模块设计

### 3.1 hal.rs — 时钟与外设初始化

- 时钟：HSI 8 MHz → PLL ×12 = 48 MHz SYSCLK，Flash 1 wait state（与 C `SystemClock_Config` 一致）。
- 产出 `HalPeripherals { rcc, gpioa/b/f parts, tim1, tim14, adc, flash, pwr, sys_tick }`；引脚模式在 bsp 构造时配置。
- SysTick：RELOAD = 480 000 − 1（10 ms）；`#[exception] SysTick` 仅递增 `TICK_COUNT: u32`（单写多读，32 位原子）。

### 3.2 bsp 驱动

| 模块 | 职责 | 要点 |
|---|---|---|
| `bsp/pwm.rs` | 暖 TIM14_CH1(PA7)/冷 TIM1_CH2(PA9) 10 kHz PWM | duty 由 `logic::pwm` 计算；`off()` |
| `bsp/key.rs` | PA4/5/6 上拉输入 + 3×`KeyFsm` | `scan()` 读引脚喂 FSM；`take_events() -> (KeyEvent×3)` |
| `bsp/adc.rs` | CH1/CH2/CH3 各 8 样本均值 | 换算调 `logic::sensor`；采样时间尽量设 239.5 周期（C 等效） |
| `bsp/led.rs` | PB1/PF0/PF1 低有效 | `set(RgbColor)` |
| `bsp/flash.rs` | 0x08003C00 页擦写 | 半字编程（F0 要求）；编解码走 `logic::settings`；读改写保留另一组 |
| `bsp/power.rs` | `pre_setup()` / `enter_standby()` | 修正原型错误：PWR 时钟在 **APBENR** bit28；SB/WUF 清除、EWUP1 管理（FR-1.1） |

### 3.3 main.rs — 主循环

```text
init hal → 组装 bsp → flash.load() → app.setup(is_dark, battery_v, now)
应用 setup 输出（含开机低电流程）
loop {
    等 tick 变化（10 ms）
    keys.scan() → 事件
    每 1 s：adc 采电量/NTC（FR-6.3 节流；过热期间跳过电量判定不跳过采样）
    out = app.tick(input)
    应用 out：light → pwm；rgb → led；sleep → 睡眠序列
    RTT 日志（状态变化、按键、1 s 采样值、睡眠原因）
}
```

**睡眠序列**（`sleep_seq(reason)`，不返回）：保存两组设置（FR-9.3）→ PWM off → LED off → PA10 低 → EWUP1 使能、清 WUF → PDDS + SLEEPDEEP → WFI。开机时 `pre_setup()` 复位 SB/EWUP1/WUF。

### 3.4 PA10（TPS63802）

保持 C 现状：初始化拉低（PFM）后不再拉高，睡眠序列再拉低（幂等）。C 从未将其拉高，属实机行为疑点，冒烟时用示波器/万用表确认后如有必要再议。

## 4. 关键算法与参数

| 算法 | 定义 |
|---|---|
| PWM 占空比 | §2.2，逐级整数除法与 C 完全一致 |
| 渐变 | 1 s 线性：`cur = from + (to−from)×elapsed/1000`，到期回 `to` |
| 过热降亮（Q5） | `dur < 60 s → base`；否则 `max(5, base − 10×dur/60_000)`，`base` = 当前模式组亮度 |
| 低电（D2） | <2.7 V 触发；红灯 1 Hz（亮 500 ms/灭 500 ms）×20 s；≥3.1 V 解除并恢复指示 |
| 模式指示 | FUN 单击后显示 2 s：手动 亮度=白/色温=黄；夜灯 亮度=紫/色温=蓝 |
| 电量色 | >3.7 绿；>3.3 黄；>2.7 红；<2.7 低电流程（FR-6.2） |

## 5. 资源与构建

- `firmware/Cargo.toml`：`opt-level = "s"`、`lto = true`、`codegen-units = 1`、`panic = "abort"`（经 panic-halt）。
- `memory.x`：FLASH 16K @0x08000000、RAM 4K @0x20000000；`build.rs` 拷贝 `memory.x`。
- `firmware/.cargo/config.toml`：`target = "thumbv6m-none-eabi"`。
- 验收硬指标：固件 ≤ 16 KB Flash、静态 RAM ≤ 4 KB（`arm-none-eabi-size` 度量）。

## 6. 测试设计

### 6.1 `logic` 主机单测（`cargo test -p light-logic`）

| 模块 | 用例（行为/边界，非实现细节） |
|---|---|
| key | 抖动 <3 tick 不产生事件；Click 在 35 tick 窗口超时后产生且不提前；DoubleClick 优于 Click；LongPress 恰在 80 tick；LongHold 每 20 tick 重复；长按释放不重复；计数饱和 |
| pwm | 与 C 公式等值样例；cct=0/100 极端；bright=5/100 边界；钳位函数 |
| settings | round-trip；C 字节布局逐字节兼容（固定期望向量）；magic/校验损坏 → None；默认值 |
| sensor | 电压/阻值换算已知点；ADC 0/4095 边界；暗亮阈值 4000；电量分档 3.7/3.3/2.7 |
| fade | 起止、中点线性、1000 ms 完成、完成后 is_active=false |
| app | 状态矩阵逐格（§6 需求矩阵）；Q1/Q3/Q5/Q6 修复点各至少一例；低电闪烁→睡眠、解除；RGB D1 优先级；渐变期间按键忽略 |

### 6.2 构建验证

- `cargo test -p light-logic`（x86）全绿。
- `cargo build --release`（thumbv6m）通过；`arm-none-eabi-size` 确认体积达标。
- `cargo build --release --no-default-features`（关闭 RTT）可编译，验证 feature 正交。

### 6.3 实机冒烟清单（ST-Link + OpenOCD/STM32CubeProgrammer）

1. 上电 → 电量色指示正确（>3.7 V 绿）；
2. FUN 长按开灯（渐亮）→ ± 调亮度/色温（FUN 单击切换，指示白/黄）→ 双击直达 100/5；
3. FUN 长按关机（渐灭）→ 重上电设置保持；
4. 遮蔽光敏 + PIR 触发 → 夜灯组点亮（紫/蓝子模式指示）→ 60 s 无触发渐灭睡眠；
5. Standby 电流近规格值；PA0 PIR 唤醒正常（唤醒=复位，走启动流程）；
6. 加热 NTC → 紫色 + 60 s 后降亮；冷却 → 恢复；
7. 供电降至 <2.7 V → 红灯闪烁 20 s → 睡眠；
8. RTT 日志可见状态切换/按键/采样值/睡眠原因。

## 7. 实现阶段（每阶段一个 git commit）

| 阶段 | 内容 | 验证 |
|---|---|---|
| S1 | workspace + `logic`：key / pwm / settings / sensor + 单测 | `cargo test -p light-logic` |
| S2 | `logic`：fade / app 状态机与保护逻辑 + 单测 | 同上，覆盖 §6.1 app 行 |
| S3 | `firmware`：hal + bsp（pwm/key/adc/led/flash/power） | thumbv6m 编译通过 |
| S4 | `firmware`：main 集成 + RTT 日志 | 编译 + 体积达标 + no-rtt 可编译 |
| S5 | 验证与收尾：冒烟（如硬件在位）、README、需求文档勘误 | §6.2/6.3 |

## 8. 风险与取舍

| 风险 | 处置 |
|---|---|
| `stm32f0xx-hal 0.18` API 与细节（ADC 采样时间等） | 沿用原型已编译代码；采样时间不支持 239.5 时用库默认并在冒烟中校验读数 |
| PA7/PA9 冷暖映射与实物不符（Q7） | 按决议 PA7=暖；常量集中在 `logic::pwm` 一处，实机反了改一行 |
| PA10 恒 PFM（C 现状） | 保持现状；冒烟确认纹波/亮度无异常 |
| 16 KB Flash 余量 | `opt-level="s"`+LTO；RTT 可关；S4 出体积数据 |
| 无硬件在位时冒烟无法执行 | 单测+编译先行交付，冒烟清单随固件交付，硬件就绪后执行 |
