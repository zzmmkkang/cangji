<script setup lang="ts">
/**
 * ⑥ 往来单位管理（契约第 6 章）：与货物管理同构 + type 切换（supplier/customer/both）。
 * 同名建档软提示不阻断；contact/note 传 "" 置 NULL（A2 交接要点）；无独立删除，仅归档。
 */
import { computed, onMounted, reactive, ref } from "vue";
import type { Partner, PartnerType } from "../api/types";
import { PARTNER_TYPE_LABELS } from "../api/types";
import EmptyState from "../components/EmptyState.vue";
import { usePartnersStore } from "../stores/partners";
import { formatTime } from "../utils/format";
import { toast, toastError, toastSuccess } from "../utils/toast";

const partnersStore = usePartnersStore();

const kw = ref("");
const formOpen = ref(false);
const saving = ref(false);
const formError = ref("");

const form = reactive({
  id: null as string | null,
  name: "",
  type: "both" as PartnerType,
  contact: "",
  note: "",
});

const typeOptions: { value: PartnerType; label: string }[] = [
  { value: "supplier", label: PARTNER_TYPE_LABELS.supplier },
  { value: "customer", label: PARTNER_TYPE_LABELS.customer },
  { value: "both", label: PARTNER_TYPE_LABELS.both },
];

const shown = computed(() => partnersStore.search(kw.value, true));

function openCreate(): void {
  Object.assign(form, { id: null, name: "", type: "both", contact: "", note: "" });
  formError.value = "";
  formOpen.value = true;
}

function openEdit(partner: Partner): void {
  Object.assign(form, {
    id: partner.id,
    name: partner.name,
    type: partner.type,
    contact: partner.contact ?? "",
    note: partner.note ?? "",
  });
  formError.value = "";
  formOpen.value = true;
}

async function submitForm(): Promise<void> {
  const name = form.name.trim();
  if (!name) {
    formError.value = "单位名称不能为空";
    return;
  }
  const isEdit = form.id !== null;
  const dup = partnersStore.hasSameName(name, form.id ?? undefined);
  saving.value = true;
  try {
    if (isEdit) {
      await partnersStore.update(form.id as string, {
        name,
        type: form.type,
        contact: form.contact.trim(),
        note: form.note.trim(),
      });
      toastSuccess("已保存往来单位");
    } else {
      await partnersStore.create({
        name,
        type: form.type,
        contact: form.contact.trim() || null,
        note: form.note.trim() || null,
      });
      if (dup) {
        toast(`已建档；已存在同名单位「${name}」（按约定不阻断）`, "info");
      } else {
        toastSuccess("已新建往来单位");
      }
    }
    formOpen.value = false;
  } catch (e) {
    toastError(e);
  } finally {
    saving.value = false;
  }
}

async function toggleArchived(partner: Partner): Promise<void> {
  try {
    const wasArchived = partner.isArchived;
    await partnersStore.setArchived(partner.id, !wasArchived);
    toastSuccess(wasArchived ? `已恢复「${partner.name}」` : `已归档「${partner.name}」`);
  } catch (e) {
    toastError(e);
  }
}

onMounted(() => {
  partnersStore.fetch(true).catch(toastError);
});
</script>

<template>
  <section class="mx-auto w-full max-w-3xl p-4 md:p-6">
    <div class="flex items-center justify-between">
      <h1 class="text-xl font-bold text-slate-800">往来单位管理</h1>
      <button
        type="button"
        class="rounded-lg bg-emerald-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-emerald-700"
        @click="openCreate"
      >
        ＋ 新建单位
      </button>
    </div>

    <!-- 新建 / 编辑表单 -->
    <div
      v-if="formOpen"
      class="mt-4 rounded-xl border border-emerald-200 bg-emerald-50/50 p-4"
    >
      <h2 class="text-sm font-semibold text-slate-700">
        {{ form.id ? "编辑往来单位" : "新建往来单位" }}
      </h2>
      <div class="mt-3 grid grid-cols-1 gap-3 sm:grid-cols-2">
        <div class="sm:col-span-2">
          <label class="mb-1 block text-xs font-medium text-slate-500">名称 *</label>
          <input
            v-model="form.name"
            type="text"
            maxlength="100"
            placeholder="如：老王批发部"
            class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm outline-none focus:border-emerald-400"
          />
        </div>
        <div>
          <label class="mb-1 block text-xs font-medium text-slate-500">类型</label>
          <select
            v-model="form.type"
            class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm text-slate-700 outline-none focus:border-emerald-400"
          >
            <option v-for="t in typeOptions" :key="t.value" :value="t.value">
              {{ t.label }}
            </option>
          </select>
        </div>
        <div>
          <label class="mb-1 block text-xs font-medium text-slate-500">联系人</label>
          <input
            v-model="form.contact"
            type="text"
            maxlength="100"
            placeholder="选填（清空即删除）"
            class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm outline-none focus:border-emerald-400"
          />
        </div>
        <div class="sm:col-span-2">
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
      placeholder="搜索名称 / 联系人 / 备注…"
      class="mt-4 w-full rounded-lg border border-slate-300 px-3 py-2 text-sm outline-none focus:border-emerald-400"
    />

    <!-- 列表 -->
    <div v-if="partnersStore.loading && partnersStore.list.length === 0" class="py-8 text-center text-sm text-slate-400">
      加载中…
    </div>
    <EmptyState
      v-else-if="partnersStore.error"
      title="往来单位加载失败"
      :description="partnersStore.error"
    />
    <EmptyState
      v-else-if="shown.length === 0"
      :title="kw ? '没有匹配的单位' : '还没有往来单位'"
      :description="kw ? '换个关键词试试。' : '点右上角「＋ 新建单位」开始建档。'"
    />
    <div v-else class="mt-3 space-y-2">
      <div
        v-for="partner in shown"
        :key="partner.id"
        class="rounded-xl border bg-white p-3 shadow-sm"
        :class="partner.isArchived ? 'border-slate-200 opacity-70' : 'border-slate-200'"
      >
        <div class="flex items-start justify-between gap-3">
          <div class="min-w-0">
            <div class="flex flex-wrap items-center gap-2">
              <span
                class="min-w-0 truncate text-sm font-medium"
                :class="partner.isArchived ? 'text-slate-500 line-through' : 'text-slate-800'"
              >
                {{ partner.name }}
              </span>
              <span class="rounded bg-emerald-50 px-1.5 py-0.5 text-[11px] text-emerald-700">
                {{ PARTNER_TYPE_LABELS[partner.type] }}
              </span>
              <span
                v-if="partner.isArchived"
                class="rounded bg-slate-100 px-1.5 py-0.5 text-[11px] text-slate-500"
              >
                已归档
              </span>
            </div>
            <div class="mt-1 text-xs text-slate-500">
              <span v-if="partner.contact">联系人 {{ partner.contact }}</span>
            </div>
            <p v-if="partner.note" class="mt-0.5 text-xs text-slate-400">备注：{{ partner.note }}</p>
            <p class="mt-0.5 text-[11px] text-slate-300">
              更新于 {{ formatTime(partner.updatedAt) }}
            </p>
          </div>
          <div class="flex shrink-0 flex-col gap-1.5">
            <button
              type="button"
              class="rounded-md border border-slate-200 px-2.5 py-1 text-xs text-slate-600 hover:bg-slate-50"
              @click="openEdit(partner)"
            >
              编辑
            </button>
            <button
              type="button"
              class="rounded-md px-2.5 py-1 text-xs"
              :class="
                partner.isArchived
                  ? 'border border-emerald-200 text-emerald-600 hover:bg-emerald-50'
                  : 'border border-amber-200 text-amber-600 hover:bg-amber-50'
              "
              @click="toggleArchived(partner)"
            >
              {{ partner.isArchived ? "恢复" : "归档" }}
            </button>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>
