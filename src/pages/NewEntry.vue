<script setup lang="ts">
/**
 * ② 记一笔（契约第 6 章）：
 * - IN/OUT 切换、货物/往来/操作人选择器（模糊搜索）、按单位控精度的数量输入
 * - 业务时间 datetime-local（本地默认现在，提交转 UTC，契约 7.3）
 * - 复制上次单据预填（copiedFrom 溯源）；提交后刷新库存，负库存红字即时提示（7.4）
 */
import { computed, onMounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { createTransaction } from "../api/commands";
import type { NewTransaction, Transaction, TxType } from "../api/types";
import { PARTNER_TYPE_LABELS } from "../api/types";
import SelectPicker from "../components/SelectPicker.vue";
import { useInventoryStore } from "../stores/inventory";
import { useItemsStore } from "../stores/items";
import { usePartnersStore } from "../stores/partners";
import { useSessionStore } from "../stores/session";
import { useTransactionsStore } from "../stores/transactions";
import {
  formatQty,
  isIntegerUnit,
  localInputNow,
  toUtcIso,
  validateQuantity,
} from "../utils/format";
import { toast, toastError, toastSuccess } from "../utils/toast";

const router = useRouter();
const itemsStore = useItemsStore();
const partnersStore = usePartnersStore();
const session = useSessionStore();
const inventory = useInventoryStore();
const txStore = useTransactionsStore();

const txType = ref<TxType>("IN");
const itemId = ref<string | null>(null);
const quantity = ref("");
const partnerId = ref<string | null>(null);
const operatorId = ref<string | null>(null);
const remark = ref("");
const happenedAt = ref(localInputNow());
const copiedFrom = ref<string | null>(null);
const copyHint = ref<string | null>(null);

const itemError = ref("");
const operatorError = ref("");
const qtyError = ref("");
const submitting = ref(false);

const showNewOperator = ref(false);
const newOperatorName = ref("");

const selectedItem = computed(
  () => itemsStore.list.find((i) => i.id === itemId.value) ?? null,
);

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
    hint: PARTNER_TYPE_LABELS[p.type],
  })),
);

const operatorOptions = computed(() =>
  session.operators.map((o) => ({ id: o.id, label: o.name })),
);

const quantityHint = computed(() => {
  const unit = selectedItem.value?.unit;
  if (!unit) return "选择货物后按单位校验精度";
  return isIntegerUnit(unit)
    ? `单位「${unit}」：只允许整数`
    : `单位「${unit}」：最多两位小数`;
});

/** 记住操作人选择（会话内预选）。 */
watch(operatorId, (v) => {
  if (v) session.currentOperatorId = v;
});

function applyPrefill(src: Transaction): void {
  if (src.type === "IN" || src.type === "OUT") txType.value = src.type;
  itemId.value = src.itemId;
  quantity.value = String(src.quantity);
  partnerId.value = src.partnerId;
  operatorId.value = src.operatorId;
  remark.value = src.remark ?? "";
  copiedFrom.value = src.id;
  copyHint.value = `已复制单据：${src.itemName} · ${formatQty(src.quantity)}${src.unit}（记录 copiedFrom 溯源）`;
}

function validate(): boolean {
  itemError.value = itemId.value ? "" : "请选择货物";
  operatorError.value = operatorId.value ? "" : "请选择操作人";
  qtyError.value =
    validateQuantity(quantity.value, selectedItem.value?.unit ?? "") ?? "";
  return itemError.value === "" && operatorError.value === "" && qtyError.value === "";
}

async function submit(): Promise<void> {
  if (!validate() || submitting.value) return;
  submitting.value = true;
  const input: NewTransaction = {
    type: txType.value,
    itemId: itemId.value as string,
    quantity: Number(quantity.value),
    partnerId: partnerId.value,
    operatorId: operatorId.value as string,
    remark: remark.value.trim() || null,
    happenedAt: toUtcIso(happenedAt.value),
    copiedFrom: copiedFrom.value,
  };
  try {
    const tx = await createTransaction(input);
    toastSuccess(
      `已记一笔：${tx.itemName} ${tx.type === "IN" ? "入库" : "出库"} ${formatQty(tx.quantity)}${tx.unit}`,
    );
    // 契约 7.4：提交后刷新库存，余量为负红字即时提示（不拦截）
    await inventory.fetch();
    const row = inventory.rowOf(tx.itemId);
    if (row && row.negative) {
      toast(`已提交，当前余量 ${formatQty(row.balance)}（低于 0）`, "error");
    }
    // 保留货物/往来/操作人便于连续记账，清空数量与备注
    quantity.value = "";
    remark.value = "";
    happenedAt.value = localInputNow();
    copiedFrom.value = null;
    copyHint.value = null;
    qtyError.value = "";
  } catch (e) {
    toastError(e);
  } finally {
    submitting.value = false;
  }
}

async function addOperator(): Promise<void> {
  const name = newOperatorName.value.trim();
  if (!name) {
    toast("请输入操作人姓名", "error");
    return;
  }
  try {
    const op = await session.addOperator(name);
    operatorId.value = op.id;
    operatorError.value = "";
    newOperatorName.value = "";
    showNewOperator.value = false;
    toastSuccess(`已创建操作人「${op.name}」`);
  } catch (e) {
    toastError(e);
  }
}

onMounted(() => {
  const src = txStore.takePrefill();
  if (src) applyPrefill(src);

  itemsStore.fetch().catch(toastError);
  partnersStore.fetch().catch(toastError);
  session
    .loadOperators()
    .then(() => {
      if (!operatorId.value) {
        const remembered =
          session.currentOperatorId &&
          session.operators.some((o) => o.id === session.currentOperatorId)
            ? session.currentOperatorId
            : null;
        operatorId.value = remembered ?? session.operators[0]?.id ?? null;
      }
      if (session.operators.length === 0) showNewOperator.value = true;
    })
    .catch(toastError);
});
</script>

<template>
  <section class="mx-auto w-full max-w-3xl p-4 md:p-6">
    <h1 class="text-xl font-bold text-slate-800">记一笔</h1>

    <!-- 复制预填提示 -->
    <p
      v-if="copyHint"
      class="mt-3 rounded-lg border border-sky-200 bg-sky-50 px-3 py-2 text-xs text-sky-700"
    >
      {{ copyHint }}
    </p>

    <form class="mt-4 space-y-4" @submit.prevent="submit">
      <!-- 方向切换 -->
      <div class="grid grid-cols-2 gap-2">
        <button
          type="button"
          class="rounded-xl border px-4 py-3 text-sm font-semibold transition-colors"
          :class="
            txType === 'IN'
              ? 'border-emerald-500 bg-emerald-50 text-emerald-700'
              : 'border-slate-200 bg-white text-slate-500 hover:bg-slate-50'
          "
          @click="txType = 'IN'"
        >
          入库 IN
        </button>
        <button
          type="button"
          class="rounded-xl border px-4 py-3 text-sm font-semibold transition-colors"
          :class="
            txType === 'OUT'
              ? 'border-amber-500 bg-amber-50 text-amber-700'
              : 'border-slate-200 bg-white text-slate-500 hover:bg-slate-50'
          "
          @click="txType = 'OUT'"
        >
          出库 OUT
        </button>
      </div>

      <!-- 货物 -->
      <div>
        <div class="mb-1 flex items-center justify-between">
          <label class="text-xs font-medium text-slate-500">货物 *</label>
          <button
            type="button"
            class="text-xs text-emerald-600 hover:underline"
            @click="router.push({ path: '/items', query: { from: 'entry' } })"
          >
            去建新货物 →
          </button>
        </div>
        <SelectPicker
          v-model="itemId"
          :options="itemOptions"
          placeholder="选择货物（可搜索）"
          empty-text="暂无货物，请先建新货物"
        />
        <p v-if="itemError" class="mt-1 text-xs text-red-600">{{ itemError }}</p>
      </div>

      <!-- 数量 -->
      <div>
        <label class="mb-1 block text-xs font-medium text-slate-500">数量 *</label>
        <div class="flex items-center gap-2">
          <input
            v-model="quantity"
            type="text"
            inputmode="decimal"
            placeholder="0"
            class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm text-slate-800 outline-none focus:border-emerald-400"
            @blur="qtyError = validateQuantity(quantity, selectedItem?.unit ?? '') ?? ''"
          />
          <span class="shrink-0 text-sm text-slate-500">
            {{ selectedItem?.unit ?? "单位" }}
          </span>
        </div>
        <p class="mt-1 text-xs" :class="qtyError ? 'text-red-600' : 'text-slate-400'">
          {{ qtyError || quantityHint }}
        </p>
      </div>

      <!-- 往来单位（可空） -->
      <div>
        <label class="mb-1 block text-xs font-medium text-slate-500">往来单位（可空）</label>
        <SelectPicker
          v-model="partnerId"
          :options="partnerOptions"
          placeholder="选择往来单位（可搜索，可不填）"
          empty-text="暂无往来单位"
        />
      </div>

      <!-- 操作人 -->
      <div>
        <div class="mb-1 flex items-center justify-between">
          <label class="text-xs font-medium text-slate-500">操作人 *</label>
          <button
            type="button"
            class="text-xs text-emerald-600 hover:underline"
            @click="showNewOperator = !showNewOperator"
          >
            ＋ 新增操作人
          </button>
        </div>
        <SelectPicker
          v-model="operatorId"
          :options="operatorOptions"
          placeholder="选择操作人"
          :clearable="false"
          empty-text="暂无操作人"
        />
        <p v-if="operatorError" class="mt-1 text-xs text-red-600">{{ operatorError }}</p>

        <div
          v-if="session.operators.length === 0 && !showNewOperator"
          class="mt-1 text-xs text-amber-600"
        >
          还没有操作人，请先点右上角「＋ 新增操作人」创建一位。
        </div>

        <div v-if="showNewOperator" class="mt-2 flex gap-2">
          <input
            v-model="newOperatorName"
            type="text"
            placeholder="操作人姓名（全局唯一）"
            class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm outline-none focus:border-emerald-400"
            @keyup.enter="addOperator"
          />
          <button
            type="button"
            class="shrink-0 rounded-lg bg-emerald-600 px-4 py-2 text-sm font-medium text-white hover:bg-emerald-700"
            @click="addOperator"
          >
            创建
          </button>
        </div>
      </div>

      <!-- 业务时间 -->
      <div>
        <label class="mb-1 block text-xs font-medium text-slate-500">业务时间</label>
        <input
          v-model="happenedAt"
          type="datetime-local"
          class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm text-slate-800 outline-none focus:border-emerald-400"
        />
        <p class="mt-1 text-xs text-slate-400">默认当前时间；按本地时间填写，存库自动转 UTC。</p>
      </div>

      <!-- 备注 -->
      <div>
        <label class="mb-1 block text-xs font-medium text-slate-500">备注</label>
        <input
          v-model="remark"
          type="text"
          maxlength="200"
          placeholder="选填，如单号/说明"
          class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm text-slate-800 outline-none focus:border-emerald-400"
        />
      </div>

      <button
        type="submit"
        class="w-full rounded-xl bg-emerald-600 px-4 py-3 text-sm font-semibold text-white shadow-sm transition-colors hover:bg-emerald-700 disabled:cursor-not-allowed disabled:opacity-60"
        :disabled="submitting"
      >
        {{ submitting ? "提交中…" : txType === "IN" ? "确认入库" : "确认出库" }}
      </button>
      <p class="text-center text-xs text-slate-400">
        出库允许负库存：提交不拦截，库存总览标红提示（D2）。
      </p>
    </form>
  </section>
</template>
