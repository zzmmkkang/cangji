<script setup lang="ts">
/**
 * ① 速记主页（契约第 6 章，A4 交付）：
 * - 自然语言规则解析（utils/parser.ts，规划书 5.2）→ 待核对单据卡片 → 一键提交
 *   唯一命中直接带出 / 多候选择一 / 零命中 [立即建档][改为备注]（规则 3）
 * - 今日进出摘要（按单据笔数）+ 负库存提醒
 * - 最近流水快览（作废置灰/删除线）
 */
import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { createTransaction, listTransactions, toCommandError } from "../api/commands";
import type { NewTransaction, TxType } from "../api/types";
import EmptyState from "../components/EmptyState.vue";
import RedHintBadge from "../components/RedHintBadge.vue";
import SelectPicker from "../components/SelectPicker.vue";
import TxCard from "../components/TxCard.vue";
import { useInventoryStore } from "../stores/inventory";
import { useItemsStore } from "../stores/items";
import { usePartnersStore } from "../stores/partners";
import { useSessionStore } from "../stores/session";
import { computeVoidedIds, useTransactionsStore } from "../stores/transactions";
import { dayEndUtc, dayStartUtc, formatQty, todayStr, validateQuantity } from "../utils/format";
import type { MemoParse } from "../utils/parser";
import { parseMemo } from "../utils/parser";
import { toast, toastError, toastSuccess } from "../utils/toast";

const router = useRouter();
const txStore = useTransactionsStore();
const inventory = useInventoryStore();
const itemsStore = useItemsStore();
const partnersStore = usePartnersStore();
const session = useSessionStore();

const todayInCount = ref(0);
const todayOutCount = ref(0);
const summaryError = ref<string | null>(null);

// ============ 自然语言速记（A4） ============
const memoText = ref("");
const parsed = ref<MemoParse | null>(null);
const parsing = ref(false);
const submitting = ref(false);
const fieldError = ref("");
const noItemAcked = ref(false);

// 待核对单据卡片（解析后可任意修改）
const cardType = ref<TxType>("IN");
const cardItemId = ref<string | null>(null);
const cardQuantity = ref("");
const cardPartnerId = ref<string | null>(null);
const cardOperatorId = ref<string | null>(null);
const cardRemark = ref("");

const itemOptions = computed(() =>
  itemsStore.active.map((i) => ({
    id: i.id,
    label: i.name,
    hint: `${i.spec ? i.spec + " · " : ""}${i.unit}`,
  })),
);
const partnerOptions = computed(() =>
  partnersStore.active.map((p) => ({
    id: p.id,
    label: p.name,
    hint: p.type === "supplier" ? "供应商" : p.type === "customer" ? "客户" : "两者皆是",
  })),
);
const operatorOptions = computed(() =>
  session.operators.map((o) => ({ id: o.id, label: o.name })),
);
const selectedItem = computed(() => itemsStore.list.find((i) => i.id === cardItemId.value) ?? null);

const recentVoided = computed(() => computeVoidedIds(txStore.recent));
const negativeCount = computed(() => inventory.negativeRows.length);

/** 规则 1–4：解析输入，组装待核对卡片（唯一命中直接带出）。 */
function doParse(): void {
  const text = memoText.value.trim();
  if (!text) {
    toast("请输入速记内容", "error");
    return;
  }
  parsing.value = true;
  try {
    const result = parseMemo(text, {
      items: itemsStore.list,
      partners: partnersStore.list,
      operators: session.operators,
    });
    parsed.value = result;
    noItemAcked.value = false;
    fieldError.value = "";
    cardType.value = result.direction ?? "IN";
    cardQuantity.value = result.quantity !== null ? String(result.quantity) : "";
    cardItemId.value = result.itemCandidates.length === 1 ? result.itemCandidates[0].id : null;
    cardPartnerId.value =
      result.partnerCandidates.length === 1 ? result.partnerCandidates[0].id : null;
    const remembered =
      session.currentOperatorId &&
      session.operators.some((o) => o.id === session.currentOperatorId)
        ? session.currentOperatorId
        : null;
    cardOperatorId.value =
      result.operatorCandidates.length === 1
        ? result.operatorCandidates[0].id
        : remembered ?? session.operators[0]?.id ?? null;
    cardRemark.value = result.leftover;

    // 单位仅作提示不自动换算（parser 已知局限），不一致时红字提醒核对
    if (result.unit && cardItemId.value) {
      const item = itemsStore.list.find((i) => i.id === cardItemId.value);
      if (item && item.unit !== result.unit) {
        toast(`输入单位「${result.unit}」与货物单位「${item.unit}」不一致，请核对数量`, "error");
      }
    }
    if (result.warnings.length > 0) {
      toast(result.warnings.join("；"), "info");
    }
  } finally {
    parsing.value = false;
  }
}

/** 卡片一键提交：校验 → create_transaction → 刷新库存与摘要。 */
async function submitCard(): Promise<void> {
  if (submitting.value) return;
  if (!cardItemId.value) {
    fieldError.value = parsed.value && parsed.value.itemCandidates.length === 0
      ? "未匹配到货物：点「立即建档」新增，或点「改为备注」后到「记一笔」补录"
      : "请选择货物";
    return;
  }
  if (!cardOperatorId.value) {
    fieldError.value = "请选择操作人";
    return;
  }
  const qtyErr = validateQuantity(cardQuantity.value, selectedItem.value?.unit ?? "");
  if (qtyErr) {
    fieldError.value = qtyErr;
    return;
  }
  fieldError.value = "";
  submitting.value = true;
  const input: NewTransaction = {
    type: cardType.value,
    itemId: cardItemId.value,
    quantity: Number(cardQuantity.value),
    partnerId: cardPartnerId.value,
    operatorId: cardOperatorId.value,
    remark: cardRemark.value.trim() || null,
  };
  try {
    const tx = await createTransaction(input);
    toastSuccess(
      `已记一笔：${tx.itemName} ${tx.type === "IN" ? "入库" : "出库"} ${formatQty(tx.quantity)}${tx.unit}`,
    );
    // 契约 7.4：提交后刷新库存，负库存红字即时提示（不拦截）
    await inventory.fetch();
    const row = inventory.rowOf(tx.itemId);
    if (row && row.negative) {
      toast(`已提交，当前余量 ${formatQty(row.balance)}（低于 0）`, "error");
    }
    // 清空速记状态，刷新最近流水与今日摘要
    memoText.value = "";
    parsed.value = null;
    cardQuantity.value = "";
    cardRemark.value = "";
    cardItemId.value = null;
    cardPartnerId.value = null;
    await Promise.all([loadRecent(), loadToday()]);
  } catch (e) {
    toastError(e);
  } finally {
    submitting.value = false;
  }
}

async function loadRecent(): Promise<void> {
  try {
    await txStore.fetchRecent(5);
  } catch (e) {
    toastError(e);
  }
}

async function loadToday(): Promise<void> {
  summaryError.value = null;
  const dateFrom = dayStartUtc(todayStr());
  const dateTo = dayEndUtc(todayStr());
  try {
    const [inPage, outPage] = await Promise.all([
      listTransactions({ type: "IN", dateFrom, dateTo, limit: 1 }),
      listTransactions({ type: "OUT", dateFrom, dateTo, limit: 1 }),
    ]);
    todayInCount.value = inPage.total;
    todayOutCount.value = outPage.total;
  } catch (e) {
    summaryError.value = toCommandError(e).message;
    toastError(e);
  }
}

onMounted(() => {
  loadRecent();
  loadToday();
  inventory.fetch().catch(toastError);
  // 速记解析需要三份档案目录
  itemsStore.fetch().catch(toastError);
  partnersStore.fetch().catch(toastError);
  session.loadOperators().catch(toastError);
});
</script>

<template>
  <section class="mx-auto w-full max-w-3xl p-4 md:p-6">
    <h1 class="text-xl font-bold text-slate-800">速记</h1>

    <!-- 自然语言速记（A4）：规则解析 → 待核对卡片 -->
    <div class="mt-4">
      <div class="flex gap-2">
        <input
          v-model="memoText"
          type="text"
          placeholder="例：入库 螺丝 50 张三 / 出 自攻钉2箱 老王"
          class="w-full rounded-xl border border-slate-300 bg-white px-4 py-3 text-sm text-slate-800 outline-none focus:border-emerald-400"
          @keyup.enter="doParse"
        />
        <button
          type="button"
          class="shrink-0 rounded-xl bg-slate-800 px-4 py-3 text-sm font-semibold text-white hover:bg-slate-700 disabled:opacity-60"
          :disabled="parsing"
          @click="doParse"
        >
          {{ parsing ? "解析中…" : "解析" }}
        </button>
      </div>
      <p class="mt-1.5 text-xs text-slate-400">
        支持方向词（入/进/收 → 入库，出/发/销 → 出库）、数字+单位、名称模糊匹配；回车即解析。
      </p>
    </div>

    <!-- 待核对单据卡片（规则 4） -->
    <div
      v-if="parsed"
      class="mt-3 rounded-xl border border-emerald-200 bg-emerald-50/60 p-4"
    >
      <div class="flex items-center justify-between">
        <h2 class="text-sm font-semibold text-emerald-800">待核对单据</h2>
        <button
          type="button"
          class="text-xs text-slate-500 hover:text-slate-700"
          @click="parsed = null"
        >
          放弃
        </button>
      </div>

      <p v-if="parsed.warnings.length > 0" class="mt-1.5 text-xs text-amber-600">
        {{ parsed.warnings.join("；") }}
      </p>

      <div class="mt-2 space-y-3">
        <!-- 方向（解析结果可改） -->
        <div class="grid grid-cols-2 gap-2">
          <button
            type="button"
            class="rounded-lg border px-3 py-2 text-xs font-semibold transition-colors"
            :class="
              cardType === 'IN'
                ? 'border-emerald-500 bg-white text-emerald-700'
                : 'border-slate-200 bg-transparent text-slate-500'
            "
            @click="cardType = 'IN'"
          >
            入库
          </button>
          <button
            type="button"
            class="rounded-lg border px-3 py-2 text-xs font-semibold transition-colors"
            :class="
              cardType === 'OUT'
                ? 'border-amber-500 bg-white text-amber-700'
                : 'border-slate-200 bg-transparent text-slate-500'
            "
            @click="cardType = 'OUT'"
          >
            出库
          </button>
        </div>

        <!-- 货物：唯一命中已带出；多候选 chips；零命中 [立即建档][改为备注] -->
        <div>
          <label class="mb-1 block text-xs font-medium text-slate-500">货物 *</label>
          <div
            v-if="parsed.itemCandidates.length > 1"
            class="mb-1.5 flex flex-wrap gap-1.5"
          >
            <button
              v-for="it in parsed.itemCandidates"
              :key="it.id"
              type="button"
              class="rounded-full border px-2.5 py-1 text-xs transition-colors"
              :class="
                cardItemId === it.id
                  ? 'border-emerald-500 bg-emerald-600 text-white'
                  : 'border-slate-300 bg-white text-slate-600 hover:border-emerald-400'
              "
              @click="cardItemId = it.id"
            >
              {{ it.name }}
            </button>
          </div>
          <SelectPicker
            v-model="cardItemId"
            :options="itemOptions"
            placeholder="选择货物（可搜索）"
            empty-text="暂无货物，请先建档"
          />
          <div
            v-if="parsed.itemCandidates.length === 0 && !noItemAcked"
            class="mt-1.5 flex items-center gap-2 text-xs"
          >
            <span class="text-slate-500">未匹配到货物：</span>
            <button
              type="button"
              class="rounded border border-emerald-300 bg-white px-2 py-0.5 text-emerald-700 hover:bg-emerald-50"
              @click="router.push('/items')"
            >
              立即建档
            </button>
            <button
              type="button"
              class="rounded border border-slate-300 bg-white px-2 py-0.5 text-slate-600 hover:bg-slate-50"
              @click="noItemAcked = true"
            >
              改为备注
            </button>
          </div>
        </div>

        <!-- 数量（解析结果可改，提交按单位校验精度） -->
        <div>
          <label class="mb-1 block text-xs font-medium text-slate-500">数量 *</label>
          <div class="flex items-center gap-2">
            <input
              v-model="cardQuantity"
              type="text"
              inputmode="decimal"
              placeholder="0"
              class="w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm text-slate-800 outline-none focus:border-emerald-400"
            />
            <span class="shrink-0 text-sm text-slate-500">
              {{ selectedItem?.unit ?? "单位" }}
            </span>
          </div>
        </div>

        <!-- 往来单位 / 操作人 -->
        <div class="grid grid-cols-1 gap-3 md:grid-cols-2">
          <div>
            <label class="mb-1 block text-xs font-medium text-slate-500">往来单位（可空）</label>
            <SelectPicker
              v-model="cardPartnerId"
              :options="partnerOptions"
              placeholder="选择往来单位"
              empty-text="暂无往来单位"
            />
            <p
              v-if="parsed.partnerTypeHint"
              class="mt-1 text-[11px] text-slate-400"
            >
              识别为{{ parsed.partnerTypeHint === "customer" ? "客户" : "供应商" }}类，仅作提示
            </p>
          </div>
          <div>
            <label class="mb-1 block text-xs font-medium text-slate-500">操作人 *</label>
            <SelectPicker
              v-model="cardOperatorId"
              :options="operatorOptions"
              placeholder="选择操作人"
              empty-text="暂无操作人"
            />
          </div>
        </div>

        <!-- 备注（默认携带零命中残留文本） -->
        <div>
          <label class="mb-1 block text-xs font-medium text-slate-500">备注</label>
          <input
            v-model="cardRemark"
            type="text"
            maxlength="200"
            placeholder="备注（未匹配文本已自动填入）"
            class="w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm text-slate-800 outline-none focus:border-emerald-400"
          />
        </div>

        <p v-if="fieldError" class="text-xs text-red-600">{{ fieldError }}</p>

        <button
          type="button"
          class="w-full rounded-lg bg-emerald-600 px-4 py-2.5 text-sm font-semibold text-white hover:bg-emerald-700 disabled:opacity-60"
          :disabled="submitting"
          @click="submitCard"
        >
          {{ submitting ? "提交中…" : "确认提交" }}
        </button>
      </div>
    </div>

    <button
      type="button"
      class="mt-3 w-full rounded-xl bg-emerald-600 px-4 py-3 text-sm font-semibold text-white shadow-sm transition-colors hover:bg-emerald-700"
      @click="router.push('/entry')"
    >
      ＋ 记一笔
    </button>

    <!-- 今日进出摘要 -->
    <div class="mt-5 grid grid-cols-3 gap-3">
      <div class="rounded-xl border border-slate-200 bg-white p-3 text-center">
        <div class="text-lg font-semibold tabular-nums text-emerald-700">
          {{ summaryError ? "—" : todayInCount }}
        </div>
        <div class="mt-0.5 text-xs text-slate-500">今日入库（笔）</div>
      </div>
      <div class="rounded-xl border border-slate-200 bg-white p-3 text-center">
        <div class="text-lg font-semibold tabular-nums text-amber-700">
          {{ summaryError ? "—" : todayOutCount }}
        </div>
        <div class="mt-0.5 text-xs text-slate-500">今日出库（笔）</div>
      </div>
      <button
        type="button"
        class="rounded-xl border p-3 text-center transition-colors"
        :class="
          negativeCount > 0
            ? 'border-red-200 bg-red-50 hover:bg-red-100'
            : 'border-slate-200 bg-white hover:bg-slate-50'
        "
        @click="router.push('/inventory')"
      >
        <div
          class="text-lg font-semibold tabular-nums"
          :class="negativeCount > 0 ? 'text-red-600' : 'text-slate-600'"
        >
          {{ inventory.error ? "—" : negativeCount }}
        </div>
        <div class="mt-0.5 flex items-center justify-center gap-1 text-xs text-slate-500">
          负库存（项）
          <RedHintBadge v-if="negativeCount > 0">标红</RedHintBadge>
        </div>
      </button>
    </div>

    <!-- 最近流水 -->
    <div class="mt-6">
      <div class="mb-2 flex items-center justify-between">
        <h2 class="text-sm font-semibold text-slate-700">最近流水</h2>
        <button
          type="button"
          class="text-xs text-emerald-600 hover:underline"
          @click="router.push('/transactions')"
        >
          查看全部 →
        </button>
      </div>

      <div v-if="txStore.loading && txStore.recent.length === 0" class="py-6 text-center text-sm text-slate-400">
        加载中…
      </div>
      <EmptyState
        v-else-if="txStore.error"
        title="流水加载失败"
        :description="txStore.error"
      />
      <EmptyState
        v-else-if="txStore.recent.length === 0"
        title="还没有流水"
        description="点击上方「记一笔」，记录第一笔进出。"
      />
      <div v-else class="space-y-2">
        <TxCard
          v-for="tx in txStore.recent"
          :key="tx.id"
          :tx="tx"
          :voided="recentVoided.has(tx.id)"
        />
      </div>
    </div>
  </section>
</template>
