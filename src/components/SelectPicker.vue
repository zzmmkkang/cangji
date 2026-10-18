<script setup lang="ts">
/**
 * 轻量下拉选择器：支持模糊搜索（label/hint 包含匹配，忽略大小写）、可清空。
 * 展开时渲染全屏透明遮罩捕获外部点击（无需自定义指令）。
 */
import { computed, nextTick, ref } from "vue";

interface PickerOption {
  id: string;
  label: string;
  /** 次要信息（规格/单位/类型等），参与模糊匹配 */
  hint?: string;
}

const props = withDefaults(
  defineProps<{
    modelValue: string | null;
    options: PickerOption[];
    placeholder?: string;
    clearable?: boolean;
    disabled?: boolean;
    emptyText?: string;
  }>(),
  {
    placeholder: "请选择",
    clearable: true,
    disabled: false,
    emptyText: "无匹配项",
  },
);

const emit = defineEmits<{
  "update:modelValue": [value: string | null];
}>();

const open = ref(false);
const query = ref("");
const inputEl = ref<HTMLInputElement | null>(null);

const selected = computed(() => props.options.find((o) => o.id === props.modelValue) ?? null);

const filtered = computed(() => {
  const q = query.value.trim().toLowerCase();
  if (!q) return props.options;
  return props.options.filter(
    (o) =>
      o.label.toLowerCase().includes(q) || (o.hint ?? "").toLowerCase().includes(q),
  );
});

async function toggle(): Promise<void> {
  if (props.disabled) return;
  open.value = !open.value;
  if (open.value) {
    query.value = "";
    await nextTick();
    inputEl.value?.focus();
  }
}

function pick(o: PickerOption): void {
  emit("update:modelValue", o.id);
  open.value = false;
}

function clear(): void {
  emit("update:modelValue", null);
  open.value = false;
}
</script>

<template>
  <div class="relative">
    <button
      type="button"
      class="flex w-full items-center justify-between gap-2 rounded-lg border px-3 py-2 text-left text-sm transition-colors"
      :class="
        disabled
          ? 'cursor-not-allowed border-slate-200 bg-slate-100 text-slate-400'
          : 'border-slate-300 bg-white text-slate-700 hover:border-emerald-400'
      "
      :aria-expanded="open"
      @click="toggle"
    >
      <span class="min-w-0 flex-1 truncate" :class="selected ? '' : 'text-slate-400'">
        {{ selected ? selected.label : placeholder }}
      </span>
      <span v-if="selected?.hint" class="shrink-0 truncate text-xs text-slate-400">
        {{ selected.hint }}
      </span>
      <svg class="h-4 w-4 shrink-0 text-slate-400" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
        <path
          fill-rule="evenodd"
          d="M5.23 7.21a.75.75 0 011.06.02L10 11.17l3.71-3.94a.75.75 0 111.08 1.04l-4.25 4.5a.75.75 0 01-1.08 0l-4.25-4.5a.75.75 0 01.02-1.06z"
          clip-rule="evenodd"
        />
      </svg>
    </button>

    <!-- 外部点击遮罩 -->
    <div v-if="open" class="fixed inset-0 z-20" @click="open = false"></div>

    <div
      v-if="open"
      class="absolute left-0 right-0 top-full z-30 mt-1 overflow-hidden rounded-lg border border-slate-200 bg-white shadow-lg"
    >
      <div class="border-b border-slate-100 p-2">
        <input
          ref="inputEl"
          v-model="query"
          type="text"
          placeholder="输入关键词搜索…"
          class="w-full rounded-md border border-slate-200 px-2.5 py-1.5 text-sm text-slate-700 outline-none focus:border-emerald-400"
        />
      </div>
      <ul class="max-h-56 overflow-y-auto py-1">
        <li v-if="filtered.length === 0" class="px-3 py-2 text-center text-xs text-slate-400">
          {{ emptyText }}
        </li>
        <li v-for="o in filtered" :key="o.id">
          <button
            type="button"
            class="flex w-full items-center justify-between gap-2 px-3 py-2 text-left text-sm hover:bg-emerald-50"
            :class="o.id === modelValue ? 'bg-emerald-50 font-medium text-emerald-700' : 'text-slate-700'"
            @click="pick(o)"
          >
            <span class="min-w-0 flex-1 truncate">{{ o.label }}</span>
            <span v-if="o.hint" class="shrink-0 text-xs text-slate-400">{{ o.hint }}</span>
          </button>
        </li>
        <li v-if="clearable && modelValue" class="border-t border-slate-100 pt-1">
          <button
            type="button"
            class="w-full px-3 py-2 text-left text-sm text-slate-500 hover:bg-slate-50"
            @click="clear"
          >
            清除选择
          </button>
        </li>
      </ul>
    </div>
  </div>
</template>
