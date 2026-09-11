<script setup lang="ts">
import { computed } from 'vue'

import { clockToMinutes, minutesToClock } from '../format'
import { appendRule, draft, removeRule, setDays, setRuleMode, setScheduleKind, sunPreview, updateRule } from '../store'
import { DAY_LABEL, MODE_LABEL, type DayFilter, type Mode } from '../types'

const DAY_OPTIONS: DayFilter[] = ['daily', 'weekdays', 'weekend']
const MODE_OPTIONS: Mode[] = ['light', 'dark']

const isFixed = computed(() => draft.schedule.kind === 'fixed')

function onKind(kind: 'fixed' | 'sun'): void {
  setScheduleKind(kind)
}

function onTime(index: number, event: Event): void {
  const target = event.target
  if (!(target instanceof HTMLInputElement)) {
    return
  }
  const minutes = clockToMinutes(target.value)
  if (minutes === null) {
    // 输入非法，回填原值
    const rule = draft.schedule.kind === 'fixed' ? draft.schedule.rules[index] : undefined
    target.value = minutesToClock(rule?.minutes ?? 0)
    return
  }
  updateRule(index, { minutes })
}

function onRuleMode(index: number, event: Event): void {
  const target = event.target
  if (!(target instanceof HTMLSelectElement)) {
    return
  }
  setRuleMode(index, target.value === 'dark' ? 'dark' : 'light')
}

function onRuleDays(index: number, event: Event): void {
  const target = event.target
  if (!(target instanceof HTMLSelectElement)) {
    return
  }
  const value = target.value
  if (value === 'daily' || value === 'weekdays' || value === 'weekend') {
    setDays(index, value)
  }
}

function onNumber(field: 'latitude' | 'longitude', event: Event): void {
  const target = event.target
  if (!(target instanceof HTMLInputElement) || draft.schedule.kind !== 'sun') {
    return
  }
  const value = Number(target.value)
  if (!Number.isFinite(value)) {
    return
  }
  draft.schedule[field] = value
}
</script>

<template>
  <div class="card">
    <div class="card-head">
      <span class="label">切换时机</span>
      <div class="seg">
        <button type="button" :data-active="isFixed" @click="onKind('fixed')">固定时间</button>
        <button type="button" :data-active="!isFixed" @click="onKind('sun')">日出日落</button>
      </div>
    </div>

    <div v-if="draft.schedule.kind === 'fixed'" class="card-body flex flex-col gap-2">
      <div
        v-for="(rule, index) in draft.schedule.rules"
        :key="index"
        class="flex items-center gap-2"
      >
        <select
          class="input"
          style="width: 84px"
          :value="rule.mode"
          @change="onRuleMode(index, $event)"
        >
          <option v-for="mode in MODE_OPTIONS" :key="mode" :value="mode">
            {{ MODE_LABEL[mode] }}
          </option>
        </select>

        <input
          class="input"
          type="time"
          style="width: 108px"
          :value="minutesToClock(rule.minutes)"
          @change="onTime(index, $event)"
        />

        <select
          class="input"
          style="width: 120px"
          :value="rule.days"
          @change="onRuleDays(index, $event)"
        >
          <option v-for="day in DAY_OPTIONS" :key="day" :value="day">
            {{ DAY_LABEL[day] }}
          </option>
        </select>

        <button
          class="btn btn-sm ml-auto"
          type="button"
          :disabled="draft.schedule.rules.length <= 1"
          title="删除该时间点"
          @click="removeRule(index)"
        >
          删除
        </button>
      </div>

      <div class="flex items-center justify-between pt-1">
        <button class="btn btn-sm" type="button" @click="appendRule">添加时间点</button>
        <span class="text-[11.5px]" style="color: var(--muted)">
          到达时间点即切主题与壁纸，每天各一次
        </span>
      </div>
    </div>

    <div v-else class="card-body flex flex-col gap-3">
      <p class="text-[12px] leading-relaxed" style="color: var(--muted)">
        日出时切浅色、日落时切深色，按下面的经纬度在本地计算，不联网。
      </p>

      <div class="flex items-end gap-3">
        <label class="flex flex-col gap-1">
          <span class="label">纬度</span>
          <input
            class="input"
            style="width: 112px"
            type="number"
            step="0.0001"
            min="-90"
            max="90"
            :value="draft.schedule.latitude"
            @change="onNumber('latitude', $event)"
          />
        </label>
        <label class="flex flex-col gap-1">
          <span class="label">经度</span>
          <input
            class="input"
            style="width: 112px"
            type="number"
            step="0.0001"
            min="-180"
            max="180"
            :value="draft.schedule.longitude"
            @change="onNumber('longitude', $event)"
          />
        </label>
        <span class="pb-2 text-[11.5px]" style="color: var(--muted)">东经/北纬为正</span>
      </div>

      <div
        class="flex items-center gap-4 rounded-lg px-3 py-2 text-[12.5px]"
        style="background: var(--surface-2); border: 1px solid var(--line)"
      >
        <template v-if="sunPreview === null">
          <span style="color: var(--muted)">正在计算…</span>
        </template>
        <template v-else-if="sunPreview.polar">
          <span style="color: var(--warn-text)">该纬度当日为极昼或极夜，无法计算日出日落</span>
        </template>
        <template v-else>
          <span>{{ sunPreview.date }}</span>
          <span>日出 <b>{{ sunPreview.sunrise ?? '—' }}</b></span>
          <span>日落 <b>{{ sunPreview.sunset ?? '—' }}</b></span>
        </template>
      </div>
    </div>
  </div>
</template>
