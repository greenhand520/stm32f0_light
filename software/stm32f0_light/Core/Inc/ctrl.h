//
// Created by eartholnpc on 2026/3/6.
//

#pragma once

#include "main.h"

#define LOWEST_BRIGHTNESS 5

// Function prototypes
void Ctrl_Init(void);
void Ctrl_Process(void);
uint8_t Ctrl_GetBrightness(void);
uint8_t Ctrl_GetColorTemp(void);
void Ctrl_SetBrightness(uint8_t val);
void Ctrl_SetColorTemp(uint8_t val);