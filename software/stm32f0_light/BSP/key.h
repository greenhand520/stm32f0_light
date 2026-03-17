//
// Created by eartholnpc on 2026/3/16.
//

#pragma once

#include "stm32f030x6.h"

typedef enum {
    KEY_EVENT_NONE = 0,
    KEY_EVENT_PRESS,       // 按下（去抖后确认按下，可用于检测hold）
    KEY_EVENT_RELEASE,     // 释放（短按的结束信号）
    KEY_EVENT_CLICK,       // 单击（短按完成）
    KEY_EVENT_LONG_PRESS,  // 长按完成（通常在松手时触发）
    KEY_EVENT_LONG_HOLD,   // 长按期间持续触发（可选）
    KEY_EVENT_DOUBLE_CLICK // 双击完成
} KeyEvent;

typedef struct {
    GPIO_TypeDef       *port;
    uint16_t            pin;

    // 内部状态
    uint8_t             debounce_cnt;
    uint16_t            press_duration;     // 按下持续时间（10ms tick）
    uint8_t             prev_state;         // 上一次读取的电平（用于边沿检测）
    uint8_t             click_wait_cnt;     // 双击等待计时器
    uint8_t             long_pressed_flag;  // 是否已经触发过长按

    // 最近一次产生的事件（主循环读取后应清零或消费）
    volatile KeyEvent last_event;
} Key;

#define DEBOUNCE_MS         30      // 去抖时间 30ms
#define LONG_PRESS_MS       800     // 长按判定 800ms
#define DOUBLE_CLICK_MS     350     // 双击窗口 350ms
#define SCAN_PERIOD_MS      10

// 去抖计数阈值
#define DEBOUNCE_TH         (DEBOUNCE_MS / SCAN_PERIOD_MS)

// 长按/双击计时（单位：10ms tick）
#define LONG_PRESS_TICK     (LONG_PRESS_MS / SCAN_PERIOD_MS)
#define DOUBLE_CLICK_TICK   (DOUBLE_CLICK_MS / SCAN_PERIOD_MS)

// 对外声明的三个按键
extern Key key_fun;
extern Key key_ctrl_m;   // CTRL-
extern Key key_ctrl_p;   // CTRL+

void BSP_Key_Init(void);
// 在 SysTick_Handler 或 10ms 定时器中调用
void BSP_Key_Scan_10ms(void);

// 获取并消费事件（调用后事件会被清零）
KeyEvent BSP_Key_GetAndClearEvent(Key *key);

// 只查看事件，不消费（调试用）
KeyEvent BSP_Key_PeekEvent(Key *key);