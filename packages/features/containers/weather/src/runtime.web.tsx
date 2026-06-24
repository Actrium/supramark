/**
 * Weather Web 渲染器
 *
 * 实现 ContainerWebRenderer 接口
 *
 * @packageDocumentation
 */

import React from 'react';
import type { ContainerWebRenderArgs } from '@supramark/core';
import type { WeatherData } from './feature.js';

/**
 * 天气图标（简单 SVG）
 */
function WeatherIcon({ type }: { type: 'sunny' | 'cloudy' | 'rainy' }) {
  const icons = {
    sunny: (
      <svg width="48" height="48" viewBox="0 0 48 48" fill="none">
        <circle cx="24" cy="24" r="10" fill="#FFB300" />
        <g stroke="#FFB300" strokeWidth="2" strokeLinecap="round">
          <line x1="24" y1="2" x2="24" y2="8" />
          <line x1="24" y1="40" x2="24" y2="46" />
          <line x1="2" y1="24" x2="8" y2="24" />
          <line x1="40" y1="24" x2="46" y2="24" />
          <line x1="8.5" y1="8.5" x2="12.7" y2="12.7" />
          <line x1="35.3" y1="35.3" x2="39.5" y2="39.5" />
          <line x1="8.5" y1="39.5" x2="12.7" y2="35.3" />
          <line x1="35.3" y1="12.7" x2="39.5" y2="8.5" />
        </g>
      </svg>
    ),
    cloudy: (
      <svg width="48" height="48" viewBox="0 0 48 48" fill="none">
        <ellipse cx="20" cy="28" rx="12" ry="8" fill="#90A4AE" />
        <ellipse cx="30" cy="26" rx="10" ry="7" fill="#B0BEC5" />
        <ellipse cx="25" cy="24" rx="8" ry="6" fill="#CFD8DC" />
      </svg>
    ),
    rainy: (
      <svg width="48" height="48" viewBox="0 0 48 48" fill="none">
        <ellipse cx="24" cy="18" rx="14" ry="8" fill="#78909C" />
        <g stroke="#42A5F5" strokeWidth="2" strokeLinecap="round">
          <line x1="16" y1="30" x2="14" y2="38" />
          <line x1="24" y1="30" x2="22" y2="38" />
          <line x1="32" y1="30" x2="30" y2="38" />
        </g>
      </svg>
    ),
  };
  return icons[type] || icons.sunny;
}

/**
 * 模拟天气数据（实际应用中应该调用天气 API）
 */
function getMockWeather(location: string, units: 'metric' | 'imperial' = 'metric') {
  // 根据城市名生成伪随机温度
  const hash = location.split('').reduce((acc, c) => acc + c.charCodeAt(0), 0);
  const baseTemp = 15 + (hash % 20);
  const temp = units === 'imperial' ? Math.round(baseTemp * 1.8 + 32) : baseTemp;
  const unit = units === 'imperial' ? '°F' : '°C';

  const conditions = ['sunny', 'cloudy', 'rainy'] as const;
  const condition = conditions[hash % 3];

  const humidity = 40 + (hash % 40);
  const wind = 5 + (hash % 20);
  const windUnit = units === 'imperial' ? 'mph' : 'km/h';

  return { temp, unit, condition, humidity, wind, windUnit };
}

const styles: Record<string, React.CSSProperties> = {
  container: {
    border: '1px solid #e0e0e0',
    borderRadius: '12px',
    padding: '16px',
    margin: '12px 0',
    background: 'linear-gradient(135deg, #667eea 0%, #764ba2 100%)',
    color: 'white',
    fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
    maxWidth: '320px',
  },
  header: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: '12px',
  },
  location: {
    fontSize: '18px',
    fontWeight: 600,
    margin: 0,
  },
  format: {
    fontSize: '10px',
    background: 'rgba(255,255,255,0.2)',
    padding: '2px 6px',
    borderRadius: '4px',
  },
  main: {
    display: 'flex',
    alignItems: 'center',
    gap: '16px',
  },
  temp: {
    fontSize: '48px',
    fontWeight: 300,
    margin: 0,
  },
  details: {
    display: 'flex',
    gap: '16px',
    marginTop: '12px',
    fontSize: '14px',
    opacity: 0.9,
  },
  error: {
    background: '#ffebee',
    color: '#c62828',
    border: '1px solid #ef9a9a',
    borderRadius: '8px',
    padding: '12px',
    margin: '12px 0',
  },
  errorTitle: {
    fontWeight: 600,
    marginBottom: '8px',
  },
  errorCode: {
    fontFamily: 'monospace',
    fontSize: '12px',
    background: '#fff',
    padding: '8px',
    borderRadius: '4px',
    whiteSpace: 'pre-wrap' as const,
  },
};

/**
 * Web 渲染器 for :::weather
 */
export function renderWeatherContainerWeb({
  node,
  key,
}: ContainerWebRenderArgs): React.ReactNode {
  const data = (node?.data ?? {}) as unknown as WeatherData;
  const { format, location, units = 'metric', parseError, rawConfig } = data;

  // 解析错误时显示错误信息
  if (parseError) {
    return (
      <div key={key} style={styles.error}>
        <div style={styles.errorTitle}>⚠️ Weather 配置错误</div>
        <div>{parseError}</div>
        {rawConfig && (
          <pre style={styles.errorCode}>{rawConfig}</pre>
        )}
      </div>
    );
  }

  // 缺少必要配置
  if (!location) {
    return (
      <div key={key} style={styles.error}>
        <div style={styles.errorTitle}>⚠️ 缺少 location 配置</div>
        <div>请在配置中指定 location 字段</div>
      </div>
    );
  }

  // 获取模拟天气数据
  const weather = getMockWeather(location, units);

  return (
    <div key={key} style={styles.container}>
      <div style={styles.header}>
        <h4 style={styles.location}>{location}</h4>
        <span style={styles.format}>{format.toUpperCase()}</span>
      </div>
      <div style={styles.main}>
        <WeatherIcon type={weather.condition} />
        <p style={styles.temp}>
          {weather.temp}
          <span style={{ fontSize: '24px' }}>{weather.unit}</span>
        </p>
      </div>
      <div style={styles.details}>
        <span>💧 {weather.humidity}%</span>
        <span>💨 {weather.wind} {weather.windUnit}</span>
      </div>
    </div>
  );
}
