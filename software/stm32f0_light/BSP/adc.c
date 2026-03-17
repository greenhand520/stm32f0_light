//
// Created by eartholnpc on 2026/3/13.
//

#include "adc.h"

#include "stm32f0xx_hal_adc.h"

extern ADC_HandleTypeDef hadc;


uint32_t BSP_ADC_ReadADC(uint32_t channel, uint8_t samples)
{
    uint32_t sum = 0;
    ADC_ChannelConfTypeDef sConfig = {0};
    // 配置通道
    sConfig.Channel = channel;
    sConfig.Rank = 1;
    sConfig.SamplingTime = ADC_SAMPLETIME_239CYCLES_5;
    HAL_ADC_ConfigChannel(&hadc, &sConfig);

    // 多次采样取平均
    for(int i = 0; i < samples; i++) {
        HAL_ADC_Start(&hadc);
        HAL_ADC_PollForConversion(&hadc, 10);
        sum += HAL_ADC_GetValue(&hadc);
        HAL_Delay(1);
    }

    return sum / samples;
}

uint8_t BSP_ADC_IsDark(void)
{
    const uint32_t adc_avg = BSP_ADC_ReadADC(ADC_CHANNEL_2, 8);
    if (adc_avg > LIGHT_THRESHOLD) {
        return 1;
    }
    return 0;
}

uint32_t BSP_ADC_GetLEDTempResistance(void)
{
    const uint32_t adc_avg = BSP_ADC_ReadADC(ADC_CHANNEL_3, 8);

    // Prevent dividing by zero
    if (adc_avg >= 4095) {
        // ADC full scale with extremely high resistance (very low NTC temperature)
        return 999999;
    }
    if (adc_avg == 0) {
        // ADC is 0 and resistance is very low (NTC temperature is extremely high)
        return 0;
    }
    // 电路为固定电阻R_fixed（100K）接VCC，NTC接地。ADC测量的是NTC两端的电压。
    // R_NTC = (ADC × R_fixed) / (4095 - ADC)
    // 100K B3950 60℃ 约25R, 70℃时，约18R 75℃约15R
    return adc_avg * 100000 / (4095 - adc_avg);
}

float BSP_ADC_GetVoltage(void)
{
    // 假设分压电阻 R1=100k, R2=47k
    // 分压系数 = (R1 + R2) / R2 = (100+47)/47 ≈ 3.13
    const uint32_t adc_avg = BSP_ADC_ReadADC(ADC_CHANNEL_1, 8);
    const float pin_voltage = (float)adc_avg * 3.3f / 4095;
    return pin_voltage * 3.13f;
}