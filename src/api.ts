/** 对 Tauri invoke 的薄封装。泛型是编译器推导，不是类型断言。 */

import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import { disable, enable, isEnabled } from '@tauri-apps/plugin-autostart'

import type { AppConfig, AppState, Mode, SunPreview } from './types'

const IMAGE_EXTENSIONS = ['jpg', 'jpeg', 'png', 'bmp', 'webp']

export async function getState(): Promise<AppState> {
  return await invoke<AppState>('get_state')
}

export async function saveConfig(config: AppConfig): Promise<AppState> {
  return await invoke<AppState>('save_config', { config })
}

export async function applyNow(mode?: Mode): Promise<AppState> {
  return await invoke<AppState>('apply_now', { mode: mode ?? null })
}

export async function setPaused(paused: boolean): Promise<AppState> {
  return await invoke<AppState>('set_paused', { paused })
}

export async function sunPreview(latitude: number, longitude: number): Promise<SunPreview> {
  return await invoke<SunPreview>('sun_preview', { latitude, longitude })
}

export async function openColorSettings(): Promise<void> {
  await invoke('open_color_settings')
}

export async function dismissConflict(): Promise<AppState> {
  return await invoke<AppState>('dismiss_conflict')
}

export async function clearLockScreen(): Promise<AppState> {
  return await invoke<AppState>('clear_lock_screen')
}

/** 注册锁屏提权助手任务。未提权时会弹一次 UAC，之后切换锁屏不再需要授权。 */
export async function setupLockHelper(): Promise<void> {
  await invoke('setup_lock_helper')
}

/** 助手任务是否已注册。按需查询，不会随状态事件下发。 */
export async function lockHelperReady(): Promise<boolean> {
  return await invoke<boolean>('lock_helper_ready')
}

/** 让 Rust 侧解码并缩放壁纸，返回 data URL。文件不存在时返回 null。 */
export async function wallpaperPreview(path: string): Promise<string | null> {
  return await invoke<string | null>('wallpaper_preview', { path })
}

function hasPathField(value: object): value is { path: unknown } {
  return 'path' in value
}

/** 文件选择框的返回值在不同版本里可能是 string / string[] / FileResponse，统一取出路径。 */
function extractPath(value: unknown): string | null {
  if (typeof value === 'string') {
    return value
  }
  if (Array.isArray(value)) {
    return value.length > 0 ? extractPath(value[0]) : null
  }
  if (typeof value === 'object' && value !== null && hasPathField(value)) {
    return extractPath(value.path)
  }
  return null
}

/** 弹出系统文件选择框，返回绝对路径；取消返回 null。 */
export async function pickWallpaperImage(): Promise<string | null> {
  const picked = await open({
    multiple: false,
    directory: false,
    title: '选择壁纸图片',
    filters: [{ name: '图片', extensions: IMAGE_EXTENSIONS }],
  })

  return extractPath(picked)
}

/** 读取开机自启状态。插件不可用时按未开启处理。 */
export async function getAutostart(): Promise<boolean> {
  try {
    return await isEnabled()
  } catch {
    return false
  }
}

export async function setAutostart(on: boolean): Promise<void> {
  if (on) {
    await enable()
  } else {
    await disable()
  }
}
