<script setup lang="ts">
/**
 * ⑦ 统计导出（契约第 6 章 / 4.6）：
 * - 期间起止（本地日期转 UTC）+ group_by（item / partner / operator）查询汇总表
 * - 导出 Excel（export_stats_excel，A4 交付）：桌面 dialog 选路径 / Android 收起默认路径，
 *   成功 toast + 打开所在位置（契约 4.9 导出类统一交互）
 * 注：Rust StatsQuery 无 type 入参，方向维度以表内 in/out 列分列展示。
 */
import { computed, onMounted, ref } from "vue";
import { save } from "@tauri-apps/plugin-dialog";
import { exportStatsExcel, getStats, toCommandError } from "../api/commands";
import type { StatsGroupBy, StatsRow } from "../api/types";
import EmptyState from "../components/EmptyState.vue";
import { dayEndUtc, dayStartUtc, formatQty, monthStart, todayStr } from "../utils/format";
import { isAndroid, revealInFolder } from "../utils/platform";
import { toast, toastError, toastSuccess } from "../utils/toast";

const groupBy = ref<StatsGroupBy>("item");
const dateFrom = ref(monthStart(todayStr()));
const dateTo = ref(todayStr());
const rows = ref<StatsRow[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);
const searched = ref(false);

const groupOptions: { value: StatsGroupBy; label: string }[] = [
  { value: "item", label: "按货物" },
  { value: "partner", label: "按往来单位" },
  { value: "operator", label: "按操作人" },
];

const totals = computed(() => {
  const t = { inQty: 0, outQty: 0, txCount: 0, voidCount: 0 };
  for (const r of rows.value) {
    t.inQty += r.inQty;
    t.outQty += r.outQty;
    t.txCount += r.txCount;
    t.voidCount += r.voidCount;
  }
  return t;
});

async function search(): Promise<void> {
  if (!dateFrom.value || !dateTo.value) {
    toast("请选择起止日期", "error");
    return;
  }
  if (dateFrom.value > dateTo.value) {
    toast("起始日期不能晚于截止日期", "error");
    return;
  }
  loading.value = true;
  error.value = null;
  try {
    rows.value = await getStats({
      groupBy: groupBy.value,
      dateFrom: dayStartUtc(dateFrom.value),
      dateTo: dayEndUtc(dateTo.value),
    });
    searched.value = true;
  } catch (e) {
    error.value = toCommandError(e).message;
    toastError(e);
  } finally {
    loading.value = false;
  }
}

const exporting = ref(false);

/** 导出统计 Excel（A4）：桌面 dialog 选 .xlsx 路径；Android 直接落 exports/。 */
async function exportExcel(): Promise<void> {
  if (!dateFrom.value || !dateTo.value) {
    toast("请选择起止日期", "error");
    return;
  }
  if (dateFrom.value > dateTo.value) {
    toast("起始日期不能晚于截止日期", "error");
    return;
  }
  exporting.value = true;
  try {
    let targetPath: string | undefined;
    if (!isAndroid()) {
      const picked = await save({
        title: "导出统计 Excel",
        defaultPath: `cangji-stats-${todayStr()}.xlsx`,
        filters: [{ name: "Excel 工作簿", extensions: ["xlsx"] }],
      });
      if (picked === null) return; // 用户取消
      targetPath = picked;
    }
    const file = await exportStatsExcel(
      {
        groupBy: groupBy.value,
        dateFrom: dayStartUtc(dateFrom.value),
        dateTo: dayEndUtc(dateTo.value),
      },
      targetPath,
    );
    toastSuccess(`已导出：${file.fileName}`);
    await revealInFolder(file.path);
  } catch (e) {
    toastError(e);
  } finally {
    exporting.value = false;
  }
}

onMounted(() => {
  search();
});
</script>

<template>
  <section class="mx-auto w-full max-w-4xl p-4 md:p-6">
    <div class="flex items-center justify-between">
      <h1 class="text-xl font-bold text-slate-800">统计导出</h1>
      <button
        type="button"
        class="rounded-lg bg-emerald-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-emerald-700 disabled:opacity-60"
        :disabled="exporting || loading"
        @click="exportExcel"
      >
        {{ exporting ? "导出中…" : "导出 Excel" }}
      </button>
    </div>

    <!-- 查询条件 -->
    <div class="mt-4 rounded-xl border border-slate-200 bg-white p-3">
      <div class="grid grid-cols-2 gap-2 md:grid-cols-4">
        <select
          v-model="groupBy"
          class="w-full rounded-lg border border-slate-300 px-2 py-2 text-sm text-slate-700 outline-none focus:border-emerald-400"
        >
          <option v-for="g in groupOptions" :key="g.value" :value="g.value">
            {{ g.label }}
          </option>
        </select>
        <div>
          <span class="mb-0.5 block text-[11px] text-slate-400">起始日期</span>
          <input
            v-model="dateFrom"
            type="date"
            class="w-full rounded-lg border border-slate-300 px-2 py-1.5 text-sm text-slate-700 outline-none focus:border-emerald-400"
          />
        </div>
        <div>
          <span class="mb-0.5 block text-[11px] text-slate-400">截止日期</span>
          <input
            v-model="dateTo"
            type="date"
            class="w-full rounded-lg border border-slate-300 px-2 py-1.5 text-sm text-slate-700 outline-none focus:border-emerald-400"
          />
        </div>
        <button
          type="button"
          class="rounded-lg bg-emerald-600 px-3 py-2 text-sm font-medium text-white hover:bg-emerald-700 disabled:opacity-60"
          :disabled="loading"
          @click="search"
        >
          {{ loading ? "查询中…" : "查询" }}
        </button>
      </div>
    </div>

    <!-- 结果 -->
    <div class="mt-4">
      <div v-if="loading && rows.length === 0" class="py-8 text-center text-sm text-slate-400">
        加载中…
      </div>
      <EmptyState v-else-if="error" title="统计查询失败" :description="error" />
      <EmptyState
        v-else-if="searched && rows.length === 0"
        title="该期间没有流水"
        description="换个时间范围试试。"
      />
      <template v-else-if="rows.length > 0">
        <div class="overflow-x-auto rounded-xl border border-slate-200 bg-white">
          <table class="w-full text-sm">
            <thead>
              <tr class="border-b border-slate-200 bg-slate-50 text-left text-xs text-slate-500">
                <th class="px-3 py-2 font-medium">分组</th>
                <th class="px-3 py-2 text-right font-medium">入库</th>
                <th class="px-3 py-2 text-right font-medium">出库</th>
                <th class="px-3 py-2 text-right font-medium">单据数</th>
                <th class="px-3 py-2 text-right font-medium">作废数</th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="r in rows"
                :key="r.groupKey"
                class="border-b border-slate-100 last:border-0"
              >
                <td class="px-3 py-2 font-medium text-slate-700">{{ r.groupName }}</td>
                <td class="px-3 py-2 text-right tabular-nums text-emerald-700">
                  {{ formatQty(r.inQty) }}
                </td>
                <td class="px-3 py-2 text-right tabular-nums text-amber-700">
                  {{ formatQty(r.outQty) }}
                </td>
                <td class="px-3 py-2 text-right tabular-nums text-slate-600">{{ r.txCount }}</td>
                <td class="px-3 py-2 text-right tabular-nums text-slate-600">{{ r.voidCount }}</td>
              </tr>
              <tr class="bg-slate-50 font-medium">
                <td class="px-3 py-2 text-slate-700">合计</td>
                <td class="px-3 py-2 text-right tabular-nums text-emerald-700">
                  {{ formatQty(totals.inQty) }}
                </td>
                <td class="px-3 py-2 text-right tabular-nums text-amber-700">
                  {{ formatQty(totals.outQty) }}
                </td>
                <td class="px-3 py-2 text-right tabular-nums text-slate-700">{{ totals.txCount }}</td>
                <td class="px-3 py-2 text-right tabular-nums text-slate-700">{{ totals.voidCount }}</td>
              </tr>
            </tbody>
          </table>
        </div>
        <p class="mt-2 text-xs text-slate-400">
          入库/出库为数量合计（不同货物计量单位可能不同，仅供参考）；按往来单位分组时无单位单据归「未指定」组。
        </p>
      </template>
    </div>
  </section>
</template>
