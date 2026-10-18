<script setup lang="ts">
import { computed } from "vue";
import { useRoute } from "vue-router";
import ToastHost from "./components/ToastHost.vue";

/**
 * 根布局（契约第 6 章）：
 * - 移动端（<768px）：底部 Tab 导航（8 项）
 * - 宽屏（≥768px，md 断点）：左侧栏导航
 */
const route = useRoute();

interface NavItem {
  path: string;
  label: string;
}

const navItems: NavItem[] = [
  { path: "/", label: "速记" },
  { path: "/entry", label: "记一笔" },
  { path: "/inventory", label: "库存" },
  { path: "/transactions", label: "流水" },
  { path: "/items", label: "货物" },
  { path: "/partners", label: "往来" },
  { path: "/stats", label: "统计" },
  { path: "/sync", label: "共账" },
];

const currentTitle = computed(() => (route.meta.title as string | undefined) ?? "仓记");
</script>

<template>
  <div class="flex h-screen overflow-hidden bg-slate-50 text-slate-800">
    <!-- 宽屏侧栏（md 及以上） -->
    <aside
      class="hidden w-52 shrink-0 flex-col border-r border-slate-200 bg-white md:flex"
    >
      <div class="px-5 pb-2 pt-5 text-xl font-bold text-emerald-700">
        仓记 <span class="text-sm font-normal text-slate-400">CangJi</span>
      </div>
      <nav class="flex-1 space-y-1 overflow-y-auto px-3 py-2">
        <RouterLink
          v-for="item in navItems"
          :key="item.path"
          :to="item.path"
          class="block rounded-lg px-4 py-2.5 text-sm transition-colors"
          :class="
            route.path === item.path
              ? 'bg-emerald-600 font-medium text-white shadow-sm'
              : 'text-slate-600 hover:bg-emerald-50 hover:text-emerald-700'
          "
        >
          {{ item.label }}
        </RouterLink>
      </nav>
    </aside>

    <!-- 主内容区 -->
    <div class="flex min-w-0 flex-1 flex-col">
      <!-- 移动端顶栏 -->
      <header
        class="border-b border-slate-200 bg-white px-4 py-3 text-base font-bold text-emerald-700 md:hidden"
      >
        {{ currentTitle }}
      </header>

      <main class="flex-1 overflow-y-auto pb-20 md:pb-0">
        <RouterView />
      </main>
    </div>

    <!-- 移动端底部导航（md 以下） -->
    <nav
      class="fixed inset-x-0 bottom-0 z-10 grid grid-cols-8 border-t border-slate-200 bg-white pb-[env(safe-area-inset-bottom)] md:hidden"
    >
      <RouterLink
        v-for="item in navItems"
        :key="item.path"
        :to="item.path"
        class="flex flex-col items-center justify-center py-2 text-[11px] leading-tight transition-colors"
        :class="
          route.path === item.path
            ? 'font-semibold text-emerald-600'
            : 'text-slate-500'
        "
      >
        {{ item.label }}
      </RouterLink>
    </nav>

    <!-- 全局 toast（CommandError.message 统一提示） -->
    <ToastHost />
  </div>
</template>
