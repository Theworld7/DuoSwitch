import type { Mode } from './types'

/** 分钟数 -> "HH:MM" */
export function minutesToClock(minutes: number): string {
  const safe = Number.isFinite(minutes) ? Math.max(0, Math.min(1439, Math.round(minutes))) : 0
  const hour = Math.floor(safe / 60)
  const minute = safe % 60
  return `${String(hour).padStart(2, '0')}:${String(minute).padStart(2, '0')}`
}

/** "HH:MM" -> 分钟数；非法输入返回 null */
export function clockToMinutes(clock: string): number | null {
  const matched = /^(\d{1,2}):(\d{2})$/.exec(clock.trim())
  if (matched === null) {
    return null
  }
  const hour = Number(matched[1])
  const minute = Number(matched[2])
  if (!Number.isInteger(hour) || !Number.isInteger(minute)) {
    return null
  }
  if (hour < 0 || hour > 23 || minute < 0 || minute > 59) {
    return null
  }
  return hour * 60 + minute
}

function startOfDay(date: Date): number {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime()
}

/** 把 ISO 时刻描述成「今天 19:00」这样。 */
export function describeMoment(iso: string | null): string {
  if (iso === null) {
    return '—'
  }
  const date = new Date(iso)
  if (Number.isNaN(date.getTime())) {
    return '—'
  }

  const now = new Date()
  const days = Math.round((startOfDay(date) - startOfDay(now)) / 86_400_000)
  const clock = `${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`

  if (days === 0) {
    return `今天 ${clock}`
  }
  if (days === 1) {
    return `明天 ${clock}`
  }
  if (days === -1) {
    return `昨天 ${clock}`
  }
  return `${date.getMonth() + 1}月${date.getDate()}日 ${clock}`
}

/** 从绝对路径里取出文件名，用于界面显示。 */
export function baseName(path: string | null): string {
  if (path === null) {
    return ''
  }
  const parts = path.split(/[\\/]/)
  return parts[parts.length - 1] ?? path
}

export const MODE_TITLE: Record<Mode, string> = {
  light: '浅色模式',
  dark: '深色模式',
}
