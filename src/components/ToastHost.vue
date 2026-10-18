<script setup lang="ts">
import { toasts } from "../utils/toast";
import type { ToastKind } from "../utils/toast";

function kindClass(kind: ToastKind): string {
  switch (kind) {
    case "success":
      return "border-emerald-200 bg-emerald-50 text-emerald-700";
    case "error":
      return "border-red-200 bg-red-50 font-medium text-red-700";
    default:
      return "border-slate-200 bg-white text-slate-700";
  }
}
</script>

<template>
  <div
    class="pointer-events-none fixed inset-x-0 top-3 z-50 flex flex-col items-center gap-2 px-4"
    aria-live="polite"
  >
    <TransitionGroup name="toast">
      <div
        v-for="t in toasts"
        :key="t.id"
        class="pointer-events-auto max-w-full rounded-lg border px-4 py-2 text-center text-sm shadow-md"
        :class="kindClass(t.kind)"
      >
        {{ t.text }}
      </div>
    </TransitionGroup>
  </div>
</template>

<style scoped>
.toast-enter-active,
.toast-leave-active {
  transition:
    opacity 0.2s ease,
    transform 0.2s ease;
}
.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateY(-6px);
}
</style>
