<script setup lang="ts">
defineProps<{
  modelValue: boolean
  label: string
  hint?: string
  disabled?: boolean
}>()

const emit = defineEmits<{
  (event: 'update:modelValue', value: boolean): void
}>()
</script>

<template>
  <div class="flex items-center justify-between gap-4">
    <div v-if="label !== '' || hint !== undefined" class="flex flex-col gap-0.5">
      <span v-if="label !== ''" class="text-[13px]">{{ label }}</span>
      <span v-if="hint !== undefined" class="text-[11.5px] leading-snug" style="color: var(--muted)">
        {{ hint }}
      </span>
    </div>

    <button
      type="button"
      role="switch"
      :aria-checked="modelValue"
      :aria-label="label"
      :disabled="disabled === true"
      :style="{
        width: '40px',
        height: '22px',
        background: modelValue ? 'var(--accent)' : 'var(--line-strong)',
        opacity: disabled === true ? 0.5 : 1,
        cursor: disabled === true ? 'not-allowed' : 'pointer',
      }"
      class="relative shrink-0 rounded-full border-0 p-0 transition-colors"
      @click="emit('update:modelValue', !modelValue)"
    >
      <span
        class="absolute top-[2px] block size-[18px] rounded-full bg-white transition-all"
        :style="{ left: modelValue ? '20px' : '2px' }"
      />
    </button>
  </div>
</template>
