import { createRouter, createWebHashHistory } from "vue-router";

/**
 * 路由表（契约第 6 章）：hash 模式，8 个页面平级，
 * 组件懒加载，移动端底部 Tab / 宽屏侧栏共用。
 */
const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    {
      path: "/",
      name: "quick-memo",
      component: () => import("../pages/QuickMemo.vue"),
      meta: { title: "速记" },
    },
    {
      path: "/entry",
      name: "new-entry",
      component: () => import("../pages/NewEntry.vue"),
      meta: { title: "记一笔" },
    },
    {
      path: "/inventory",
      name: "inventory-overview",
      component: () => import("../pages/InventoryOverview.vue"),
      meta: { title: "库存" },
    },
    {
      path: "/transactions",
      name: "transactions",
      component: () => import("../pages/TransactionsPage.vue"),
      meta: { title: "流水" },
    },
    {
      path: "/items",
      name: "items-manage",
      component: () => import("../pages/ItemsManage.vue"),
      meta: { title: "货物" },
    },
    {
      path: "/partners",
      name: "partners-manage",
      component: () => import("../pages/PartnersManage.vue"),
      meta: { title: "往来" },
    },
    {
      path: "/stats",
      name: "stats-export",
      component: () => import("../pages/StatsExport.vue"),
      meta: { title: "统计" },
    },
    {
      path: "/sync",
      name: "data-sync",
      component: () => import("../pages/DataSync.vue"),
      meta: { title: "共账" },
    },
  ],
});

export default router;
