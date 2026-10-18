/**
 * 货物档案 store：列表缓存 + 模糊搜索（契约第 2 章）。
 * 缓存恒含已归档（includeArchived=true），选择器场景用 active 过滤。
 */
import { computed, ref } from "vue";
import { defineStore } from "pinia";
import {
  createItem,
  listItems,
  setItemArchived,
  toCommandError,
  updateItem,
} from "../api/commands";
import type { Item, NewItem, UpdateItem } from "../api/types";

export const useItemsStore = defineStore("items", () => {
  const list = ref<Item[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function fetch(force = false): Promise<Item[]> {
    if (!force && list.value.length > 0) return list.value;
    loading.value = true;
    error.value = null;
    try {
      list.value = await listItems(undefined, true);
      return list.value;
    } catch (e) {
      error.value = toCommandError(e).message;
      throw e;
    } finally {
      loading.value = false;
    }
  }

  /** 未归档货物（选择器默认展示）。 */
  const active = computed(() => list.value.filter((i) => !i.isArchived));

  /** 本地模糊搜索（名称 / 编码 / 规格，忽略大小写），默认只搜未归档。 */
  function search(keyword: string, includeArchived = false): Item[] {
    const q = keyword.trim().toLowerCase();
    const base = includeArchived ? list.value : active.value;
    if (!q) return base;
    return base.filter(
      (i) =>
        i.name.toLowerCase().includes(q) ||
        (i.code ?? "").toLowerCase().includes(q) ||
        (i.spec ?? "").toLowerCase().includes(q),
    );
  }

  /** 同名档案检查（A2 约定：后端不阻断，仅前端软提示）。 */
  function hasSameName(name: string, exceptId?: string): boolean {
    const n = name.trim();
    return list.value.some((i) => i.id !== exceptId && i.name.trim() === n);
  }

  async function create(input: NewItem): Promise<Item> {
    const item = await createItem(input);
    await fetch(true);
    return item;
  }

  async function update(id: string, input: UpdateItem): Promise<Item> {
    const item = await updateItem(id, input);
    await fetch(true);
    return item;
  }

  async function setArchived(id: string, archived: boolean): Promise<Item> {
    const item = await setItemArchived(id, archived);
    await fetch(true);
    return item;
  }

  return { list, loading, error, fetch, active, search, hasSameName, create, update, setArchived };
});
