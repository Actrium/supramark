/**
 * Weather React Native 渲染器
 *
 * 实现 ContainerRNRenderer 接口
 *
 * @packageDocumentation
 */

import React from 'react';
import { View, Text, StyleSheet } from 'react-native';
import type { ContainerRNRenderArgs } from '@supramark/core';
import type { WeatherData } from './feature.js';

/**
 * 模拟天气数据（实际应用中应该调用天气 API）
 */
function getMockWeather(location: string, units: 'metric' | 'imperial' = 'metric') {
  const hash = location.split('').reduce((acc, c) => acc + c.charCodeAt(0), 0);
  const baseTemp = 15 + (hash % 20);
  const temp = units === 'imperial' ? Math.round(baseTemp * 1.8 + 32) : baseTemp;
  const unit = units === 'imperial' ? '°F' : '°C';

  const conditions = ['☀️', '☁️', '🌧️'] as const;
  const condition = conditions[hash % 3];

  const humidity = 40 + (hash % 40);
  const wind = 5 + (hash % 20);
  const windUnit = units === 'imperial' ? 'mph' : 'km/h';

  return { temp, unit, condition, humidity, wind, windUnit };
}

const localStyles = StyleSheet.create({
  container: {
    borderRadius: 12,
    padding: 16,
    marginVertical: 12,
    backgroundColor: '#667eea',
  },
  header: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 12,
  },
  location: {
    fontSize: 18,
    fontWeight: '600',
    color: 'white',
  },
  format: {
    fontSize: 10,
    color: 'white',
    backgroundColor: 'rgba(255,255,255,0.2)',
    paddingHorizontal: 6,
    paddingVertical: 2,
    borderRadius: 4,
    overflow: 'hidden',
  },
  main: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 16,
  },
  icon: {
    fontSize: 48,
  },
  temp: {
    fontSize: 48,
    fontWeight: '300',
    color: 'white',
  },
  unit: {
    fontSize: 24,
    color: 'white',
  },
  details: {
    flexDirection: 'row',
    gap: 16,
    marginTop: 12,
  },
  detail: {
    fontSize: 14,
    color: 'rgba(255,255,255,0.9)',
  },
  error: {
    backgroundColor: '#ffebee',
    borderRadius: 8,
    padding: 12,
    marginVertical: 12,
  },
  errorTitle: {
    fontSize: 14,
    fontWeight: '600',
    color: '#c62828',
    marginBottom: 8,
  },
  errorText: {
    fontSize: 12,
    color: '#c62828',
  },
  errorCode: {
    fontFamily: 'monospace',
    fontSize: 11,
    color: '#333',
    backgroundColor: '#fff',
    padding: 8,
    borderRadius: 4,
    marginTop: 8,
  },
});

/**
 * RN 渲染器 for :::weather
 */
export function renderWeatherContainerRN({
  node,
  key,
}: ContainerRNRenderArgs): React.ReactNode {
  const data = (node?.data ?? {}) as unknown as WeatherData;
  const { format, location, units = 'metric', parseError, rawConfig } = data;

  // 解析错误时显示错误信息
  if (parseError) {
    return (
      <View key={key} style={localStyles.error}>
        <Text style={localStyles.errorTitle}>⚠️ Weather 配置错误</Text>
        <Text style={localStyles.errorText}>{parseError}</Text>
        {rawConfig && (
          <Text style={localStyles.errorCode}>{rawConfig}</Text>
        )}
      </View>
    );
  }

  // 缺少必要配置
  if (!location) {
    return (
      <View key={key} style={localStyles.error}>
        <Text style={localStyles.errorTitle}>⚠️ 缺少 location 配置</Text>
        <Text style={localStyles.errorText}>请在配置中指定 location 字段</Text>
      </View>
    );
  }

  // 获取模拟天气数据
  const weather = getMockWeather(location, units);

  return (
    <View key={key} style={localStyles.container}>
      <View style={localStyles.header}>
        <Text style={localStyles.location}>{location}</Text>
        <Text style={localStyles.format}>{format.toUpperCase()}</Text>
      </View>
      <View style={localStyles.main}>
        <Text style={localStyles.icon}>{weather.condition}</Text>
        <Text style={localStyles.temp}>
          {weather.temp}
          <Text style={localStyles.unit}>{weather.unit}</Text>
        </Text>
      </View>
      <View style={localStyles.details}>
        <Text style={localStyles.detail}>💧 {weather.humidity}%</Text>
        <Text style={localStyles.detail}>💨 {weather.wind} {weather.windUnit}</Text>
      </View>
    </View>
  );
}
