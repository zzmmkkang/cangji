/**
 * 流水 store：筛选状态 + 分页列表 + 最近流水 + 复制预填（契约第 2 章）。
 * 筛选空值用 "" 表示"未指定"，提交前映射为 undefined（契约 4.4 可选字段）。
 */
import { computed, reactive, ref } from "vue";
import { defineStore } from "pinia";
import { listTransactions, toCommandError } from "../api/commands";
import type {
  Transaction,
  TransactionFilter,
  TransactionPage,
  TxType,
} from "../api/types";
import { dayEndUtc, dayStartUtc } from "../utils/format";

/** 契约 4.4：默认 100，后端钳制 1~500 */
export const PAGE_LIMIT = 100;

/** 筛选表单状态（"" = 未指定；日期为本地 YYYY-MM-DD）。 */
export interface FilterState {
  type: TxType | "";
  itemId: string;
  partnerId: string;
  operatorId: string;
  dateFrom: string;
  dateTo: string;
  keyword: string;
}

export function emptyFilter(): FilterState {
  return {
    type: "",
    itemId: "",
    partnerId: "",
    operatorId: "",
    dateFrom: "",
    dateTo: "",
    keyword: "",
  };
}

/** 由一组流水推导"已被作废的原单 id"集合（VOID 行的 refId）。 */
export function computeVoidedIds(rows: Transaction[]): Set<string> {
  const ids = new Set<string>();
  for (const r of rows) {
    if (r.type === "VOID" && r.refId) ids.add(r.refId);
  }
  return ids;
}

export const useTransactionsStore = defineStore("transactions", () => {
  const filter = reactive<FilterState>(emptyFilter());
  const offset = ref(0);
  const page = ref<TransactionPage | null>(null);
  const recent = ref<Transaction[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);
  /** 复制上次单据的预填数据（流水页写入，记一笔页取走） */
  const prefill = ref<Transaction | null>(null);

  /** FilterState → IPC TransactionFilter（空值置空、本地日期段转 UTC）。 */
  function toFilter(): TransactionFilter {
    const f: TransactionFilter = { limit: PAGE_LIMIT, offset: offset.value };
    if (filter.type) f.type = filter.type;
    if (filter.itemId) f.itemId = filter.itemId;
    if (filter.partnerId) f.partnerId = filter.partnerId;
    if (filter.operatorId) f.operatorId = filter.operatorId;
    const kw = filter.keyword.trim();
    if (kw) f.keyword = kw;
    if (filter.dateFrom) f.dateFrom = dayStartUtc(filter.dateFrom);
    if (filter.dateTo) f.dateTo = dayEndUtc(filter.dateTo);
    return f;
  }

  async function fetchPage(): Promise<TransactionPage> {
    loading.value = true;
    error.value = null;
    try {
      page.value = await listTransactions(toFilter());
      return page.value;
    } catch (e) {
      error.value = toCommandError(e).message;
      throw e;
    } finally {
      loading.value = false;
    }
  }

  /** 重置筛选并回第一页。 */
  function resetFilter(): void {
    Object.assign(filter, emptyFilter());
    offset.value = 0;
  }

  /** 速记主页最近流水（契约 4.4：limit 5）。 */
  async function fetchRecent(limit = 5): Promise<Transaction[]> {
    loading.value = true;
    error.value = null;
    try {
      const p = await listTransactions({ limit });
      recent.value = p.rows;
      return recent.value;
    } catch (e) {
      error.value = toCommandError(e).message;
      throw e;
    } finally {
      loading.value = false;
    }
  }

  /** 当前页中已被作废的原单 id 集合。 */
  const voidedIds = computed(() =>
    computeVoidedIds(page.value?.rows ?? []),
  );

  function setPrefill(tx: Transaction): void {
    prefill.value = tx;
  }

  /** 记一笔页挂载时取走预填（一次性）。 */
  function takePrefill(): Transaction | null {
    const tx = prefill.value;
    prefill.value = null;
    return tx;
  }

  return {
    filter,
    offset,
    page,
    recent,
    loading,
    error,
    prefill,
    toFilter,
    fetchPage,
    resetFilter,
    fetchRecent,
    voidedIds,
    setPrefill,
    takePrefill,
  };
});
