<script setup lang="ts">
/** 通用空态 / 错误态占位。 */
defineProps<{
  title: string;
  description?: string;
}>();
</script>

<template>
  <div class="flex flex-col items-center justify-center rounded-xl border border-dashed border-slate-300 bg-white px-6 py-10 text-center">
    <p class="text-sm font-medium text-slate-600">{{ title }}</p>
    <p v-if="description" class="mt-1 max-w-sm text-xs leading-relaxed text-slate-400">
      {{ description }}
    </p>
    <div v-if="$slots.action" class="mt-4"><slot name="action" /></div>
  </div>
</template>
