//
// Created by eartholnpc on 2026/3/16.
//

#include "pwm.h"

#include "stm32f0xx_hal_tim.h"

extern TIM_HandleTypeDef htim1;
extern TIM_HandleTypeDef htim14;

void BSP_PWM_SetBrightnessAndCCT(const uint8_t bright, const uint8_t cct)
{
    const uint32_t pwm_period = 4799;
    const uint32_t warm_duty = (bright * cct / 100) * pwm_period / 100;
    const uint32_t cold_duty = (bright * (100 - cct) / 100) * pwm_period / 100;

    __HAL_TIM_SET_COMPARE(&htim14, TIM_CHANNEL_1, warm_duty); // PA7 W_PWM
    __HAL_TIM_SET_COMPARE(&htim1, TIM_CHANNEL_2, cold_duty);  // PA9 Y_PWM
}