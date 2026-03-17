//
// Created by eartholnpc on 2026/3/6.
//

#pragma once

#include "main.h"

// Function prototypes
void Detect_Init(void);
void Detect_Process(void);
uint16_t Detect_GetBatteryVoltage(void);
uint16_t Detect_GetLedTemperatureResistance(void);
uint8_t Detect_IsNight();