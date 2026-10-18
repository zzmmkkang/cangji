<script setup lang="ts">
/**
 * ③ 库存总览（契约第 6 章 / D2 / 7.4）：
 * - 列表展示货物余量与今日进出；negative 行整行红底红字
 * - 已归档货物单独折叠区（get_inventory 含归档，前端与货物档案交叉比对折叠）
 * - 点行跳转该货物流水（/transactions?itemId=…）
 */
import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import EmptyState from "../components/EmptyState.vue";
import RedHintBadge from "../components/RedHintBadge.vue";
import { useInventoryStore } from "../stores/inventory";
import { useItemsStore } from "../stores/items";
import { formatQty } from "../utils/format";
import { toastError } from "../utils/toast";

const router = useRouter();
const inventory = useInventoryStore();
const itemsStore = useItemsStore();

const showArchived = ref(false);

const archivedIds = computed(
  () => new Set(itemsStore.list.filter((i) => i.isArchived).map((i) => i.id)),
);
const activeRows = computed(() =>
  inventory.rows.filter((r) => !archivedIds.value.has(r.itemId)),
);
const archivedRows = computed(() =>
  inventory.rows.filter((r) => archivedIds.value.has(r.itemId)),
);
const negativeCount = computed(() => inventory.negativeRows.length);

function openItem(itemId: string): void {
  router.push({ path: "/transactions", query: { itemId } });
}

onMounted(() => {
  inventory.fetch().catch(toastError);
  itemsStore.fetch().catch(toastError);
});
</script>

<template>
  <section class="mx-auto w-full max-w-3xl p-4 md:p-6">
    <div class="flex items-center justify-between">
      <h1 class="text-xl font-bold text-slate-800">库存总览</h1>
      <button
        type="button"
        class="text-xs text-emerald-600 hover:underline"
        @click="inventory.fetch().catch(toastError)"
      >
        刷新
      </button>
    </div>

    <!-- 负库存总提醒（D2） -->
    <p
      v-if="negativeCount > 0"
      class="mt-3 rounded-lg border border-red-200 bg-red-50 px-3 py-2 text-xs text-red-700"
    >
      有 {{ negativeCount }} 项货物余量低于 0（已标红，允许出库放行）。
    </p>

    <div v-if="inventory.loading && inventory.rows.length === 0" class="py-8 text-center text-sm text-slate-400">
      加载中…
    </div>
    <EmptyState
      v-else-if="inventory.error"
      title="库存加载失败"
      :description="inventory.error"
    />
    <EmptyState
      v-else-if="inventory.rows.length === 0"
      title="暂无库存数据"
      description="先在「货物管理」建档，再到「记一笔」录入进出。"
    />

    <template v-else>
      <!-- 有效货物 -->
      <div class="mt-4 space-y-2">
        <button
          v-for="row in activeRows"
          :key="row.itemId"
          type="button"
          class="w-full rounded-xl border p-3 text-left shadow-sm transition-colors"
          :class="
            row.negative
              ? 'border-red-300 bg-red-50 hover:border-red-400'
              : 'border-slate-200 bg-white hover:border-emerald-300'
          "
          @click="openItem(row.itemId)"
        >
          <div class="flex items-start justify-between gap-3">
            <div class="min-w-0">
              <div class="flex flex-wrap items-center gap-2">
                <span
                  class="min-w-0 truncate text-sm font-medium"
                  :class="row.negative ? 'text-red-700' : 'text-slate-800'"
                >
                  {{ row.itemName }}
                </span>
                <RedHintBadge v-if="row.negative" />
              </div>
              <div class="mt-1 text-xs text-slate-500">
                {{ row.spec ?? "无规格" }} · 单位 {{ row.unit }}
              </div>
              <div
                class="mt-1 text-xs"
                :class="row.negative ? 'text-red-600' : 'text-slate-400'"
              >
                今日进出：+{{ formatQty(row.todayIn) }} / −{{ formatQty(row.todayOut) }}
                {{ row.unit }}
              </div>
            </div>
            <div class="shrink-0 text-right">
              <div
                class="text-lg font-semibold tabular-nums"
                :class="row.negative ? 'text-red-600' : 'text-slate-800'"
              >
                {{ formatQty(row.balance) }}
              </div>
              <div class="text-[11px] text-slate-400">余量（{{ row.unit }}）</div>
            </div>
          </div>
        </button>

        <p v-if="activeRows.length === 0" class="py-6 text-center text-sm text-slate-400">
          暂无未归档货物
        </p>
      </div>

      <!-- 已归档货物（折叠） -->
      <div v-if="archivedRows.length > 0" class="mt-6">
        <button
          type="button"
          class="flex w-full items-center justify-between rounded-lg border border-slate-200 bg-slate-100 px-3 py-2 text-sm text-slate-600 hover:bg-slate-200"
          @click="showArchived = !showArchived"
        >
          <span>已归档货物（{{ archivedRows.length }}）</span>
          <span>{{ showArchived ? "收起 ▲" : "展开 ▼" }}</span>
        </button>

        <div v-if="showArchived" class="mt-2 space-y-2">
          <button
            v-for="row in archivedRows"
            :key="row.itemId"
            type="button"
            class="w-full rounded-xl border border-slate-200 bg-white p-3 text-left opacity-70 shadow-sm transition-colors hover:border-emerald-300"
            :class="row.negative ? 'border-red-300' : ''"
            @click="openItem(row.itemId)"
          >
            <div class="flex items-start justify-between gap-3">
              <div class="min-w-0">
                <div class="flex flex-wrap items-center gap-2">
                  <span
                    class="min-w-0 truncate text-sm font-medium"
                    :class="row.negative ? 'text-red-700 line-through' : 'text-slate-600'"
                  >
                    {{ row.itemName }}
                  </span>
                  <span
                    class="rounded bg-slate-100 px-1.5 py-0.5 text-[11px] text-slate-500"
                  >
                    已归档
                  </span>
                  <RedHintBadge v-if="row.negative" />
                </div>
                <div class="mt-1 text-xs text-slate-400">
                  {{ row.spec ?? "无规格" }} · 单位 {{ row.unit }}
                </div>
              </div>
              <div class="shrink-0 text-right">
                <div
                  class="text-base font-semibold tabular-nums"
                  :class="row.negative ? 'text-red-600' : 'text-slate-500'"
                >
                  {{ formatQty(row.balance) }}
                </div>
                <div class="text-[11px] text-slate-400">余量（{{ row.unit }}）</div>
              </div>
            </div>
          </button>
        </div>
      </div>
    </template>
  </section>
</template>
