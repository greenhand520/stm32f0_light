//
// Created by eartholnpc on 2026/3/16.
//

#include "key.h"

#include "main.h"
#include "stm32f0xx_hal_gpio.h"

Key key_fun   = {GPIOA, FUN_Pin,  0,0,0,0,0, KEY_EVENT_NONE};
Key key_ctrl_m = {GPIOA, CTRL_M_Pin,  0,0,0,0,0, KEY_EVENT_NONE};
Key key_ctrl_p = {GPIOA, CTRL_P_Pin,  0,0,0,0,0, KEY_EVENT_NONE};

void BSP_Key_Init(void)
{
    // Prerequisite: PA4/5/6 has been configured as input + drop-up in CubeMX or main fun
    key_fun.last_event   = KEY_EVENT_NONE;
    key_ctrl_m.last_event = KEY_EVENT_NONE;
    key_ctrl_p.last_event = KEY_EVENT_NONE;
}

static void BSP_Key_Process(Key *key)
{
    // Assume that the low level is pressed
    const uint8_t now = HAL_GPIO_ReadPin(key->port, key->pin) == GPIO_PIN_RESET;

    // debounce
    if (now == key->prev_state) {
        if (key->debounce_cnt < DEBOUNCE_TH) {
            key->debounce_cnt++;
        }
    } else {
        key->debounce_cnt = 0;
    }

    key->prev_state = now;

    // The level is considered valid only after debounce is stabilized
    if (key->debounce_cnt < DEBOUNCE_TH) {
        return;
    }

    // ──────────────────────────────────────────────
    //  Stabilized button state machine
    // ──────────────────────────────────────────────

    if (now) {   // pressed
        if (key->press_duration < 0xFFFF) {
            key->press_duration++;
        }

        // 长按期间可以持续触发（可选，根据需求）
        if (key->press_duration >= LONG_PRESS_TICK && !key->long_pressed_flag) {
            key->last_event = KEY_EVENT_LONG_PRESS;
            key->long_pressed_flag = 1;
            // 如果你需要长按期间每隔一段时间触发一次，可以在这里加计数器
        }
    }
    else {       // 释放状态
        if (key->press_duration >= DEBOUNCE_TH) {   // 之前确实按过
            if (key->press_duration < LONG_PRESS_TICK) {
                // 短按范围
                if (key->click_wait_cnt > 0) {
                    // 在双击窗口内又来一次 → 双击
                    key->last_event = KEY_EVENT_DOUBLE_CLICK;
                    key->click_wait_cnt = 0;
                } else {
                    // 第一次短按，进入等待双击窗口
                    key->last_event = KEY_EVENT_CLICK;
                    key->click_wait_cnt = DOUBLE_CLICK_TICK;
                }
            }
            else {
                // 长按释放
                if (key->long_pressed_flag) {
                    key->last_event = KEY_EVENT_LONG_PRESS;  // 有些人喜欢在松手时再触发一次长按事件
                }
            }
        }

        // 状态复位
        key->press_duration = 0;
        key->long_pressed_flag = 0;
    }
}

void BSP_Key_Scan_10ms(void)
{
    BSP_Key_Process(&key_fun);
    BSP_Key_Process(&key_ctrl_m);
    BSP_Key_Process(&key_ctrl_p);

    // 双击等待计时递减
    if (key_fun.click_wait_cnt   > 0) key_fun.click_wait_cnt--;
    if (key_ctrl_m.click_wait_cnt > 0) key_ctrl_m.click_wait_cnt--;
    if (key_ctrl_p.click_wait_cnt > 0) key_ctrl_p.click_wait_cnt--;
}

KeyEvent BSP_Key_GetAndClearEvent(Key *key)
{
    const KeyEvent ev = key->last_event;
    key->last_event = KEY_EVENT_NONE;
    return ev;
}

KeyEvent BSP_Key_PeekEvent(Key *key)
{
    return key->last_event;
}