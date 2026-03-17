//
// Created by eartholnpc on 2026/3/13.
//

#pragma once

#include <stdint.h>

// ADC value of 5516 photoresistor at night
#define LIGHT_THRESHOLD 4000
#define TEMP_THRESHOLD_MIN_RESISTANCE 15
#define TEMP_THRESHOLD_MAX_RESISTANCE 25

void BSP_ADC_Init(void);

// 读取指定通道的ADC值（带均值滤波）
uint32_t BSP_ADC_ReadADC(uint32_t channel, uint8_t samples);

/// 是否时黑暗的环境
uint8_t BSP_ADC_IsDark(void);

/// 获取led灯板上热敏电阻的阻值
uint32_t BSP_ADC_GetLEDTempResistance(void);

/// 获取电池实际电压(V)
float BSP_ADC_GetVoltage(void);