//
// Created by eartholnpc on 2026/3/6.
//

#pragma once

#include "main.h"

// Power states
typedef enum {
    POWER_STATE_SLEEP,
    POWER_STATE_AWAKE,
    POWER_STATE_BRIGHTNESS_CONTROL,
    POWER_STATE_COLOR_TEMP_CONTROL
} PowerState;

// Function prototypes
void PowerManager_Init(void);
void PowerManager_Process(void);
PowerState PowerManager_GetState(void);
void PowerManager_SetState(PowerState state);
void PowerManager_SaveSettings(uint8_t brightness, uint8_t color_temp);
void PowerManager_LoadSettings(uint8_t* brightness, uint8_t* color_temp);