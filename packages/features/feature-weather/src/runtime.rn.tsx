/**
 * Weather React Native 渲染器
 *
 * 实现 ContainerRNRenderer 接口
 *
 * @packageDocumentation
 */

import React from 'react';
import { View, Text, StyleSheet, Pressable, ImageBackground, Image } from 'react-native';
import type { ContainerRNRenderArgs } from '@supramark/core';
import type { WeatherData } from './feature.js';

/**
 * 模拟天气数据（实际应用中应该调用天气 API）
 */
function getMockWeather(location: string, units: 'metric' | 'imperial' = 'metric') {
  const hash = location.split('').reduce((acc, c) => acc + c.charCodeAt(0), 0);
  const baseTemp = 12 + (hash % 15);
  const lowTempC = baseTemp - (2 + (hash % 3));
  const highTempC = baseTemp + (2 + ((hash >> 1) % 4));
  const lowTemp = units === 'imperial' ? Math.round(lowTempC * 1.8 + 32) : lowTempC;
  const highTemp = units === 'imperial' ? Math.round(highTempC * 1.8 + 32) : highTempC;
  const unit = units === 'imperial' ? '°F' : '°C';

  const conditions = [
    { icon: '☀', text: '晴', secondaryText: '阳光充足' },
    { icon: '☁', text: '多云', secondaryText: '云量较多' },
    { icon: '☂', text: '阵雨', secondaryText: '短时有雨' },
    { icon: '☈', text: '雷阵雨并伴有冰雹', secondaryText: '对流天气明显' },
  ] as const;
  const condition = conditions[hash % conditions.length];

  const wind = 5 + (hash % 20);
  const weekdays = ['周日', '周一', '周二', '周三', '周四', '周五', '周六'] as const;
  const month = (hash % 12) + 1;
  const day = (hash % 28) + 1;

  // 按风速区间生成更接近卡片文案的风力等级。
  const windLevel = wind < 10 ? '<3级' : wind < 18 ? '3-4级' : '4-5级';

  return {
    lowTemp,
    highTemp,
    unit,
    conditionIcon: condition.icon,
    conditionText: condition.text,
    conditionSecondaryText: condition.secondaryText,
    windLevel,
    weekday: weekdays[hash % weekdays.length],
    dateLabel: `${month}/${day}`,
  };
}

const localStyles = StyleSheet.create({
  container: {
    borderRadius: 14,
    overflow: 'hidden',
    marginBottom: 12,
    backgroundColor: '#f5f7ff',
    minWidth: '70%',
  },
  content: {
    paddingTop: 32,
    paddingHorizontal: 24,
    paddingBottom: 28,
    alignItems: 'center',
  },
  contentDark: {
    backgroundColor: 'rgba(13, 17, 23, 0.72)',
  },
  weekday: {
    fontSize: 14,
    fontWeight: '600',
    color: '#252525',
  },
  date: {
    marginTop: 2,
    fontSize: 14,
    fontWeight: '600',
    color: '#252525',
  },
  summary: {
    marginTop: 14,
    fontSize: 12,
    color: '#565656',
    textAlign: 'center',
  },
  primaryIcon: {
    marginTop: 18,
    fontSize: 16,
    color: '#252525',
  },
  range: {
    marginTop: 28,
    fontSize: 18,
    fontWeight: '600',
    color: '#2F54EB',
  },
  secondaryIcon: {
    marginTop: 18,
    fontSize: 16,
    color: '#252525',
  },
  secondarySummary: {
    marginTop: 14,
    fontSize: 12,
    color: '#565656',
    textAlign: 'center',
  },
  windLevel: {
    marginTop: 14,
    fontSize: 12,
    color: '#565656',
    textAlign: 'center',
  },
  textDark: {
    color: '#f0f6fc',
  },
  mutedTextDark: {
    color: '#c9d1d9',
  },
  accentTextDark: {
    color: '#79c0ff',
  },
  footer: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    paddingHorizontal: 8,
    paddingVertical: 14,
    columnGap: 2,
    backgroundColor: '#E9F0FC',
  },
  footerDark: {
    backgroundColor: '#161b22',
  },
  locationRow: {
    flexDirection: 'row',
    alignItems: 'center',
    flex: 1,
    minWidth: 0,
    gap: 6,
  },
  locationIcon: {
    width: 13.2,
    height: 16.2,
  },
  location: {
    flex: 1,
    fontSize: 16,
    fontWeight: '600',
    color: '#252525',
  },
  unitSwitch: {
    flexDirection: 'row',
    alignItems: 'center',
    borderRadius: 12,
    backgroundColor: '#b8cef8',
    width: 78,
    height: 24,
  },
  unitSwitchDark: {
    backgroundColor: '#30363d',
  },
  unitOption: {
    width: 39,
    alignItems: 'center',
  },
  unitOptionActive: {
    width: 39,
    borderRadius: 12,
    backgroundColor: '#5b7bf5',
  },
  unitOptionActiveDark: {
    backgroundColor: '#1f6feb',
  },
  unitOptionText: {
    fontSize: 14,
    fontWeight: '500',
    lineHeight: 24,
    color: '#252525',
  },
  unitOptionTextActive: {
    color: '#ffffff',
  },
  error: {
    backgroundColor: '#ffebee',
    borderRadius: 8,
    padding: 12,
    marginVertical: 12,
  },
  errorDark: {
    backgroundColor: '#2d1517',
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
  errorTextDark: {
    color: '#ffb4ad',
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
  errorCodeDark: {
    color: '#ffd8d3',
    backgroundColor: '#161b22',
  },
});

const unitOptions = [
  { key: 'metric', label: '°C' },
  { key: 'imperial', label: '°F' },
] as const;
const weatherBackground = require('../icons/weather-bg.png');
const locationIcon = require('../icons/location.png');

type UnitType = 'metric' | 'imperial';

type WeatherCardProps = {
  location: string;
  initialUnits: UnitType;
  theme: 'light' | 'dark';
};

function WeatherCard({ location, initialUnits, theme }: WeatherCardProps) {
  // 组件内部维护当前单位，点击开关时只切换展示状态。
  const [currentUnits, setCurrentUnits] = React.useState<UnitType>(initialUnits);

  // 当前主题只影响天气卡片自身色板，不改变天气数据和单位切换状态。
  const isDark = theme === 'dark';

  const weather = getMockWeather(location, currentUnits);

  return (
    <View style={localStyles.container}>
      <ImageBackground
        source={weatherBackground}
        resizeMode="cover"
        style={[localStyles.content, isDark && localStyles.contentDark]}
        imageStyle={isDark ? { opacity: 0.35 } : undefined}
      >
        <Text style={[localStyles.weekday, isDark && localStyles.textDark]}>{weather.weekday}</Text>
        <Text style={[localStyles.date, isDark && localStyles.textDark]}>{weather.dateLabel}</Text>
        <Text style={[localStyles.summary, isDark && localStyles.mutedTextDark]}>
          {weather.conditionText}
        </Text>
        <Text style={[localStyles.primaryIcon, isDark && localStyles.textDark]}>
          {weather.conditionIcon}
        </Text>
        <Text style={[localStyles.range, isDark && localStyles.accentTextDark]}>
          {weather.lowTemp}
          {weather.unit}/{weather.highTemp}
          {weather.unit}
        </Text>
        <Text style={[localStyles.secondaryIcon, isDark && localStyles.textDark]}>
          {weather.conditionIcon}
        </Text>
        <Text style={[localStyles.secondarySummary, isDark && localStyles.mutedTextDark]}>
          {weather.conditionSecondaryText}
        </Text>
        <Text style={[localStyles.windLevel, isDark && localStyles.mutedTextDark]}>
          {weather.windLevel}
        </Text>
      </ImageBackground>
      <View style={[localStyles.footer, isDark && localStyles.footerDark]}>
        <View style={localStyles.locationRow}>
          <Image source={locationIcon} style={localStyles.locationIcon} />
          <Text
            style={[localStyles.location, isDark && localStyles.textDark]}
            numberOfLines={1}
            ellipsizeMode="tail"
          >
            {location}
          </Text>
        </View>
        <View style={[localStyles.unitSwitch, isDark && localStyles.unitSwitchDark]}>
          {unitOptions.map(option => {
            const selected = currentUnits === option.key;

            return (
              <Pressable
                key={option.key}
                onPress={() => setCurrentUnits(option.key)}
                style={[
                  localStyles.unitOption,
                  selected && localStyles.unitOptionActive,
                  selected && isDark && localStyles.unitOptionActiveDark,
                ]}
              >
                <Text
                  style={[
                    localStyles.unitOptionText,
                    isDark && localStyles.textDark,
                    selected && localStyles.unitOptionTextActive,
                  ]}
                >
                  {option.label}
                </Text>
              </Pressable>
            );
          })}
        </View>
      </View>
    </View>
  );
}

/**
 * RN 渲染器 for :::weather
 */
export function renderWeatherContainerRN({
  node,
  key,
  theme = 'light',
}: ContainerRNRenderArgs): React.ReactNode {
  const data = (node?.data ?? {}) as WeatherData;
  const { location, units = 'metric', parseError, rawConfig } = data;
  // 错误态与正常卡片使用同一个主题来源，避免浅色错误块插进深色内容。
  const isDark = theme === 'dark';

  // 解析错误时显示错误信息
  if (parseError) {
    return (
      <View key={key} style={[localStyles.error, isDark && localStyles.errorDark]}>
        <Text style={[localStyles.errorTitle, isDark && localStyles.errorTextDark]}>
          ⚠️ Weather 配置错误
        </Text>
        <Text style={[localStyles.errorText, isDark && localStyles.errorTextDark]}>
          {parseError}
        </Text>
        {rawConfig && (
          <Text style={[localStyles.errorCode, isDark && localStyles.errorCodeDark]}>
            {rawConfig}
          </Text>
        )}
      </View>
    );
  }

  // 缺少必要配置
  if (!location) {
    return (
      <View key={key} style={[localStyles.error, isDark && localStyles.errorDark]}>
        <Text style={[localStyles.errorTitle, isDark && localStyles.errorTextDark]}>
          ⚠️ 缺少 location 配置
        </Text>
        <Text style={[localStyles.errorText, isDark && localStyles.errorTextDark]}>
          请在配置中指定 location 字段
        </Text>
      </View>
    );
  }

  return <WeatherCard key={key} location={location} initialUnits={units} theme={theme} />;
}
