/**
 * 往来单位 store：档案缓存 + 模糊搜索（契约第 2 章，与 items 同构）。
 */
import { computed, ref } from "vue";
import { defineStore } from "pinia";
import {
  createPartner,
  listPartners,
  setPartnerArchived,
  toCommandError,
  updatePartner,
} from "../api/commands";
import type { NewPartner, Partner, UpdatePartner } from "../api/types";

export const usePartnersStore = defineStore("partners", () => {
  const list = ref<Partner[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function fetch(force = false): Promise<Partner[]> {
    if (!force && list.value.length > 0) return list.value;
    loading.value = true;
    error.value = null;
    try {
      list.value = await listPartners(undefined, true);
      return list.value;
    } catch (e) {
      error.value = toCommandError(e).message;
      throw e;
    } finally {
      loading.value = false;
    }
  }

  /** 未归档往来单位。 */
  const active = computed(() => list.value.filter((p) => !p.isArchived));

  /** 本地模糊搜索（名称 / 联系人 / 备注）。 */
  function search(keyword: string, includeArchived = false): Partner[] {
    const q = keyword.trim().toLowerCase();
    const base = includeArchived ? list.value : active.value;
    if (!q) return base;
    return base.filter(
      (p) =>
        p.name.toLowerCase().includes(q) ||
        (p.contact ?? "").toLowerCase().includes(q) ||
        (p.note ?? "").toLowerCase().includes(q),
    );
  }

  /** 同名软提示检查（A2 约定不阻断）。 */
  function hasSameName(name: string, exceptId?: string): boolean {
    const n = name.trim();
    return list.value.some((p) => p.id !== exceptId && p.name.trim() === n);
  }

  async function create(input: NewPartner): Promise<Partner> {
    const partner = await createPartner(input);
    await fetch(true);
    return partner;
  }

  async function update(id: string, input: UpdatePartner): Promise<Partner> {
    const partner = await updatePartner(id, input);
    await fetch(true);
    return partner;
  }

  async function setArchived(id: string, archived: boolean): Promise<Partner> {
    const partner = await setPartnerArchived(id, archived);
    await fetch(true);
    return partner;
  }

  return { list, loading, error, fetch, active, search, hasSameName, create, update, setArchived };
});
