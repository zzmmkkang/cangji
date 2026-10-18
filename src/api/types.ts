/**
 * IPC 类型定义（契约第 4 章）——与 src-tauri/src/commands/*.rs 的 Rust DTO 一一对应。
 * Rust 侧统一 `#[serde(rename_all = "camelCase")]`，序列化后的 JSON 字段均为 camelCase；
 * TxType 序列化为 "IN" | "OUT" | "VOID"（serde rename_all = "UPPERCASE"）。
 * 结构化错误 { code, message, detail } 的承载类型见 api/commands.ts 的 CommandError 类。
 */

/** 应用信息（契约 4.1 AppInfo）。 */
export interface AppInfo {
  appVersion: string;
  schemaVersion: number;
  deviceId: string;
  deviceName: string;
  dbPath: string;
  /** merge_logs 最大 imported_at；null = 从未同步 */
  lastSyncAt: string | null;
}

/** 设置键值（契约 4.1 Setting）。 */
export interface Setting {
  key: string;
  value: string;
  updatedAt: string;
}

/** 货物档案（契约 4.2 Item）。 */
export interface Item {
  id: string;
  name: string;
  code: string | null;
  spec: string | null;
  unit: string;
  note: string | null;
  isArchived: boolean;
  createdAt: string;
  updatedAt: string;
}

/** 新建货物（契约 4.2 NewItem）。 */
export interface NewItem {
  name: string;
  code?: string | null;
  spec?: string | null;
  unit: string;
  note?: string | null;
}

/**
 * 更新货物（契约 4.2 UpdateItem）：None/省略 = 不修改；
 * 可空字段传 "" = 置 NULL（A2 交接要点）。
 */
export interface UpdateItem {
  name?: string | null;
  code?: string | null;
  spec?: string | null;
  unit?: string | null;
  note?: string | null;
}

/** 往来单位类型（契约 4.3）。 */
export type PartnerType = "supplier" | "customer" | "both";

/** 往来单位类型中文标签（前端展示用）。 */
export const PARTNER_TYPE_LABELS: Record<PartnerType, string> = {
  supplier: "供应商",
  customer: "客户",
  both: "两者皆是",
};

/** 往来单位（契约 4.3 Partner）。 */
export interface Partner {
  id: string;
  name: string;
  type: PartnerType;
  contact: string | null;
  note: string | null;
  isArchived: boolean;
  createdAt: string;
  updatedAt: string;
}

/** 新建往来单位。 */
export interface NewPartner {
  name: string;
  type: PartnerType;
  contact?: string | null;
  note?: string | null;
}

/** 更新往来单位（"" = 置 NULL，省略 = 不改）。 */
export interface UpdatePartner {
  name?: string | null;
  type?: PartnerType | null;
  contact?: string | null;
  note?: string | null;
}

/** 操作人（契约 4.3 Operator；name 全局唯一）。 */
export interface Operator {
  id: string;
  name: string;
  createdAt: string;
  updatedAt: string;
}

/** 流水方向（契约 4.4 TxType）。 */
export type TxType = "IN" | "OUT" | "VOID";

/** 流水明细（契约 4.4 Transaction，查询基于 v_transaction_detail）。 */
export interface Transaction {
  id: string;
  type: TxType;
  itemId: string;
  itemName: string;
  unit: string;
  quantity: number;
  partnerId: string | null;
  partnerName: string | null;
  operatorId: string;
  operatorName: string;
  deviceId: string;
  remark: string | null;
  /** 业务时间（UTC RFC3339，展示需 dayjs 本地化） */
  happenedAt: string;
  copiedFrom: string | null;
  refId: string | null;
  createdAt: string;
  updatedAt: string;
}

/** 新建流水（契约 4.4 NewTransaction；JSON 字段名为 "type"）。 */
export interface NewTransaction {
  type: TxType;
  itemId: string;
  quantity: number;
  partnerId?: string | null;
  operatorId: string;
  remark?: string | null;
  /** UTC RFC3339；缺省 = 服务端当前时刻（前端把本地业务时间转 UTC，契约 7.3） */
  happenedAt?: string | null;
  /** 复制上次单据时传原单 id */
  copiedFrom?: string | null;
}

/** 流水筛选（契约 4.4 TransactionFilter；日期为 UTC RFC3339，limit 默认 100 钳制 1~500）。 */
export interface TransactionFilter {
  itemId?: string;
  partnerId?: string;
  operatorId?: string;
  type?: TxType;
  dateFrom?: string;
  dateTo?: string;
  keyword?: string;
  limit?: number;
  offset?: number;
}

/** 流水分页结果（契约 4.4 TransactionPage）。 */
export interface TransactionPage {
  rows: Transaction[];
  total: number;
  limit: number;
  offset: number;
}

/**
 * 库存行（契约 4.5 InventoryRow）。
 * 注意：Rust DTO 无 isArchived 字段——已归档货物仍在列表中，
 * 前端需与 list_items(includeArchived) 交叉比对自行折叠。
 */
export interface InventoryRow {
  itemId: string;
  itemName: string;
  spec: string | null;
  unit: string;
  totalIn: number;
  totalOut: number;
  totalVoided: number;
  balance: number;
  /** balance < 0 → 整行标红（D2 / 契约 7.4） */
  negative: boolean;
  todayIn: number;
  todayOut: number;
}

/** 统计维度（契约 4.6 group_by；operator 为 A2 超集扩展）。 */
export type StatsGroupBy = "item" | "partner" | "operator";

/** 统计查询入参（契约 4.6 StatsQuery；日期为 UTC RFC3339）。 */
export interface StatsQuery {
  groupBy: StatsGroupBy;
  dateFrom: string;
  dateTo: string;
  itemId?: string;
  partnerId?: string;
}

/** 统计汇总行（契约 4.6 StatsRow）。 */
export interface StatsRow {
  groupKey: string;
  groupName: string;
  inQty: number;
  outQty: number;
  txCount: number;
  voidCount: number;
}

/**
 * 导出类命令的统一返回（统计 Excel / 数据包 / .db 备份，契约 4.6 ExportedFile）。
 * 桌面端 path 可直接交给 opener 打开所在位置；Android 为应用私有目录路径。
 */
export interface ExportedFile {
  path: string;
  fileName: string;
  sizeBytes: number;
}

/** 数据包内各类记录条数（契约 5.2 counts）。 */
export interface PackCounts {
  items: number;
  partners: number;
  operators: number;
  transactions: number;
}

/** 导入前预览摘要（契约 5.4 PackSummary）。 */
export interface PackSummary {
  schemaVersion: number;
  exportedDeviceId: string;
  exportedDeviceName: string;
  exportedAt: string;
  counts: PackCounts;
  checksumOk: boolean;
}

/** 合并冲突记录（契约 5.4 MergeConflict）。 */
export interface MergeConflict {
  table: string;
  id: string;
  localUpdatedAt: string;
  incomingUpdatedAt: string;
  /** "local" | "incoming" */
  winner: string;
}

/** 导入合并报告（契约 5.4 ImportReport）。 */
export interface ImportReport {
  backupPath: string;
  addedTransactions: number;
  updatedItems: number;
  updatedPartners: number;
  newOperators: number;
  conflicts: MergeConflict[];
}

/**
 * 合并日志行（【契约偏离】对应第 25 条命令 list_merge_logs，
 * 读契约 3.1 的 merge_logs 审计表，供数据共账页展示「距上次同步」与历史合并明细）。
 */
export interface MergeLogEntry {
  id: string;
  importedAt: string;
  sourceDeviceId: string;
  sourceDeviceName: string | null;
  sourceFile: string;
  backupPath: string;
  addedTransactions: number;
  updatedItems: number;
  updatedPartners: number;
  newOperators: number;
  conflicts: MergeConflict[];
}
