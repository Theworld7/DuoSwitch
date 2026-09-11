/**
 * 单窗口应用的状态中枢。用模块级单例而不是 Pinia：
 * 只有一个窗口、一份状态，引入 store 库不划算。
 */

import { computed, reactive, ref, toRaw, watch } from 'vue'
import { listen } from '@tauri-apps/api/event'

import * as api from './api'
import type { AppConfig, AppState, DayFilter, Mode, Schedule, SunPreview, TimeRule } from './types'

const DEFAULT_CONFIG: AppConfig = {
  enabled: true,
  light_wallpaper: null,
  dark_wallpaper: null,
  schedule: {
    kind: 'fixed',
    rules: [
      { mode: 'light', minutes: 7 * 60, days: 'daily' },
      { mode: 'dark', minutes: 19 * 60, days: 'daily' },
    ],
  },
  lock_screen: false,
}

export const state = ref<AppState | null>(null)
export const draft = reactive<AppConfig>(structuredClone(DEFAULT_CONFIG))
export const busy = ref(false)
export const message = ref<string | null>(null)
export const autostartEnabled = ref(false)
export const sunPreview = ref<SunPreview | null>(null)
/** 壁纸缩略图（data URL），由 Rust 侧解码缩放后返回，避免给 webview 开放文件读取 */
export const previews = reactive<{ light: string | null; dark: string | null }>({
  light: null,
  dark: null,
})
/** 锁屏助手任务是否已注册；null 表示还没查过 */
export const lockHelperReady = ref<boolean | null>(null)
/** 首次启动时后端把内置壁纸路径写进了配置，用来提示用户点「立即应用」 */
export const firstRunSeeded = ref(false)

const MODES: Mode[] = ['light', 'dark']

/** 已与后端同步过的配置版本，用于避免事件覆盖用户正在编辑的内容 */
let syncedRevision = -1

export const rules = computed<TimeRule[]>(() =>
  draft.schedule.kind === 'fixed' ? draft.schedule.rules : [],
)

export const conflict = computed(() => state.value?.conflict ?? null)
export const isPaused = computed(() => state.value?.paused ?? false)
export const online = computed(() => state.value !== null)

function describeError(error: unknown): string {
  if (typeof error === 'string') {
    return error
  }
  if (error instanceof Error) {
    return error.message
  }
  return String(error)
}

function applyState(next: AppState): void {
  state.value = next
  document.documentElement.classList.toggle('dark', next.theme === 'dark')

  if (next.revision !== syncedRevision) {
    syncedRevision = next.revision
    Object.assign(draft, structuredClone(next.config))
  }
}

/** 提交当前草稿到后端。所有控件改完都走这里。 */
export async function commit(): Promise<void> {
  busy.value = true
  try {
    const next = await api.saveConfig(structuredClone(toRaw(draft)))
    syncedRevision = next.revision
    applyState(next)
    message.value = next.notice
  } catch (error) {
    message.value = describeError(error)
  } finally {
    busy.value = false
  }
}

export async function bootstrap(): Promise<void> {
  try {
    await listen<AppState>('state', (event) => {
      applyState(event.payload)
    })
    await listen('first-run-seeded', () => {
      firstRunSeeded.value = true
    })
    applyState(await api.getState())
    autostartEnabled.value = await api.getAutostart()
    await refreshLockHelper()
    await refreshPreviews()
  } catch (error) {
    message.value = describeError(error)
  }
}

async function loadPreview(mode: Mode): Promise<void> {
  const path = mode === 'light' ? draft.light_wallpaper : draft.dark_wallpaper
  if (path === null) {
    previews[mode] = null
    return
  }
  try {
    previews[mode] = await api.wallpaperPreview(path)
  } catch {
    previews[mode] = null
  }
}

async function refreshPreviews(): Promise<void> {
  await Promise.all(MODES.map(async (mode) => loadPreview(mode)))
}

async function refreshLockHelper(): Promise<void> {
  try {
    lockHelperReady.value = await api.lockHelperReady()
  } catch {
    lockHelperReady.value = null
  }
}

/**
 * 提权实例注册完任务才会被查出来，而用户可能在 UAC 弹窗上停留一会儿，
 * 所以这里轮询等待，最多 20 秒。
 */
async function waitForLockHelper(): Promise<void> {
  for (let attempt = 0; attempt < 20; attempt += 1) {
    await new Promise((resolve) => setTimeout(resolve, 1000))
    await refreshLockHelper()
    if (lockHelperReady.value === true) {
      return
    }
  }
}

export async function dismissConflict(): Promise<void> {
  try {
    applyState(await api.dismissConflict())
  } catch (error) {
    message.value = describeError(error)
  }
}

export function setSchedule(next: Schedule): void {
  draft.schedule = next
  void commit()
}

export function setScheduleKind(kind: Schedule['kind']): void {
  if (draft.schedule.kind === kind) {
    return
  }
  if (kind === 'sun') {
    setSchedule({ kind: 'sun', latitude: 39.9042, longitude: 116.4074 })
    return
  }
  setSchedule({ kind: 'fixed', rules: structuredClone(DEFAULT_CONFIG.schedule.kind === 'fixed' ? DEFAULT_CONFIG.schedule.rules : []) })
}

export function updateRule(index: number, patch: Partial<TimeRule>): void {
  if (draft.schedule.kind !== 'fixed') {
    return
  }
  const rule = draft.schedule.rules[index]
  if (rule === undefined) {
    return
  }
  Object.assign(rule, patch)
  void commit()
}

export function appendRule(): void {
  if (draft.schedule.kind !== 'fixed') {
    return
  }
  draft.schedule.rules.push({ mode: 'light', minutes: 12 * 60, days: 'weekdays' })
  void commit()
}

export function removeRule(index: number): void {
  if (draft.schedule.kind !== 'fixed') {
    return
  }
  draft.schedule.rules.splice(index, 1)
  void commit()
}

/** 改壁纸后立刻重新应用，否则当前桌面还是旧图。 */
export async function setWallpaper(mode: Mode, path: string | null): Promise<void> {
  if (mode === 'light') {
    draft.light_wallpaper = path
  } else {
    draft.dark_wallpaper = path
  }
  await commit()
  await applyNow()
}

export async function chooseWallpaper(mode: Mode): Promise<void> {
  try {
    const path = await api.pickWallpaperImage()
    if (path === null) {
      return
    }
    await setWallpaper(mode, path)
  } catch (error) {
    message.value = describeError(error)
  }
}

export function setEnabled(enabled: boolean): void {
  draft.enabled = enabled
  void commit()
}

/** 打开锁屏同步时顺手把助手任务注册掉，否则每次写锁屏都会因权限不足失败。 */
export async function setLockScreen(enabled: boolean): Promise<void> {
  draft.lock_screen = enabled
  await commit()

  if (!enabled) {
    return
  }

  await refreshLockHelper()
  if (lockHelperReady.value !== true) {
    await setupLockHelper()
    return
  }
  await applyNow()
}

/** 请求管理员权限注册助手任务，成功后立即写一次锁屏。 */
export async function setupLockHelper(): Promise<void> {
  busy.value = true
  try {
    await api.setupLockHelper()
  } catch (error) {
    message.value = describeError(error)
    return
  } finally {
    busy.value = false
  }

  message.value = '已请求管理员权限，请在弹窗中允许授权'
  await waitForLockHelper()

  if (lockHelperReady.value === true) {
    message.value = '锁屏助手任务已注册，之后切换锁屏不再需要授权'
    await applyNow()
  } else {
    message.value = '未检测到锁屏助手任务；授权未通过时锁屏壁纸不会跟随'
  }
}

export function setDays(index: number, days: DayFilter): void {
  updateRule(index, { days })
}

export function setRuleMode(index: number, mode: Mode): void {
  updateRule(index, { mode })
}

export async function applyNow(): Promise<void> {
  busy.value = true
  try {
    applyState(await api.applyNow())
    firstRunSeeded.value = false
  } catch (error) {
    message.value = describeError(error)
  } finally {
    busy.value = false
  }
}

export async function togglePause(): Promise<void> {
  try {
    applyState(await api.setPaused(!isPaused.value))
  } catch (error) {
    message.value = describeError(error)
  }
}

export async function setAutostart(on: boolean): Promise<void> {
  try {
    await api.setAutostart(on)
    autostartEnabled.value = await api.getAutostart()
  } catch (error) {
    message.value = describeError(error)
  }
}

export async function clearLockScreen(): Promise<void> {
  busy.value = true
  try {
    applyState(await api.clearLockScreen())
    message.value = '已撤销锁屏接管，锁屏壁纸交还给系统设置'
  } catch (error) {
    message.value = describeError(error)
  } finally {
    busy.value = false
  }
}

export async function openColorSettings(): Promise<void> {
  try {
    await api.openColorSettings()
  } catch (error) {
    message.value = describeError(error)
  }
}

export function dismissMessage(): void {
  message.value = null
}

async function refreshSunPreview(): Promise<void> {
  const schedule = draft.schedule
  if (schedule.kind !== 'sun') {
    sunPreview.value = null
    return
  }
  try {
    sunPreview.value = await api.sunPreview(schedule.latitude, schedule.longitude)
  } catch (error) {
    message.value = describeError(error)
  }
}

watch(
  () => {
    const schedule = draft.schedule
    return schedule.kind === 'sun' ? `${schedule.latitude},${schedule.longitude}` : ''
  },
  () => {
    void refreshSunPreview()
  },
  { immediate: true },
)

// 壁纸路径变化后重取缩略图（不改 draft，所以不会触发保存）
watch(
  () => `${draft.light_wallpaper ?? ''}|${draft.dark_wallpaper ?? ''}`,
  () => {
    void refreshPreviews()
  },
)
