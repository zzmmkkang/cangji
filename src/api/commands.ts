/**
 * invoke 封装（契约 4.0）：从 `@tauri-apps/api/core` 导入（v2 路径，禁用 v1 写法）；
 * 每条命令一个薄封装函数，参数 camelCase（顶层参数由 Tauri 自动转 snake_case）；
 * 统一 try/catch 抛出结构化 CommandError { code, message, detail }。
 *
 * A4 起封装全部 25 条命令 = 契约 4.8 的 24 条 + 新增 list_merge_logs（契约偏离，
 * 合并日志展示所需，已在 Rust generate_handler 与本文件标注）。
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  AppInfo,
  ExportedFile,
  ImportReport,
  InventoryRow,
  Item,
  MergeLogEntry,
  NewItem,
  NewPartner,
  NewTransaction,
  Operator,
  PackSummary,
  Partner,
  Setting,
  StatsQuery,
  StatsRow,
  Transaction,
  TransactionFilter,
  TransactionPage,
  UpdateItem,
  UpdatePartner,
} from "./types";

/**
 * 结构化命令错误（契约 4.0/7.5）：Rust CommandError 序列化后的 { code, message, detail }。
 * 页面层统一 `catch (e) { toastError(e) }`，toast 读 message。
 */
export class CommandError extends Error {
  readonly code: string;
  readonly detail?: unknown;

  constructor(code: string, message: string, detail?: unknown) {
    super(message);
    this.name = "CommandError";
    this.code = code;
    this.detail = detail;
  }
}

/** 是否运行在 Tauri 宿主内（否则 invoke 必失败，给出可读提示避免白屏/不明报错）。 */
function isTauriEnv(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** 把任意 catch 到的值规范化为 CommandError。 */
export function toCommandError(e: unknown): CommandError {
  if (e instanceof CommandError) return e;
  if (e && typeof e === "object" && "code" in e && "message" in e) {
    const o = e as { code?: unknown; message?: unknown; detail?: unknown };
    if (typeof o.code === "string" && typeof o.message === "string") {
      return new CommandError(o.code, o.message, o.detail);
    }
  }
  if (!isTauriEnv()) {
    return new CommandError(
      "E_INTERNAL",
      "非 Tauri 运行环境，后端命令不可用（浏览器预览模式）",
    );
  }
  if (e instanceof Error) return new CommandError("E_INTERNAL", e.message);
  if (typeof e === "string") return new CommandError("E_INTERNAL", e);
  return new CommandError("E_INTERNAL", "未知错误");
}

/** 统一调用入口：捕获 invoke 错误并规范化为 CommandError。 */
async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw toCommandError(e);
  }
}

// ============ 应用信息 / 设置域（契约 4.1） ============

export const getAppInfo = () => call<AppInfo>("get_app_info");
export const getSettings = () => call<Setting[]>("get_settings");
export const setSetting = (key: string, value: string) =>
  call<void>("set_setting", { key, value });

// ============ 货物域（契约 4.2） ============

export const listItems = (keyword?: string, includeArchived?: boolean) =>
  call<Item[]>("list_items", { keyword, includeArchived });
export const createItem = (input: NewItem) => call<Item>("create_item", { input });
export const updateItem = (id: string, input: UpdateItem) =>
  call<Item>("update_item", { id, input });
export const setItemArchived = (id: string, archived: boolean) =>
  call<Item>("set_item_archived", { id, archived });

// ============ 往来单位域（契约 4.3） ============

export const listPartners = (keyword?: string, includeArchived?: boolean) =>
  call<Partner[]>("list_partners", { keyword, includeArchived });
export const createPartner = (input: NewPartner) =>
  call<Partner>("create_partner", { input });
export const updatePartner = (id: string, input: UpdatePartner) =>
  call<Partner>("update_partner", { id, input });
export const setPartnerArchived = (id: string, archived: boolean) =>
  call<Partner>("set_partner_archived", { id, archived });

// ============ 操作人域（契约 4.3） ============

export const listOperators = () => call<Operator[]>("list_operators");
export const createOperator = (name: string) =>
  call<Operator>("create_operator", { name });
export const updateOperator = (id: string, name: string) =>
  call<Operator>("update_operator", { id, name });

// ============ 流水域（契约 4.4，append-only） ============

export const createTransaction = (input: NewTransaction) =>
  call<Transaction>("create_transaction", { input });
export const voidTransaction = (id: string, reason?: string) =>
  call<Transaction>("void_transaction", { id, reason });
export const listTransactions = (filter: TransactionFilter = {}) =>
  call<TransactionPage>("list_transactions", { filter });

// ============ 库存域（契约 4.5） ============

export const getInventory = () => call<InventoryRow[]>("get_inventory");

// ============ 统计域（契约 4.6） ============

export const getStats = (query: StatsQuery) =>
  call<StatsRow[]>("get_stats", { query });
export const exportStatsExcel = (query: StatsQuery, targetPath?: string) =>
  call<ExportedFile>("export_stats_excel", { query, targetPath });

// ============ 数据包 / 备份域（契约 4.7，A4 交付） ============

export const exportDataPack = (targetPath?: string) =>
  call<ExportedFile>("export_data_pack", { targetPath });
export const inspectDataPack = (sourcePath: string) =>
  call<PackSummary>("inspect_data_pack", { sourcePath });
export const importDataPack = (sourcePath: string) =>
  call<ImportReport>("import_data_pack", { sourcePath });
export const backupDatabase = (targetPath?: string) =>
  call<ExportedFile>("backup_database", { targetPath });

/**
 * 合并日志列表（【契约偏离】第 25 条命令，契约 4.8 核对单之外）：
 * 按 imported_at 倒序返回 merge_logs 审计行，供数据共账页合并日志区展示。
 */
export const listMergeLogs = () => call<MergeLogEntry[]>("list_merge_logs");
