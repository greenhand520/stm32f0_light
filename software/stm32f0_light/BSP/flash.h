//
// Created by eartholnpc on 2026/3/16.
//

#pragma once

#include <stdint.h>
#include <string.h>

#include "stm32f0xx_hal_def.h"

// STM32F030F4P6：16KB Flash，最后一页从 0x08003C00 开始
#define FLASH_SAVE_BASE_ADDR    ((uint32_t)0x08003C00U)
// #define FLASH_PAGE_SIZE         1024U   // F030 页大小为 1KB

typedef struct {
    uint32_t magic;      // 0xAA55AA55
    // last brightness
    uint8_t  brightness;
    // last color
    uint8_t  cct;
    // brightness after pyroelectric infrared sensor trig
    uint8_t  dark_brightness;
    // color after pyroelectric infrared sensor trig
    uint8_t  dark_cct;
    uint8_t  checksum;
} SavedConfig;

SavedConfig saved_config;

void BSP_Flash_Save(uint8_t bright, uint8_t cct);
void BSP_Flash_SaveDark(uint8_t bright, uint8_t cct);

void BSP_Flash_Load(uint8_t* bright, uint8_t* cct);
void BSP_Flash_LoadDark(uint8_t* bright, uint8_t* cct);