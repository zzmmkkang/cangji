<script setup lang="ts">
/**
 * 轻量二次确认弹窗（Teleport 到 body，避免父级 stacking 干扰）。
 * 父级用 v-if 控制显隐；确认/取消通过事件回传，默认插槽可放附加内容（如作废原因）。
 */
withDefaults(
  defineProps<{
    title: string;
    message?: string;
    confirmText?: string;
    cancelText?: string;
  }>(),
  { confirmText: "确认", cancelText: "取消" },
);

const emit = defineEmits<{
  confirm: [];
  cancel: [];
}>();
</script>

<template>
  <Teleport to="body">
    <div class="fixed inset-0 z-40 flex items-center justify-center p-4">
      <div class="absolute inset-0 bg-slate-900/40" @click="emit('cancel')"></div>
      <div
        class="relative w-full max-w-sm rounded-xl bg-white p-5 shadow-xl"
        role="dialog"
        aria-modal="true"
      >
        <h2 class="text-base font-semibold text-slate-800">{{ title }}</h2>
        <p v-if="message" class="mt-2 text-sm leading-relaxed text-slate-500">{{ message }}</p>
        <div v-if="$slots.default" class="mt-3"><slot /></div>
        <div class="mt-5 flex justify-end gap-2">
          <button
            type="button"
            class="rounded-lg border border-slate-200 px-4 py-2 text-sm text-slate-600 hover:bg-slate-50"
            @click="emit('cancel')"
          >
            {{ cancelText }}
          </button>
          <button
            type="button"
            class="rounded-lg bg-red-600 px-4 py-2 text-sm font-medium text-white hover:bg-red-700"
            @click="emit('confirm')"
          >
            {{ confirmText }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
