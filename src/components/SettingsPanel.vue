<script setup lang="ts">
import { computed } from 'vue'

import {
  autostartEnabled,
  busy,
  clearLockScreen,
  draft,
  lockHelperPending,
  lockHelperReady,
  setAutostart,
  setLockScreen,
  setupLockHelper,
  state,
} from '../store'
import ToggleSwitch from './ToggleSwitch.vue'

/** 当前进程权限 + 助手任务状态的合并描述 */
const helperLabel = computed<string>(() => {
  if (lockHelperReady.value === true) {
    return '已注册'
  }
  if (lockHelperReady.value === false) {
    return '未注册'
  }
  return '状态未知'
})

function onLockScreen(value: boolean): void {
  void setLockScreen(value)
}

function onAutostart(value: boolean): void {
  void setAutostart(value)
}

function onSetupLockHelper(): void {
  void setupLockHelper()
}

function onClearLockScreen(): void {
  void clearLockScreen()
}
</script>

<template>
  <div class="card">
    <div class="card-head">
      <span class="label">系统集成</span>
    </div>

    <div class="card-body flex flex-col gap-4">
      <ToggleSwitch
        :model-value="draft.lock_screen"
        label="同时切换锁屏壁纸"
        hint="锁屏壁纸存在系统目录下，写入需要管理员权限；接管期间 Windows 设置里可能显示「部分设置由你的组织管理」"
        :loading="lockHelperPending"
        @update:model-value="onLockScreen"
      />

      <div v-if="draft.lock_screen" class="banner">
        <div class="flex flex-col gap-2">
          <span>
            当前进程{{ state?.elevated === true ? '已是' : '不是' }}管理员权限，锁屏助手任务{{ helperLabel }}。
            注册助手后切换锁屏不再弹授权框；主题与桌面壁纸始终不受影响。
          </span>
          <div class="flex gap-2">
            <button
              v-if="lockHelperReady !== true"
              class="btn btn-sm"
              type="button"
              :disabled="busy"
              @click="onSetupLockHelper"
            >
              注册锁屏助手（请求管理员权限）
            </button>
            <button class="btn btn-sm" type="button" :disabled="busy" @click="onClearLockScreen">
              撤销锁屏接管（请求管理员权限）
            </button>
          </div>
        </div>
      </div>

      <div class="h-px" style="background: var(--line)" />

      <ToggleSwitch
        :model-value="autostartEnabled"
        label="开机自动启动"
        hint="写入当前用户的启动项，登录后自动驻留托盘并接管调度"
        @update:model-value="onAutostart"
      />
    </div>
  </div>
</template>
