/**
 * 会话 store：应用信息（版本/设备/DB 路径）、操作人列表、当前操作人选择。
 * 薄封装：数据获取与缓存，不承载业务逻辑（契约第 2 章）。
 */
import { computed, ref } from "vue";
import { defineStore } from "pinia";
import {
  createOperator,
  getAppInfo,
  listOperators,
  setSetting,
  toCommandError,
  updateOperator,
} from "../api/commands";
import type { AppInfo, Operator } from "../api/types";

export const useSessionStore = defineStore("session", () => {
  const appInfo = ref<AppInfo | null>(null);
  const operators = ref<Operator[]>([]);
  /** 当前操作人（记一笔页预选；仅会话内记忆，不落库） */
  const currentOperatorId = ref<string | null>(null);
  const appInfoError = ref<string | null>(null);
  const operatorsError = ref<string | null>(null);

  async function loadAppInfo(): Promise<AppInfo> {
    appInfoError.value = null;
    try {
      appInfo.value = await getAppInfo();
      return appInfo.value;
    } catch (e) {
      appInfoError.value = toCommandError(e).message;
      throw e;
    }
  }

  async function loadOperators(): Promise<Operator[]> {
    operatorsError.value = null;
    try {
      operators.value = await listOperators();
      return operators.value;
    } catch (e) {
      operatorsError.value = toCommandError(e).message;
      throw e;
    }
  }

  /** 新增操作人（重名 → E_DUPLICATE，由调用方 toast）；成功后自动选中。 */
  async function addOperator(name: string): Promise<Operator> {
    const op = await createOperator(name.trim());
    await loadOperators();
    currentOperatorId.value = op.id;
    return op;
  }

  /** 操作人改名（改名入口在数据共账页，契约 4.3）。 */
  async function renameOperator(id: string, name: string): Promise<Operator> {
    const op = await updateOperator(id, name.trim());
    await loadOperators();
    return op;
  }

  /** 修改设备名（settings.device_name，契约 7.2）。 */
  async function saveDeviceName(name: string): Promise<void> {
    await setSetting("device_name", name.trim());
    await loadAppInfo();
  }

  const currentOperator = computed(
    () => operators.value.find((o) => o.id === currentOperatorId.value) ?? null,
  );

  return {
    appInfo,
    operators,
    currentOperatorId,
    currentOperator,
    appInfoError,
    operatorsError,
    loadAppInfo,
    loadOperators,
    addOperator,
    renameOperator,
    saveDeviceName,
  };
});
