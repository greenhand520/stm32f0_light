//
// Created by eartholnpc on 2026/3/16.
//

#pragma once
#include <stdint.h>

/// set brightness and color
/// @param bright bright level, 0 - 100
/// @param cct color temperature， 0 - 100
void BSP_PWM_SetBrightnessAndCCT(uint8_t bright, uint8_t cct);