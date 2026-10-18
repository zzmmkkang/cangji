<script setup lang="ts">
/**
 * ⑤ 货物管理（契约第 6 章）：档案 CRUD + 归档切换（软删除）。
 * - 表单字段：name（必填）、code、spec、unit（必填）、note；更新时可空字段传 "" 置 NULL（A2 交接要点）
 * - 同名建档仅软提示不阻断（契约 3.4 / A2 约定）
 * - 支持从「记一笔」携 ?from=entry 跳入建档，建完自动返回
 */
import { computed, onMounted, reactive, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import type { Item } from "../api/types";
import EmptyState from "../components/EmptyState.vue";
import { useItemsStore } from "../stores/items";
import { formatTime } from "../utils/format";
import { toast, toastError, toastSuccess } from "../utils/toast";

const route = useRoute();
const router = useRouter();
const itemsStore = useItemsStore();

const kw = ref("");
const formOpen = ref(false);
const saving = ref(false);
const formError = ref("");

const form = reactive({
  id: null as string | null,
  name: "",
  code: "",
  spec: "",
  unit: "",
  note: "",
});

const shown = computed(() => {
  const q = kw.value.trim().toLowerCase();
  if (!q) return itemsStore.list;
  return itemsStore.list.filter(
    (i) =>
      i.name.toLowerCase().includes(q) ||
      (i.code ?? "").toLowerCase().includes(q),
  );
});

function openCreate(): void {
  Object.assign(form, { id: null, name: "", code: "", spec: "", unit: "", note: "" });
  formError.value = "";
  formOpen.value = true;
}

function openEdit(item: Item): void {
  Object.assign(form, {
    id: item.id,
    name: item.name,
    code: item.code ?? "",
    spec: item.spec ?? "",
    unit: item.unit,
    note: item.note ?? "",
  });
  formError.value = "";
  formOpen.value = true;
}

async function submitForm(): Promise<void> {
  const name = form.name.trim();
  const unit = form.unit.trim();
  if (!name) {
    formError.value = "货物名称不能为空";
    return;
  }
  if (!unit) {
    formError.value = "计量单位不能为空";
    return;
  }
  const isEdit = form.id !== null;
  // 同名软提示（A2：后端不阻断，前端提示后继续提交）
  const dup = itemsStore.hasSameName(name, form.id ?? undefined);
  saving.value = true;
  try {
    if (isEdit) {
      await itemsStore.update(form.id as string, {
        name,
        unit,
        // "" = 置 NULL（A2 交接要点：null/省略 = 不修改）
        code: form.code.trim(),
        spec: form.spec.trim(),
        note: form.note.trim(),
      });
      toastSuccess("已保存货物档案");
    } else {
      await itemsStore.create({
        name,
        unit,
        code: form.code.trim() || null,
        spec: form.spec.trim() || null,
        note: form.note.trim() || null,
      });
      if (dup) {
        toast(`已建档；已存在同名货物「${name}」（按约定不阻断）`, "info");
      } else {
        toastSuccess("已新建货物");
      }
    }
    formOpen.value = false;
    if (!isEdit && route.query.from === "entry") router.push("/entry");
  } catch (e) {
    toastError(e);
  } finally {
    saving.value = false;
  }
}

async function toggleArchived(item: Item): Promise<void> {
  try {
    const wasArchived = item.isArchived;
    await itemsStore.setArchived(item.id, !wasArchived);
    toastSuccess(wasArchived ? `已恢复「${item.name}」` : `已归档「${item.name}」`);
  } catch (e) {
    toastError(e);
  }
}

onMounted(() => {
  itemsStore.fetch(true).catch(toastError);
});
</script>

<template>
  <section class="mx-auto w-full max-w-3xl p-4 md:p-6">
    <div class="flex items-center justify-between">
      <h1 class="text-xl font-bold text-slate-800">货物管理</h1>
      <button
        type="button"
        class="rounded-lg bg-emerald-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-emerald-700"
        @click="openCreate"
      >
        ＋ 新建货物
      </button>
    </div>

    <!-- 新建 / 编辑表单 -->
    <div
      v-if="formOpen"
      class="mt-4 rounded-xl border border-emerald-200 bg-emerald-50/50 p-4"
    >
      <h2 class="text-sm font-semibold text-slate-700">
        {{ form.id ? "编辑货物" : "新建货物" }}
      </h2>
      <div class="mt-3 grid grid-cols-1 gap-3 sm:grid-cols-2">
        <div class="sm:col-span-2">
          <label class="mb-1 block text-xs font-medium text-slate-500">名称 *</label>
          <input
            v-model="form.name"
            type="text"
            maxlength="100"
            placeholder="如：苹果汁"
            class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm outline-none focus:border-emerald-400"
          />
        </div>
        <div>
          <label class="mb-1 block text-xs font-medium text-slate-500">自编码</label>
          <input
            v-model="form.code"
            type="text"
            maxlength="50"
            placeholder="选填，如 SKU-001（清空即删除）"
            class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm outline-none focus:border-emerald-400"
          />
        </div>
        <div>
          <label class="mb-1 block text-xs font-medium text-slate-500">规格</label>
          <input
            v-model="form.spec"
            type="text"
            maxlength="100"
            placeholder="选填，如 500ml×24"
            class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm outline-none focus:border-emerald-400"
          />
        </div>
        <div>
          <label class="mb-1 block text-xs font-medium text-slate-500">计量单位 *</label>
          <input
            v-model="form.unit"
            type="text"
            maxlength="20"
            placeholder="如：箱 / kg / 个"
            class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm outline-none focus:border-emerald-400"
          />
        </div>
        <div>
          <label class="mb-1 block text-xs font-medium text-slate-500">备注</label>
          <input
            v-model="form.note"
            type="text"
            maxlength="200"
            placeholder="选填（清空即删除）"
            class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm outline-none focus:border-emerald-400"
          />
        </div>
      </div>
      <p v-if="formError" class="mt-2 text-xs text-red-600">{{ formError }}</p>
      <div class="mt-3 flex gap-2">
        <button
          type="button"
          class="rounded-lg bg-emerald-600 px-4 py-2 text-sm font-medium text-white hover:bg-emerald-700 disabled:opacity-60"
          :disabled="saving"
          @click="submitForm"
        >
          {{ saving ? "保存中…" : "保存" }}
        </button>
        <button
          type="button"
          class="rounded-lg border border-slate-200 bg-white px-4 py-2 text-sm text-slate-600 hover:bg-slate-50"
          @click="formOpen = false"
        >
          取消
        </button>
      </div>
    </div>

    <!-- 搜索 -->
    <input
      v-model="kw"
      type="text"
      placeholder="搜索名称 / 编码…"
      class="mt-4 w-full rounded-lg border border-slate-300 px-3 py-2 text-sm outline-none focus:border-emerald-400"
    />

    <!-- 列表 -->
    <div v-if="itemsStore.loading && itemsStore.list.length === 0" class="py-8 text-center text-sm text-slate-400">
      加载中…
    </div>
    <EmptyState
      v-else-if="itemsStore.error"
      title="货物列表加载失败"
      :description="itemsStore.error"
    />
    <EmptyState
      v-else-if="shown.length === 0"
      :title="kw ? '没有匹配的货物' : '还没有货物档案'"
      :description="kw ? '换个关键词试试。' : '点右上角「＋ 新建货物」开始建档。'"
    />
    <div v-else class="mt-3 space-y-2">
      <div
        v-for="item in shown"
        :key="item.id"
        class="rounded-xl border bg-white p-3 shadow-sm"
        :class="item.isArchived ? 'border-slate-200 opacity-70' : 'border-slate-200'"
      >
        <div class="flex items-start justify-between gap-3">
          <div class="min-w-0">
            <div class="flex flex-wrap items-center gap-2">
              <span
                class="min-w-0 truncate text-sm font-medium"
                :class="item.isArchived ? 'text-slate-500 line-through' : 'text-slate-800'"
              >
                {{ item.name }}
              </span>
              <span
                v-if="item.isArchived"
                class="rounded bg-slate-100 px-1.5 py-0.5 text-[11px] text-slate-500"
              >
                已归档
              </span>
            </div>
            <div class="mt-1 text-xs text-slate-500">
              <span v-if="item.code">编码 {{ item.code }} · </span>
              <span v-if="item.spec">{{ item.spec }} · </span>
              单位 {{ item.unit }}
            </div>
            <p v-if="item.note" class="mt-0.5 text-xs text-slate-400">备注：{{ item.note }}</p>
            <p class="mt-0.5 text-[11px] text-slate-300">
              更新于 {{ formatTime(item.updatedAt) }}
            </p>
          </div>
          <div class="flex shrink-0 flex-col gap-1.5">
            <button
              type="button"
              class="rounded-md border border-slate-200 px-2.5 py-1 text-xs text-slate-600 hover:bg-slate-50"
              @click="openEdit(item)"
            >
              编辑
            </button>
            <button
              type="button"
              class="rounded-md px-2.5 py-1 text-xs"
              :class="
                item.isArchived
                  ? 'border border-emerald-200 text-emerald-600 hover:bg-emerald-50'
                  : 'border border-amber-200 text-amber-600 hover:bg-amber-50'
              "
              @click="toggleArchived(item)"
            >
              {{ item.isArchived ? "恢复" : "归档" }}
            </button>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>
