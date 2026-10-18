<script setup lang="ts">
/**
 * ⑧ 数据共账（契约第 6 章 / 4.7，A4 交付）：
 * - 设备信息卡：版本 / Schema / 设备名（可改，settings）/ 设备 ID / DB 路径 / 距上次同步
 * - 操作人改名入口（契约 4.3：改名入口在本页）
 * - 数据包：导出（dialog 选路径）→ 查看（inspect 预览不改库）→ 导入合并
 *   （先 inspect 预览 → ConfirmDialog 确认 → 自动备份 → 合并 → 报告 + 冲突明细）
 * - .db 一键备份（VACUUM INTO）；成功 toast + 打开所在位置（契约 4.9 统一交互）
 * - 合并日志区：list_merge_logs（【契约偏离】第 25 条命令）审计明细
 */
import { onMounted, ref } from "vue";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  backupDatabase,
  exportDataPack,
  importDataPack,
  inspectDataPack,
  listMergeLogs,
} from "../api/commands";
import type { ImportReport, MergeLogEntry, PackSummary } from "../api/types";
import ConfirmDialog from "../components/ConfirmDialog.vue";
import EmptyState from "../components/EmptyState.vue";
import { useSessionStore } from "../stores/session";
import { daysAgoText, formatTime, todayStr } from "../utils/format";
import { isAndroid, revealInFolder } from "../utils/platform";
import { toast, toastError, toastSuccess } from "../utils/toast";

const session = useSessionStore();

const deviceNameDraft = ref("");
const savingName = ref(false);

const editingOpId = ref<string | null>(null);
const editingOpName = ref("");
const savingOp = ref(false);

/** 导出类操作互斥忙态："" = 空闲。 */
const busy = ref<"" | "export" | "inspect" | "import" | "backup">("");

/** 查看数据包（inspect）预览结果，不改库。 */
const summary = ref<PackSummary | null>(null);

/** 待确认导入：inspect 通过后的包与摘要。 */
const confirmOpen = ref(false);
const pendingPath = ref("");
const pendingSummary = ref<PackSummary | null>(null);

/** 最近一次导入合并报告。 */
const report = ref<ImportReport | null>(null);

/** 合并日志（list_merge_logs）。 */
const logs = ref<MergeLogEntry[]>([]);
const logsError = ref<string | null>(null);
const logsLoading = ref(false);

const PACK_FILTER = [{ name: "仓记数据包", extensions: ["cjpack"] }];

/** 桌面弹 save 对话框；Android 返回 undefined 走缺省路径（契约 4.9）。 */
async function pickSave(defaultName: string, ext: string, title: string): Promise<string | null | undefined> {
  if (isAndroid()) return undefined;
  const picked = await save({
    title,
    defaultPath: `${defaultName}-${todayStr()}.${ext}`,
    filters: [{ name: ext === "db" ? "SQLite 数据库" : "仓记数据包", extensions: [ext] }],
  });
  return picked; // null = 用户取消
}

/** 选择 .cjpack 源文件；取消返回 null。 */
async function pickPack(): Promise<string | null> {
  const picked = await open({ multiple: false, filters: PACK_FILTER });
  if (picked === null || picked === undefined) return null;
  return typeof picked === "string" ? picked : (picked[0] ?? null);
}

/** 导出全量数据包。 */
async function doExportPack(): Promise<void> {
  busy.value = "export";
  try {
    const targetPath = await pickSave("cangji-pack", "cjpack", "导出数据包");
    if (targetPath === null) return; // 取消
    const file = await exportDataPack(targetPath);
    toastSuccess(`数据包已导出：${file.fileName}`);
    await revealInFolder(file.path);
  } catch (e) {
    toastError(e);
  } finally {
    busy.value = "";
  }
}

/** 查看数据包：解析 + 7 条校验中的 1–6 + 摘要，不改库。 */
async function doInspectPack(): Promise<void> {
  busy.value = "inspect";
  try {
    const path = await pickPack();
    if (!path) return;
    summary.value = await inspectDataPack(path);
    report.value = null;
  } catch (e) {
    toastError(e);
  } finally {
    busy.value = "";
  }
}

/** 导入合并第一步：选文件 → inspect 预览（校验不过直接报错，不进确认）。 */
async function startImport(): Promise<void> {
  busy.value = "import";
  try {
    const path = await pickPack();
    if (!path) return;
    pendingSummary.value = await inspectDataPack(path);
    pendingPath.value = path;
    confirmOpen.value = true;
  } catch (e) {
    toastError(e);
  } finally {
    busy.value = "";
  }
}

/** 导入合并第二步：确认后执行（后端再校验 → 自动备份 → 单事务合并 → merge_logs）。 */
async function doConfirmImport(): Promise<void> {
  confirmOpen.value = false;
  busy.value = "import";
  try {
    report.value = await importDataPack(pendingPath.value);
    summary.value = pendingSummary.value;
    toastSuccess(
      `合并完成：新增流水 ${report.value.addedTransactions}、冲突 ${report.value.conflicts.length}`,
    );
    await Promise.all([loadLogs(), session.loadAppInfo()]);
  } catch (e) {
    toastError(e);
  } finally {
    busy.value = "";
    pendingPath.value = "";
    pendingSummary.value = null;
  }
}

/** 一键备份 .db（VACUUM INTO）。 */
async function doBackup(): Promise<void> {
  busy.value = "backup";
  try {
    const targetPath = await pickSave("cangji-backup", "db", "备份数据库");
    if (targetPath === null) return;
    const file = await backupDatabase(targetPath);
    toastSuccess(`备份完成：${file.fileName}`);
    await revealInFolder(file.path);
  } catch (e) {
    toastError(e);
  } finally {
    busy.value = "";
  }
}

/** 加载合并日志。 */
async function loadLogs(): Promise<void> {
  logsLoading.value = true;
  logsError.value = null;
  try {
    logs.value = await listMergeLogs();
  } catch (e) {
    logsError.value = e instanceof Error ? e.message : String(e);
    toastError(e);
  } finally {
    logsLoading.value = false;
  }
}

async function saveDeviceName(): Promise<void> {
  const name = deviceNameDraft.value.trim();
  if (!name) {
    toast("设备名不能为空", "error");
    return;
  }
  savingName.value = true;
  try {
    await session.saveDeviceName(name);
    toastSuccess("设备名已保存");
  } catch (e) {
    toastError(e);
  } finally {
    savingName.value = false;
  }
}

function startRenameOp(id: string, name: string): void {
  editingOpId.value = id;
  editingOpName.value = name;
}

async function saveRenameOp(): Promise<void> {
  const id = editingOpId.value;
  const name = editingOpName.value.trim();
  if (!id) return;
  if (!name) {
    toast("操作人姓名不能为空", "error");
    return;
  }
  savingOp.value = true;
  try {
    await session.renameOperator(id, name);
    toastSuccess("操作人已改名（历史单据自动跟随）");
    editingOpId.value = null;
    editingOpName.value = "";
  } catch (e) {
    toastError(e);
  } finally {
    savingOp.value = false;
  }
}

onMounted(() => {
  session
    .loadAppInfo()
    .then(() => {
      deviceNameDraft.value = session.appInfo?.deviceName ?? "";
    })
    .catch(toastError);
  session.loadOperators().catch(toastError);
  loadLogs();
});
</script>

<template>
  <section class="mx-auto w-full max-w-3xl p-4 md:p-6">
    <h1 class="text-xl font-bold text-slate-800">数据共账</h1>

    <!-- 设备信息卡 -->
    <div class="mt-4 rounded-xl border border-slate-200 bg-white p-4 shadow-sm">
      <h2 class="text-sm font-semibold text-slate-700">设备信息</h2>

      <div v-if="session.appInfoError" class="mt-3">
        <EmptyState title="应用信息加载失败" :description="session.appInfoError" />
      </div>

      <div v-else-if="!session.appInfo" class="mt-3 py-4 text-center text-sm text-slate-400">
        加载中…
      </div>

      <dl v-else class="mt-3 space-y-2 text-sm">
        <div class="flex justify-between gap-3">
          <dt class="shrink-0 text-slate-500">应用版本</dt>
          <dd class="text-right font-medium tabular-nums text-slate-800">
            v{{ session.appInfo.appVersion }}
          </dd>
        </div>
        <div class="flex justify-between gap-3">
          <dt class="shrink-0 text-slate-500">Schema 版本</dt>
          <dd class="text-right font-medium tabular-nums text-slate-800">
            {{ session.appInfo.schemaVersion }}
          </dd>
        </div>
        <div class="flex justify-between gap-3">
          <dt class="shrink-0 text-slate-500">距上次同步</dt>
          <dd class="text-right font-medium text-slate-800">
            {{ daysAgoText(session.appInfo.lastSyncAt) }}
            <span
              v-if="session.appInfo.lastSyncAt"
              class="ml-1 font-normal text-slate-400"
            >
              （{{ formatTime(session.appInfo.lastSyncAt) }}）
            </span>
          </dd>
        </div>

        <!-- 设备名可编辑（settings.device_name，契约 7.2） -->
        <div>
          <dt class="text-slate-500">设备名</dt>
          <dd class="mt-1 flex gap-2">
            <input
              v-model="deviceNameDraft"
              type="text"
              maxlength="50"
              class="w-full rounded-lg border border-slate-300 px-3 py-1.5 text-sm outline-none focus:border-emerald-400"
            />
            <button
              type="button"
              class="shrink-0 rounded-lg bg-emerald-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-emerald-700 disabled:opacity-60"
              :disabled="savingName || deviceNameDraft.trim() === session.appInfo.deviceName"
              @click="saveDeviceName"
            >
              {{ savingName ? "保存中…" : "保存" }}
            </button>
          </dd>
        </div>

        <div>
          <dt class="text-slate-500">设备 ID（永不变，数据包据此区分来源）</dt>
          <dd class="mt-0.5 break-all font-mono text-xs text-slate-700">
            {{ session.appInfo.deviceId }}
          </dd>
        </div>
        <div>
          <dt class="text-slate-500">数据库路径</dt>
          <dd class="mt-0.5 break-all font-mono text-xs text-slate-700">
            {{ session.appInfo.dbPath }}
          </dd>
        </div>
      </dl>
    </div>

    <!-- 操作人改名（契约 4.3：改名入口在数据共账页） -->
    <div class="mt-4 rounded-xl border border-slate-200 bg-white p-4 shadow-sm">
      <h2 class="text-sm font-semibold text-slate-700">操作人</h2>
      <p class="mt-1 text-xs text-slate-400">
        重名不可（全局唯一）；改名后历史单据与统计自动跟随新名字。
      </p>

      <div v-if="session.operatorsError" class="mt-3">
        <EmptyState title="操作人加载失败" :description="session.operatorsError" />
      </div>
      <div v-else-if="session.operators.length === 0" class="mt-3">
        <EmptyState
          title="还没有操作人"
          description="到「记一笔」页操作人选择器右上角「＋ 新增操作人」创建。"
        />
      </div>
      <ul v-else class="mt-3 space-y-2">
        <li
          v-for="op in session.operators"
          :key="op.id"
          class="flex items-center justify-between gap-2"
        >
          <template v-if="editingOpId === op.id">
            <input
              v-model="editingOpName"
              type="text"
              maxlength="50"
              class="w-full rounded-lg border border-slate-300 px-3 py-1.5 text-sm outline-none focus:border-emerald-400"
              @keyup.enter="saveRenameOp"
            />
            <div class="flex shrink-0 gap-2">
              <button
                type="button"
                class="rounded-lg bg-emerald-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-emerald-700 disabled:opacity-60"
                :disabled="savingOp"
                @click="saveRenameOp"
              >
                保存
              </button>
              <button
                type="button"
                class="rounded-lg border border-slate-200 px-3 py-1.5 text-xs text-slate-600 hover:bg-slate-50"
                @click="editingOpId = null"
              >
                取消
              </button>
            </div>
          </template>
          <template v-else>
            <span class="min-w-0 truncate text-sm text-slate-700">{{ op.name }}</span>
            <button
              type="button"
              class="shrink-0 rounded-md border border-slate-200 px-2.5 py-1 text-xs text-slate-600 hover:bg-slate-50"
              @click="startRenameOp(op.id, op.name)"
            >
              改名
            </button>
          </template>
        </li>
      </ul>
    </div>

    <!-- 合并日志（list_merge_logs【契约偏离】审计明细） -->
    <div class="mt-4 rounded-xl border border-slate-200 bg-white p-4 shadow-sm">
      <div class="flex items-center justify-between">
        <h2 class="text-sm font-semibold text-slate-700">合并日志</h2>
        <button
          type="button"
          class="text-xs text-emerald-600 hover:underline"
          @click="loadLogs"
        >
          刷新
        </button>
      </div>

      <div v-if="logsLoading && logs.length === 0" class="py-4 text-center text-sm text-slate-400">
        加载中…
      </div>
      <EmptyState
        v-else-if="logsError"
        title="合并日志加载失败"
        :description="logsError"
      />
      <EmptyState
        v-else-if="logs.length === 0"
        title="暂无合并记录"
        description="用「导入合并」接入其他设备的数据包后，这里会记录每次导入的来源、增量与冲突明细。"
      />
      <ul v-else class="mt-3 space-y-3">
        <li v-for="l in logs" :key="l.id" class="rounded-lg border border-slate-100 bg-slate-50/60 p-3">
          <div class="flex flex-wrap items-center justify-between gap-x-3 gap-y-1 text-xs">
            <span class="font-medium text-slate-700">
              {{ formatTime(l.importedAt) }}
              <span class="ml-2 font-normal text-slate-500">
                来源：{{ l.sourceDeviceName || "未知设备" }}
              </span>
            </span>
            <span class="break-all font-mono text-[11px] text-slate-400">{{ l.sourceFile }}</span>
          </div>
          <div class="mt-1.5 flex flex-wrap gap-x-3 gap-y-1 text-xs tabular-nums text-slate-600">
            <span>新增流水 {{ l.addedTransactions }}</span>
            <span>更新货物 {{ l.updatedItems }}</span>
            <span>更新单位 {{ l.updatedPartners }}</span>
            <span>新增操作人 {{ l.newOperators }}</span>
            <span :class="l.conflicts.length > 0 ? 'text-amber-600' : 'text-slate-400'">
              冲突 {{ l.conflicts.length }}
            </span>
          </div>
          <p class="mt-1 break-all text-[11px] text-slate-400">备份：{{ l.backupPath }}</p>
        </li>
      </ul>
    </div>

    <!-- 数据包 / 备份（A4 交付：export/inspect/import_data_pack、backup_database） -->
    <div class="mt-4 rounded-xl border border-slate-200 bg-white p-4 shadow-sm">
      <h2 class="text-sm font-semibold text-slate-700">数据包与备份</h2>
      <p class="mt-1 text-xs text-slate-400">
        数据包为 <code class="font-mono">.cjpack</code>（JSON+SHA-256，四表全量含归档）；
        导入 = 校验 → 自动备份 → 单事务合并（UUID 幂等 + LWW）。备份为完整
        <code class="font-mono">.db</code> 快照兜底。
      </p>
      <div class="mt-3 grid grid-cols-2 gap-2">
        <button
          type="button"
          class="rounded-lg border border-slate-200 bg-white px-3 py-2.5 text-sm text-slate-700 hover:bg-slate-50 disabled:opacity-60"
          :disabled="busy !== ''"
          @click="doExportPack"
        >
          {{ busy === "export" ? "导出中…" : "导出数据包" }}
        </button>
        <button
          type="button"
          class="rounded-lg border border-slate-200 bg-white px-3 py-2.5 text-sm text-slate-700 hover:bg-slate-50 disabled:opacity-60"
          :disabled="busy !== ''"
          @click="doInspectPack"
        >
          {{ busy === "inspect" ? "校验中…" : "查看数据包" }}
        </button>
        <button
          type="button"
          class="rounded-lg bg-emerald-600 px-3 py-2.5 text-sm font-medium text-white hover:bg-emerald-700 disabled:opacity-60"
          :disabled="busy !== ''"
          @click="startImport"
        >
          {{ busy === "import" ? "合并中…" : "导入合并" }}
        </button>
        <button
          type="button"
          class="rounded-lg border border-slate-200 bg-white px-3 py-2.5 text-sm text-slate-700 hover:bg-slate-50 disabled:opacity-60"
          :disabled="busy !== ''"
          @click="doBackup"
        >
          {{ busy === "backup" ? "备份中…" : "备份数据库（.db）" }}
        </button>
      </div>

      <!-- 查看数据包：预览摘要（不改库） -->
      <div v-if="summary" class="mt-3 rounded-lg border border-sky-100 bg-sky-50/70 p-3 text-xs">
        <div class="flex items-center justify-between">
          <h3 class="font-semibold text-slate-700">数据包预览</h3>
          <span
            class="rounded-full px-2 py-0.5 font-medium"
            :class="summary.checksumOk ? 'bg-emerald-100 text-emerald-700' : 'bg-red-100 text-red-700'"
          >
            {{ summary.checksumOk ? "校验和通过" : "校验和异常" }}
          </span>
        </div>
        <dl class="mt-2 grid grid-cols-2 gap-x-4 gap-y-1 tabular-nums text-slate-600">
          <div class="flex justify-between"><dt>格式版本</dt><dd>v{{ summary.schemaVersion }}</dd></div>
          <div class="flex justify-between"><dt>导出时间</dt><dd>{{ formatTime(summary.exportedAt) }}</dd></div>
          <div class="col-span-2 flex justify-between gap-3">
            <dt class="shrink-0">来源设备</dt>
            <dd class="truncate text-right">{{ summary.exportedDeviceName }}</dd>
          </div>
          <div class="flex justify-between"><dt>货物</dt><dd>{{ summary.counts.items }}</dd></div>
          <div class="flex justify-between"><dt>往来单位</dt><dd>{{ summary.counts.partners }}</dd></div>
          <div class="flex justify-between"><dt>操作人</dt><dd>{{ summary.counts.operators }}</dd></div>
          <div class="flex justify-between"><dt>流水</dt><dd>{{ summary.counts.transactions }}</dd></div>
        </dl>
      </div>

      <!-- 导入合并报告（增量 + 冲突明细） -->
      <div v-if="report" class="mt-3 rounded-lg border border-emerald-100 bg-emerald-50/70 p-3 text-xs">
        <h3 class="font-semibold text-slate-700">上次合并报告</h3>
        <div class="mt-1.5 flex flex-wrap gap-x-4 gap-y-1 tabular-nums text-slate-600">
          <span>新增流水 {{ report.addedTransactions }}</span>
          <span>更新货物 {{ report.updatedItems }}</span>
          <span>更新单位 {{ report.updatedPartners }}</span>
          <span>新增操作人 {{ report.newOperators }}</span>
        </div>
        <p class="mt-1 break-all text-[11px] text-slate-400">备份：{{ report.backupPath }}</p>
        <ul v-if="report.conflicts.length > 0" class="mt-2 space-y-1">
          <li
            v-for="(c, i) in report.conflicts"
            :key="i"
            class="rounded bg-amber-50 px-2 py-1 text-[11px] text-amber-700"
          >
            {{ c.table }} · {{ c.id.slice(0, 8) }}… → 保留{{ c.winner === "local" ? "本机" : "对方" }}版本
            （本机 {{ formatTime(c.localUpdatedAt) }} / 对方 {{ formatTime(c.incomingUpdatedAt) }}）
          </li>
        </ul>
      </div>
    </div>

    <!-- 导入确认（契约：inspect 预览通过后弹出，确认即自动备份并合并） -->
    <ConfirmDialog
      v-if="confirmOpen && pendingSummary"
      title="确认导入合并此数据包？"
      :message="`来源：${pendingSummary.exportedDeviceName}（${formatTime(pendingSummary.exportedAt)}） · 流水 ${pendingSummary.counts.transactions} 笔。导入前会自动备份当前数据库，合并为单事务，失败整体回滚。`"
      confirm-text="确认导入"
      @confirm="doConfirmImport"
      @cancel="confirmOpen = false"
    />
  </section>
</template>
