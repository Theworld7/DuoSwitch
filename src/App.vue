<script setup lang="ts">
import { computed, onMounted } from 'vue'

import SchedulePanel from './components/SchedulePanel.vue'
import SettingsPanel from './components/SettingsPanel.vue'
import ToggleSwitch from './components/ToggleSwitch.vue'
import WallpaperCard from './components/WallpaperCard.vue'
import { describeMoment } from './format'
import {
  applyNow,
  bootstrap,
  busy,
  chooseWallpaper,
  conflict,
  dismissConflict,
  dismissMessage,
  draft,
  firstRunSeeded,
  isPaused,
  message,
  openColorSettings,
  previews,
  setEnabled,
  setWallpaper,
  state,
  togglePause,
} from './store'
import { MODE_LABEL, type Mode } from './types'

const themeLabel = computed(() => {
  const theme = state.value?.theme
  return theme === undefined ? '—' : MODE_LABEL[theme]
})

const nextLabel = computed(() => describeMoment(state.value?.next?.at ?? null))

const conflictText = computed(() => {
  const current = conflict.value
  if (current === null) {
    return ''
  }
  return `调度认为此刻应为「${MODE_LABEL[current.expected]}」，但系统主题是「${MODE_LABEL[current.actual]}」`
})

function pick(mode: Mode): void {
  void chooseWallpaper(mode)
}

function clear(mode: Mode): void {
  void setWallpaper(mode, null)
}

onMounted(() => {
  void bootstrap()
})
</script>

<template>
  <div class="flex h-full flex-col">
    <header
      class="flex items-center justify-between gap-4 px-5 py-3"
      style="border-bottom: 1px solid var(--line); background: var(--surface)"
    >
      <div class="flex flex-col">
        <h1 class="text-[15px] font-semibold">DuoSwitch</h1>
        <span class="text-[11.5px]" style="color: var(--muted)">
          按时间自动切换 Windows 明暗主题与壁纸
        </span>
      </div>

      <div class="flex items-center gap-3">
        <span class="pill">系统当前：{{ themeLabel }}</span>
        <span class="pill">下次切换：{{ nextLabel }}</span>
        <span class="text-[12px]" style="color: var(--muted)">启用调度</span>
        <ToggleSwitch
          :model-value="draft.enabled"
          label=""
          :disabled="busy"
          @update:model-value="setEnabled"
        />
      </div>
    </header>

    <main class="scroll flex flex-1 flex-col gap-4 p-5">
      <div class="mx-auto flex w-full max-w-[900px] flex-col gap-4">
        <div v-if="conflict !== null" class="banner">
          <div class="flex flex-col gap-2">
            <span>
              <b>检测到系统在本时段内改动了主题。</b>{{ conflictText }}。壁纸已跟着系统主题走，
              但没有夺回控制权 —— 强行夺回会和系统调度互相翻转。
            </span>
            <span>
              如果这是 Windows 原生的「自动在浅色和深色间切换」造成的，建议到系统设置里关掉它，
              由本软件统一调度。
            </span>
            <div class="flex gap-2">
              <button class="btn btn-sm" type="button" @click="openColorSettings">
                打开颜色设置
              </button>
              <button class="btn btn-sm" type="button" @click="dismissConflict">忽略本次提示</button>
            </div>
          </div>
        </div>

        <p v-if="message !== null" class="banner" @click="dismissMessage">
          {{ message }}
        </p>

        <p v-if="!draft.enabled" class="banner">
          调度已关闭，本软件不会自动改动主题与壁纸，只保留手动切换与托盘菜单。
        </p>

        <p v-if="firstRunSeeded" class="banner">
          已为你内置了一对示例壁纸（沙漠日间 / 夜间）。点右下角「立即应用」即生效，
          也可以在左侧「浅色 / 深色模式壁纸」处换成自己的图。
        </p>

        <div class="grid grid-cols-[300px_minmax(0,1fr)] items-start gap-5">
          <div class="flex flex-col gap-4">
            <div class="card">
              <WallpaperCard
                mode="light"
                title="浅色模式壁纸"
                :path="draft.light_wallpaper"
                :preview="previews.light"
                :disabled="busy"
                @pick="pick('light')"
                @clear="clear('light')"
              />
            </div>
            <div class="card">
              <WallpaperCard
                mode="dark"
                title="深色模式壁纸"
                :path="draft.dark_wallpaper"
                :preview="previews.dark"
                :disabled="busy"
                @pick="pick('dark')"
                @clear="clear('dark')"
              />
            </div>
          </div>

          <div class="flex flex-col gap-4">
            <SchedulePanel />
            <SettingsPanel />
          </div>
        </div>

        <p v-if="state?.last_error !== null && state?.last_error !== undefined" class="banner">
          上次操作有错误：{{ state.last_error }}
        </p>
      </div>
    </main>

    <footer
      class="flex items-center justify-between gap-4 px-5 py-3"
      style="border-top: 1px solid var(--line); background: var(--surface)"
    >
      <span class="text-[11.5px]" style="color: var(--muted)">
        关闭窗口只收进托盘，调度继续运行。使用托盘菜单可以退出。
      </span>
      <div class="flex gap-2">
        <button class="btn" type="button" :disabled="busy" @click="togglePause">
          {{ isPaused ? '恢复调度' : '暂停调度' }}
        </button>
        <button class="btn btn-primary" type="button" :disabled="busy" @click="applyNow">
          立即应用
        </button>
      </div>
    </footer>
  </div>
</template>
