//
// Created by eartholnpc on 2026/3/6.
//

#include <stdlib.h>

#include "app.h"

#include "adc.h"
#include "flash.h"
#include "key.h"
#include "pwm.h"
#include "rgb.h"

typedef enum {
    STATE_SLEEP,
    STATE_LIGHT_IDLE_NORMAL,
    STATE_LIGHT_ON,
    STATE_LIGHT_ON_DARK,
} SystemState;

typedef enum {
    BRIGHT = 0,
    COLOR
} CtrlMode;

volatile SystemState cur_state = STATE_SLEEP;
// global current brightness and color
uint8_t cur_led_level[2] = {5, 50};
// used for slow off and slow brightening
uint8_t target_brightness = 5;
// global current brightness and color after pyroelectric infrared sensor trig
uint8_t cur_dark_led_level[2] = {10, 50};
uint8_t target_dark_brightness = 5;
uint32_t dark_light_start_time = 0;
// default ctrl led mode is brightness, click fun key to switch mode
CtrlMode g_mode = BRIGHT;

uint32_t high_temp_time = 0;


void enterStandbyMode(void) {
    // 清除所有唤醒标志
    __HAL_PWR_CLEAR_FLAG(PWR_FLAG_WU);
    // 启用 PA0 WKUP1（你的震动/人体传感器引脚）
    HAL_PWR_EnableWakeUpPin(PWR_WAKEUP_PIN1);
    //  进入 Standby
    HAL_PWR_EnterSTANDBYMode();
}


void preSetup(void) {
    // check the reason for reset(power on)
    if (__HAL_PWR_GET_FLAG(PWR_FLAG_SB) != RESET) {
        // if the standby(SB) flag is set, it means waking up from standby mode.
        // clear SB flag
        __HAL_PWR_CLEAR_FLAG(PWR_FLAG_SB);
        // turn off the wake-up pin function and turn it on again
        // until the next time ready to enter standby.
        HAL_PWR_DisableWakeUpPin(PWR_WAKEUP_PIN1);
    }
    else {
        // normal power on reset
        // enable the power management clock
        // (critical step, otherwise the PWR register cannot be operated)
        __HAL_RCC_PWR_CLK_ENABLE();
    }

    // clear wake-up flag (WUF) in preparation for possible next entry into standby mode
    if (__HAL_PWR_GET_FLAG(PWR_FLAG_WU) != RESET) {
        __HAL_PWR_CLEAR_FLAG(PWR_FLAG_WU);
    }
}

void App_LEDPlus(const uint8_t level) {
    cur_led_level[g_mode] += level;
    cur_led_level[g_mode] = cur_led_level[g_mode] >= 100 ? 100 : cur_led_level[g_mode];
    BSP_PWM_SetBrightnessAndCCT(cur_led_level[0], cur_led_level[1]);
}

void App_LEDMinus(const uint8_t level) {
    cur_led_level[g_mode] -= level;
    cur_led_level[g_mode] = cur_led_level[g_mode] <= 5 ? 5 : cur_led_level[g_mode];
    BSP_PWM_SetBrightnessAndCCT(cur_led_level[0], cur_led_level[1]);
}

void App_DarkLEDPlus(const uint8_t level) {
    cur_dark_led_level[g_mode] += level;
    cur_dark_led_level[g_mode] = cur_dark_led_level[g_mode] >= 100 ? 100 : cur_dark_led_level[g_mode];
    BSP_PWM_SetBrightnessAndCCT(cur_dark_led_level[0], cur_dark_led_level[1]);
}

void App_DarkLEDMinus(const uint8_t level) {
    cur_dark_led_level[g_mode] -= level;
    cur_dark_led_level[g_mode] = cur_dark_led_level[g_mode] <= 5 ? 5 : cur_dark_led_level[g_mode];
    BSP_PWM_SetBrightnessAndCCT(cur_dark_led_level[0], cur_dark_led_level[1]);
}


void App_BatteryVoltageCheck(void) {
    // check battery voltage
    const float voltage = BSP_ADC_GetVoltage();
    if (voltage > 4.0) {
        // >4V green
        BSP_RGB_SetGreen();
    }
    else if (voltage > 3.3) {
        // >3.3V yellow (red+green)
        BSP_RGB_SetYellow();
    }
    else if (voltage > 2.7) { // <3.3V red
        BSP_RGB_SetRed();
    }
    else {
        // < 2.7, enter standby mode
        enterStandbyMode();
    }
}

void App_LEDTempCheck(void) {
    const uint32_t resistance = BSP_ADC_GetLEDTempResistance();
    if (resistance <= TEMP_THRESHOLD_MIN_RESISTANCE) {
        // The temperature exceeds 75℃
        BSP_RGB_SetPurple();
        const uint32_t now = HAL_GetTick();
        if (high_temp_time == 0) {
            high_temp_time = now;
        }
        else if (now - high_temp_time >= 60000) {
            uint8_t level = cur_led_level[0] - (10 * (now - high_temp_time) / high_temp_time);
            level = level < 5 ? 5 : level;
            BSP_PWM_SetBrightnessAndCCT(level, cur_led_level[1]);
        }
    }
    else if (resistance >= TEMP_THRESHOLD_MAX_RESISTANCE) {
        // The temperature is below 60 ℃
        high_temp_time = 0;
        BSP_PWM_SetBrightnessAndCCT(cur_led_level[0], cur_led_level[1]);
    }
}


void App_Setup() {
    preSetup();
    BSP_Flash_Load(&cur_led_level[0], &cur_led_level[1]);
    BSP_Flash_LoadDark(&cur_dark_led_level[0], &cur_dark_led_level[1]);
    // If at dark environment, control the to the lowest brightness
    // else long press to start.
    is_dark = BSP_ADC_IsDark();
    if (is_dark == 1) {
        BSP_PWM_SetBrightnessAndCCT(cur_dark_led_level[0], cur_dark_led_level[1]);
        dark_light_start_time = HAL_GetTick();
        cur_state = STATE_LIGHT_ON_DARK;
    }
    else {
        cur_state = STATE_LIGHT_IDLE_NORMAL;
    }
    App_BatteryVoltageCheck();
}


void App_Loop() {
    const KeyEvent ev_fun = BSP_Key_GetAndClearEvent(&key_fun);
    const KeyEvent ev_minus = BSP_Key_GetAndClearEvent(&key_ctrl_m);
    const KeyEvent ev_plus = BSP_Key_GetAndClearEvent(&key_ctrl_p);
    switch (cur_state) {
    case STATE_SLEEP:
        // long press fun key to power off, next enter standby mode only PA0 wakes up
        HAL_PWR_EnableWakeUpPin(PWR_WAKEUP_PIN1);
        BSP_RGB_SetOff();
        BSP_Flash_Save(cur_led_level[0], cur_led_level[1]);
        BSP_Flash_SaveDark(cur_dark_led_level[0], cur_dark_led_level[1]);
        BSP_PWM_SetBrightnessAndCCT(0, 0);

        // switch TPS63802DLAR to PFM mode
        HAL_GPIO_WritePin(GPIOA, GPIO_PIN_10, GPIO_PIN_RESET);
        HAL_PWR_EnterSTANDBYMode();
        break;
    case STATE_LIGHT_ON_DARK:
        // start after pyroelectric infrared sensor trig
        if (ev_fun == KEY_EVENT_LONG_PRESS) {
            BSP_PWM_SetBrightnessAndCCT(cur_led_level[0], cur_led_level[1]);
            cur_state = STATE_LIGHT_ON;
            break;
        }
        if (ev_fun == KEY_EVENT_CLICK) {
            g_mode ^= 1;
        }

        if (ev_plus == KEY_EVENT_CLICK || ev_plus == KEY_EVENT_LONG_HOLD) {
            App_DarkLEDPlus(5);
        }
        if (ev_plus == KEY_EVENT_DOUBLE_CLICK) {
            cur_dark_led_level[0] = 100;
            BSP_PWM_SetBrightnessAndCCT(cur_dark_led_level[0], cur_dark_led_level[1]);
        }

        if (ev_minus == KEY_EVENT_CLICK || ev_minus == KEY_EVENT_LONG_HOLD) {
            App_DarkLEDMinus(5);
        }
        if (ev_minus == KEY_EVENT_DOUBLE_CLICK) {
            cur_dark_led_level[0] = 5;
            BSP_PWM_SetBrightnessAndCCT(cur_dark_led_level[0], cur_dark_led_level[1]);
        }
        if (HAL_GetTick() - dark_light_start_time >= 60000
            && HAL_GPIO_ReadPin(GPIOA, GPIO_PIN_0) == GPIO_PIN_RESET) {
            // night light activates for more than 1 minute and the sensor is not triggered
            cur_state = STATE_SLEEP;
            BSP_PWM_SetBrightnessAndCCT(0, 0);
            BSP_Flash_SaveDark(cur_dark_led_level[0], cur_dark_led_level[1]);
        }
        break;
    case STATE_LIGHT_IDLE_NORMAL:
        // normal start， wait long press fun key to open light
        if (ev_fun == KEY_EVENT_LONG_PRESS) {
            BSP_PWM_SetBrightnessAndCCT(cur_led_level[0], cur_led_level[1]);
        }
        break;
    case STATE_LIGHT_ON:
        if (ev_fun == KEY_EVENT_LONG_PRESS) {
            cur_state = STATE_SLEEP;
        }
        if (ev_fun == KEY_EVENT_CLICK) {
            g_mode ^= 1;
        }

        if (ev_plus == KEY_EVENT_CLICK || ev_plus == KEY_EVENT_LONG_HOLD) {
            App_LEDPlus(5);
        }
        if (ev_plus == KEY_EVENT_DOUBLE_CLICK) {
            BSP_PWM_SetBrightnessAndCCT(100, cur_led_level[1]);
        }

        if (ev_minus == KEY_EVENT_CLICK || ev_minus == KEY_EVENT_LONG_HOLD) {
            App_LEDMinus(5);
        }
        if (ev_minus == KEY_EVENT_DOUBLE_CLICK) {
            BSP_PWM_SetBrightnessAndCCT(5, cur_led_level[1]);
        }
        break;
    }
    App_LEDTempCheck();
    if (high_temp_time == 0) {
        App_BatteryVoltageCheck();
    }
}