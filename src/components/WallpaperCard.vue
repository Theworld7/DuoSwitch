<script setup lang="ts">
import { computed } from 'vue'

import { baseName } from '../format'
import type { Mode } from '../types'

const props = defineProps<{
  mode: Mode
  title: string
  path: string | null
  preview: string | null
  disabled?: boolean
}>()

const emit = defineEmits<{
  (event: 'pick'): void
  (event: 'clear'): void
}>()

const fileName = computed(() => baseName(props.path))
</script>

<template>
  <div class="card-body flex flex-col gap-3">
    <div class="flex items-center justify-between">
      <span class="label">{{ title }}</span>
      <span v-if="mode === 'dark'" class="text-[11px]" style="color: var(--muted)">夜间自动使用</span>
    </div>

    <img v-if="preview !== null" class="thumb" :src="preview" :alt="title" />
    <div v-else class="thumb-empty">未选择壁纸</div>

    <div class="flex items-center justify-between gap-3">
      <p class="min-w-0 flex-1 truncate text-[12px]" :title="path ?? ''" style="color: var(--muted)">
        {{ path === null ? '—' : fileName }}
      </p>

      <div class="flex shrink-0 gap-2">
        <button class="btn btn-sm" type="button" :disabled="disabled === true" @click="emit('pick')">
          选择图片
        </button>
        <button
          class="btn btn-sm"
          type="button"
          :disabled="disabled === true || path === null"
          @click="emit('clear')"
        >
          清除
        </button>
      </div>
    </div>
  </div>
</template>
