<script setup lang="ts">
/**
 * ④ 流水记录（契约第 6 章）：
 * - 筛选（类型/货物/往来/操作人/日期段/关键词）+ 分页（total，limit 100）
 * - 每条操作：作废（ConfirmDialog 二次确认，作废=追加回冲单不删记录）、
 *   复制上次单据（copied_from 预填进记一笔）
 * - 支持 /transactions?itemId=… 预筛选（库存总览点行跳转）
 */
import { computed, onMounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { voidTransaction } from "../api/commands";
import type { Transaction } from "../api/types";
import ConfirmDialog from "../components/ConfirmDialog.vue";
import EmptyState from "../components/EmptyState.vue";
import SelectPicker from "../components/SelectPicker.vue";
import TxCard from "../components/TxCard.vue";
import { useInventoryStore } from "../stores/inventory";
import { useItemsStore } from "../stores/items";
import { usePartnersStore } from "../stores/partners";
import { useSessionStore } from "../stores/session";
import { PAGE_LIMIT, useTransactionsStore } from "../stores/transactions";
import { toastError, toastSuccess } from "../utils/toast";

const route = useRoute();
const router = useRouter();
const txStore = useTransactionsStore();
const itemsStore = useItemsStore();
const partnersStore = usePartnersStore();
const session = useSessionStore();
const inventory = useInventoryStore();

const voidTarget = ref<Transaction | null>(null);
const voidReason = ref("");
const voiding = ref(false);

const rows = computed(() => txStore.page?.rows ?? []);
const total = computed(() => txStore.page?.total ?? 0);
const rangeText = computed(() => {
  if (total.value === 0) return "共 0 条";
  const start = txStore.offset + 1;
  const end = txStore.offset + rows.value.length;
  return `第 ${start}–${end} 条 / 共 ${total.value} 条`;
});
const totalPages = computed(() => Math.max(1, Math.ceil(total.value / PAGE_LIMIT)));
const pageNo = computed(() => Math.floor(txStore.offset / PAGE_LIMIT) + 1);

/** 筛选选择器含已归档档案（历史流水的档案可能已归档）。 */
const itemOptions = computed(() =>
  itemsStore.list.map((i) => ({
    id: i.id,
    label: i.name,
    hint: i.isArchived ? "已归档" : i.unit,
  })),
);
const partnerOptions = computed(() =>
  partnersStore.list.map((p) => ({ id: p.id, label: p.name, hint: p.isArchived ? "已归档" : undefined })),
);
const operatorOptions = computed(() =>
  session.operators.map((o) => ({ id: o.id, label: o.name })),
);

async function applyFilter(): Promise<void> {
  txStore.offset = 0;
  try {
    await txStore.fetchPage();
  } catch (e) {
    toastError(e);
  }
}

async function resetAll(): Promise<void> {
  txStore.resetFilter();
  try {
    await txStore.fetchPage();
  } catch (e) {
    toastError(e);
  }
}

async function goPage(delta: number): Promise<void> {
  const next = txStore.offset + delta * PAGE_LIMIT;
  if (next < 0 || next >= total.value) return;
  txStore.offset = next;
  try {
    await txStore.fetchPage();
  } catch (e) {
    toastError(e);
  }
}

function askVoid(tx: Transaction): void {
  voidTarget.value = tx;
  voidReason.value = "";
}

async function confirmVoid(): Promise<void> {
  const target = voidTarget.value;
  if (!target || voiding.value) return;
  voiding.value = true;
  try {
    await voidTransaction(target.id, voidReason.value.trim() || undefined);
    toastSuccess("已作废：追加回冲单完成，原单保留可追溯");
    voidTarget.value = null;
    await txStore.fetchPage();
    inventory.fetch().catch(toastError);
  } catch (e) {
    toastError(e);
  } finally {
    voiding.value = false;
  }
}

function copyTx(tx: Transaction): void {
  txStore.setPrefill(tx);
  router.push("/entry");
}

onMounted(() => {
  // 库存总览等入口带 ?itemId= 预筛选
  const q = route.query.itemId;
  if (typeof q === "string" && q) {
    txStore.filter.itemId = q;
    txStore.offset = 0;
  }
  itemsStore.fetch().catch(toastError);
  partnersStore.fetch().catch(toastError);
  session.loadOperators().catch(toastError);
  txStore.fetchPage().catch(toastError);
});
</script>

<template>
  <section class="mx-auto w-full max-w-3xl p-4 md:p-6">
    <div class="flex items-center justify-between">
      <h1 class="text-xl font-bold text-slate-800">流水记录</h1>
      <span v-if="totalPages > 1" class="text-xs text-slate-400">
        第 {{ pageNo }} / {{ totalPages }} 页
      </span>
    </div>

    <!-- 筛选栏 -->
    <div class="mt-3 rounded-xl border border-slate-200 bg-white p-3">
      <div class="grid grid-cols-2 gap-2 md:grid-cols-4">
        <select
          v-model="txStore.filter.type"
          class="w-full rounded-lg border border-slate-300 px-2 py-2 text-sm text-slate-700 outline-none focus:border-emerald-400"
        >
          <option value="">全部类型</option>
          <option value="IN">入库 IN</option>
          <option value="OUT">出库 OUT</option>
          <option value="VOID">作废 VOID</option>
        </select>
        <SelectPicker
          v-model="txStore.filter.itemId"
          :options="itemOptions"
          placeholder="全部货物"
        />
        <SelectPicker
          v-model="txStore.filter.partnerId"
          :options="partnerOptions"
          placeholder="全部往来单位"
        />
        <SelectPicker
          v-model="txStore.filter.operatorId"
          :options="operatorOptions"
          placeholder="全部操作人"
        />

        <div>
          <span class="mb-0.5 block text-[11px] text-slate-400">起始日期</span>
          <input
            v-model="txStore.filter.dateFrom"
            type="date"
            class="w-full rounded-lg border border-slate-300 px-2 py-1.5 text-sm text-slate-700 outline-none focus:border-emerald-400"
          />
        </div>
        <div>
          <span class="mb-0.5 block text-[11px] text-slate-400">截止日期</span>
          <input
            v-model="txStore.filter.dateTo"
            type="date"
            class="w-full rounded-lg border border-slate-300 px-2 py-1.5 text-sm text-slate-700 outline-none focus:border-emerald-400"
          />
        </div>
        <input
          v-model="txStore.filter.keyword"
          type="text"
          placeholder="关键词（货物/单位/备注）"
          class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm outline-none focus:border-emerald-400"
          @keyup.enter="applyFilter"
        />
        <div class="flex gap-2">
          <button
            type="button"
            class="flex-1 rounded-lg bg-emerald-600 px-3 py-2 text-sm font-medium text-white hover:bg-emerald-700"
            @click="applyFilter"
          >
            查询
          </button>
          <button
            type="button"
            class="flex-1 rounded-lg border border-slate-200 px-3 py-2 text-sm text-slate-600 hover:bg-slate-50"
            @click="resetAll"
          >
            重置
          </button>
        </div>
      </div>
    </div>

    <!-- 列表 -->
    <div class="mt-4">
      <div v-if="txStore.loading && rows.length === 0" class="py-8 text-center text-sm text-slate-400">
        加载中…
      </div>
      <EmptyState
        v-else-if="txStore.error"
        title="流水加载失败"
        :description="txStore.error"
      />
      <EmptyState
        v-else-if="rows.length === 0"
        title="没有符合条件的流水"
        description="调整筛选条件，或先去「记一笔」记账。"
      />
      <div v-else class="space-y-2">
        <TxCard
          v-for="tx in rows"
          :key="tx.id"
          :tx="tx"
          :voided="txStore.voidedIds.has(tx.id)"
        >
          <template #actions>
            <button
              v-if="tx.type !== 'VOID'"
              type="button"
              class="rounded-md border border-slate-200 px-2.5 py-1 text-xs text-slate-600 hover:bg-slate-50"
              @click="copyTx(tx)"
            >
              复制此单
            </button>
            <button
              v-if="tx.type !== 'VOID' && !txStore.voidedIds.has(tx.id)"
              type="button"
              class="rounded-md border border-red-200 px-2.5 py-1 text-xs text-red-600 hover:bg-red-50"
              @click="askVoid(tx)"
            >
              作废
            </button>
          </template>
        </TxCard>
      </div>
    </div>

    <!-- 分页 -->
    <div
      v-if="rows.length > 0"
      class="mt-4 flex items-center justify-between gap-2"
    >
      <button
        type="button"
        class="rounded-lg border border-slate-200 bg-white px-4 py-2 text-sm text-slate-600 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-40"
        :disabled="txStore.offset <= 0 || txStore.loading"
        @click="goPage(-1)"
      >
        上一页
      </button>
      <span class="text-xs tabular-nums text-slate-500">{{ rangeText }}</span>
      <button
        type="button"
        class="rounded-lg border border-slate-200 bg-white px-4 py-2 text-sm text-slate-600 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-40"
        :disabled="txStore.offset + PAGE_LIMIT >= total || txStore.loading"
        @click="goPage(1)"
      >
        下一页
      </button>
    </div>

    <!-- 作废二次确认 -->
    <ConfirmDialog
      v-if="voidTarget"
      title="作废该单据？"
      message="作废 = 追加一张回冲单（VOID），不删除原记录；库存按原单方向回冲，原单仍可追溯。"
      confirm-text="确认作废"
      @confirm="confirmVoid"
      @cancel="voidTarget = null"
    >
      <label class="mb-1 block text-xs font-medium text-slate-500">作废原因（选填）</label>
      <input
        v-model="voidReason"
        type="text"
        maxlength="200"
        placeholder="如：录错了数量"
        class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm outline-none focus:border-red-400"
      />
      <p v-if="voiding" class="mt-2 text-xs text-slate-400">提交中…</p>
    </ConfirmDialog>
  </section>
</template>
