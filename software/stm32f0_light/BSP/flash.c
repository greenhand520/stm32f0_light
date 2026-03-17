//
// Created by eartholnpc on 2026/3/16.
//

#include "flash.h"

// 内部使用：计算校验和
static uint8_t CalcChecksum(const SavedConfig *p)
{
    uint8_t sum = 0;
    const uint8_t *ptr = (const uint8_t *)p;
    for (int i = 0; i < (int)sizeof(SavedConfig) - 1; i++) {
        sum ^= ptr[i];
    }
    return sum;
}

// 擦除最后 1 页 Flash
static HAL_StatusTypeDef Flash_Erase_LastPage(void)
{
    FLASH_EraseInitTypeDef EraseInitStruct = {0};
    uint32_t PageError = 0;

    EraseInitStruct.TypeErase   = FLASH_TYPEERASE_PAGES;
    EraseInitStruct.PageAddress = FLASH_SAVE_BASE_ADDR;
    EraseInitStruct.NbPages     = 1;

    HAL_FLASH_Unlock();
    const HAL_StatusTypeDef status = HAL_FLASHEx_Erase(&EraseInitStruct, &PageError);
    HAL_FLASH_Lock();

    return status;
}

// 写入结构体到 Flash
static HAL_StatusTypeDef Flash_Write_Param(const SavedConfig *param)
{
    HAL_StatusTypeDef status = HAL_OK;
    uint32_t addr = FLASH_SAVE_BASE_ADDR;

    HAL_FLASH_Unlock();

    // 逐字（32位）写入
    status = HAL_FLASH_Program(FLASH_TYPEPROGRAM_WORD, addr, param->magic);
    addr += 4;
    if (status != HAL_OK) goto exit;

    status = HAL_FLASH_Program(FLASH_TYPEPROGRAM_WORD, addr,
                               ((uint32_t)param->brightness) |
                               ((uint32_t)param->cct << 8) |
                               ((uint32_t)param->dark_brightness << 16) |
                               ((uint32_t)param->dark_cct << 24));
    addr += 4;
    if (status != HAL_OK) goto exit;

    status = HAL_FLASH_Program(FLASH_TYPEPROGRAM_WORD, addr, param->checksum);
    // addr += 4;  // 可不写，因为后面没有了

    exit:
        HAL_FLASH_Lock();
    return status;
}

// 读取结构体
static void Flash_Read_Param(SavedConfig *param)
{
    memcpy(param, (void *)FLASH_SAVE_BASE_ADDR, sizeof(SavedConfig));
}

void BSP_Flash_Save(const uint8_t bright, const uint8_t cct)
{
    SavedConfig param;

    // 先读取原有数据（保留 dark_* 字段）
    Flash_Read_Param(&param);

    // 只更新手动调节/关闭时的值
    param.magic     = 0xAA55AA55U;
    param.brightness = bright;
    param.cct       = cct;
    // dark_brightness 和 dark_cct 保持不变

    param.checksum  = CalcChecksum(&param);

    // 擦除 → 写入
    if (Flash_Erase_LastPage() == HAL_OK) {
        Flash_Write_Param(&param);
    }
}

void BSP_Flash_SaveDark(const uint8_t bright, const uint8_t cct)
{
    SavedConfig param;

    // 先读取原有数据（保留 brightness 和 cct）
    Flash_Read_Param(&param);

    // 只更新暗环境唤醒默认值
    param.magic     = 0xAA55AA55U;
    param.dark_brightness = bright;
    param.dark_cct  = cct;
    // brightness 和 cct 保持不变

    param.checksum  = CalcChecksum(&param);

    // 擦除 → 写入
    if (Flash_Erase_LastPage() == HAL_OK) {
        Flash_Write_Param(&param);
    }
}

void BSP_Flash_Load(uint8_t *bright, uint8_t *cct)
{
    SavedConfig param;
    Flash_Read_Param(&param);

    // 校验魔术字和校验和
    if (param.magic == 0xAA55AA55U &&
        param.checksum == CalcChecksum(&param))
    {
        *bright = param.brightness;
        *cct    = param.cct;
    }
    else
    {
        // 默认值（第一次使用或数据损坏）
        *bright = 5;   // 最低亮度 或 你想要的默认值
        *cct    = 50;  // 中等色温
    }
}

void BSP_Flash_LoadDark(uint8_t *bright, uint8_t *cct)
{
    SavedConfig param;
    Flash_Read_Param(&param);

    if (param.magic == 0xAA55AA55U &&
        param.checksum == CalcChecksum(&param))
    {
        *bright = param.dark_brightness;
        *cct    = param.dark_cct;
    }
    else
    {
        // 默认值（暗环境唤醒默认值）
        *bright = 10;   // 建议比手动最低亮度稍高一些
        *cct    = 45;   // 稍偏暖或你喜欢的默认色温
    }
}