//
// Created by eartholnpc on 2026/3/16.
//

#include "rgb.h"

#include "main.h"
#include "stm32f0xx_hal_gpio.h"

void BSP_RGB_Set(const uint8_t r, const uint8_t g, const uint8_t b)
{
    HAL_GPIO_WritePin(STATE_R_GPIO_Port, STATE_R_Pin, r ? GPIO_PIN_RESET : GPIO_PIN_SET);
    HAL_GPIO_WritePin(STATE_G_GPIO_Port, STATE_G_Pin, g ? GPIO_PIN_RESET : GPIO_PIN_SET);
    HAL_GPIO_WritePin(STATE_B_GPIO_Port, STATE_B_Pin, b ? GPIO_PIN_RESET : GPIO_PIN_SET);
}

void BSP_RGB_SetRed(void)
{
    BSP_RGB_Set(1, 0, 0);
}

void BSP_RGB_SetGreen(void)
{
    BSP_RGB_Set(0, 1, 0);
}

void BSP_RGB_SetBlue(void)
{
    BSP_RGB_Set(0, 0, 1);
}

void BSP_RGB_SetWhite(void)
{
    BSP_RGB_Set(1, 1, 1);
}

void BSP_RGB_SetYellow(void)
{
    BSP_RGB_Set(1, 1, 0);
}

void BSP_RGB_SetPurple(void)
{
    BSP_RGB_Set(1, 0, 1);
}

void BSP_RGB_SetOff(void)
{
    BSP_RGB_Set(0, 0, 0);
}