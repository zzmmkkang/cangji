<script setup lang="ts">
import { computed } from "vue";
import type { Transaction } from "../api/types";
import { formatQty, formatTime } from "../utils/format";

/**
 * 流水卡片：类型徽标 + 货物/数量 + 往来/操作人/业务时间/备注。
 * VOID 单或被作废原单（voided=true，由页面据同页 VOID 行 refId 推导）置灰 + 删除线。
 */
const props = withDefaults(
  defineProps<{
    tx: Transaction;
    /** 该单已被作废（同页 VOID 行 refId 指向本单） */
    voided?: boolean;
  }>(),
  { voided: false },
);

const isVoided = computed(() => props.voided || props.tx.type === "VOID");

const typeLabel = computed(() => {
  switch (props.tx.type) {
    case "IN":
      return "入库";
    case "OUT":
      return "出库";
    default:
      return "作废";
  }
});

const badgeClass = computed(() => {
  switch (props.tx.type) {
    case "IN":
      return "bg-emerald-100 text-emerald-700";
    case "OUT":
      return "bg-amber-100 text-amber-700";
    default:
      return "bg-slate-200 text-slate-600";
  }
});

const qtyClass = computed(() => {
  if (isVoided.value) return "text-slate-400";
  switch (props.tx.type) {
    case "IN":
      return "text-emerald-700";
    case "OUT":
      return "text-amber-700";
    default:
      return "text-slate-500";
  }
});

const sign = computed(() => (props.tx.type === "IN" ? "+" : props.tx.type === "OUT" ? "−" : ""));
</script>

<template>
  <article
    class="rounded-xl border bg-white p-3 shadow-sm"
    :class="isVoided ? 'border-slate-200 opacity-70' : 'border-slate-200'"
  >
    <div class="flex items-start justify-between gap-3">
      <div class="min-w-0">
        <div class="flex flex-wrap items-center gap-2">
          <span
            class="shrink-0 rounded px-1.5 py-0.5 text-xs font-medium"
            :class="badgeClass"
          >
            {{ typeLabel }}
          </span>
          <span
            class="min-w-0 truncate text-sm font-medium"
            :class="isVoided ? 'text-slate-500 line-through' : 'text-slate-800'"
          >
            {{ tx.itemName }}
          </span>
          <span
            v-if="tx.copiedFrom"
            class="shrink-0 rounded bg-slate-100 px-1.5 py-0.5 text-[11px] text-slate-500"
            title="该单由「复制上次单据」产生"
          >
            复制单
          </span>
        </div>
        <div class="mt-1 text-xs leading-relaxed text-slate-500">
          {{ tx.partnerName ?? "未指定往来" }} · {{ tx.operatorName }} ·
          {{ formatTime(tx.happenedAt) }}
        </div>
        <p v-if="tx.remark" class="mt-1 break-words text-xs text-slate-400">
          备注：{{ tx.remark }}
        </p>
      </div>
      <div class="shrink-0 text-right">
        <span
          class="text-lg font-semibold tabular-nums"
          :class="[qtyClass, isVoided ? 'line-through' : '']"
        >
          {{ sign }}{{ formatQty(tx.quantity) }}
        </span>
        <span class="ml-0.5 text-xs text-slate-400">{{ tx.unit }}</span>
      </div>
    </div>

    <div
      v-if="$slots.actions"
      class="mt-2 flex justify-end gap-2 border-t border-slate-100 pt-2"
    >
      <slot name="actions" />
    </div>
  </article>
</template>
