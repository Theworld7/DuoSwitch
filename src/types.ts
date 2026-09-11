/**
 * 与 Rust 侧结构一一对应的类型。
 *
 * Rust 用 serde 默认命名（snake_case），且没有 rename_all，
 * 因此这里的字段名保持 snake_case，避免两边维护两套映射。
 */

export type Mode = 'light' | 'dark'

export type DayFilter = 'daily' | 'weekdays' | 'weekend'

export interface TimeRule {
  mode: Mode
  /** 当日 00:00 起的分钟数 */
  minutes: number
  days: DayFilter
}

/** Rust 侧是 `#[serde(tag = "kind")]` 的枚举 */
export type Schedule =
  | { kind: 'fixed'; rules: TimeRule[] }
  | { kind: 'sun'; latitude: number; longitude: number }

export interface AppConfig {
  enabled: boolean
  light_wallpaper: string | null
  dark_wallpaper: string | null
  schedule: Schedule
  lock_screen: boolean
}

/** Rust 的 DateTime<Local> 序列化为带偏移的 RFC3339 字符串 */
export interface Transition {
  at: string
  mode: Mode
}

export interface Conflict {
  expected: Mode
  actual: Mode
  at: string
}

export interface AppState {
  config: AppConfig
  revision: number
  /** 系统当前实际主题 */
  theme: Mode
  /** 调度最后一次主动断言的模式 */
  applied_mode: Mode | null
  next: Transition | null
  conflict: Conflict | null
  last_error: string | null
  /** 保存配置时被自动修正的说明 */
  notice: string | null
  paused: boolean
  last_tick: string | null
  elevated: boolean
}

export interface SunPreview {
  date: string
  /** HH:MM 本地时间 */
  sunrise: string | null
  sunset: string | null
  polar: boolean
}

export const MODE_LABEL: Record<Mode, string> = {
  light: '浅色',
  dark: '深色',
}

export const DAY_LABEL: Record<DayFilter, string> = {
  daily: '每天',
  weekdays: '工作日',
  weekend: '周末',
}
