/**
 * 库存 store：v_inventory 视图数据（契约第 2 章 / 4.5）。
 * negative 字段由 Rust 侧计算（契约 v1.1 双向回冲已修正），前端直接消费标红。
 */
import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { getInventory, toCommandError } from "../api/commands";
import type { InventoryRow } from "../api/types";

export const useInventoryStore = defineStore("inventory", () => {
  const rows = ref<InventoryRow[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function fetch(force = true): Promise<InventoryRow[]> {
    if (!force && rows.value.length > 0) return rows.value;
    loading.value = true;
    error.value = null;
    try {
      rows.value = await getInventory();
      return rows.value;
    } catch (e) {
      error.value = toCommandError(e).message;
      throw e;
    } finally {
      loading.value = false;
    }
  }

  /** 负库存行（D2 标红）。 */
  const negativeRows = computed(() => rows.value.filter((r) => r.negative));

  /** 按货物取库存行（记一笔提交后查余量）。 */
  function rowOf(itemId: string): InventoryRow | null {
    return rows.value.find((r) => r.itemId === itemId) ?? null;
  }

  return { rows, loading, error, fetch, negativeRows, rowOf };
});
