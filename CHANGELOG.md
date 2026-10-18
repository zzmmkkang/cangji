# 变更日志

本项目所有有意义的变更都记录在此文件。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [0.1.0] - 2026-10-18

首发版本：Windows + Android 双端、单机能用 + 数据包共账全量功能。

### Added — 速记（首页）
- 自然语言速记解析：方向/数量识别，货物与往来单位模糊匹配打分（100–40），多候选项三态切换，一键预填「记一笔」提交
- 零命中的口语表达退回表单兜底（立即建档 / 改为备注）
- 最近单据快览

### Added — 记一笔
- 标准录单表单：方向切换 → 选货物 → 数量 → 选往来单位 → 备注
- 数量按货物单位校验（整数/小数）

### Added — 库存总览
- 全部货物卡片：当前余量、今日进出
- 负库存**放行并标红**提示（决策 D2）

### Added — 流水记录
- 按货物/单位/操作人/时间段筛选
- 每行「复制此单」预填新单
- 作废（append-only）：追加作废标记、原单保留、库存自动回冲，不提供物理删除

### Added — 货物管理 / 往来单位管理
- 货物档案（名称/编号/规格/单位/备注）增删改查
- 往来单位档案（供应商/客户/两者皆是）增删改查

### Added — 统计导出
- 按单位/货物/时间段汇总
- 导出 Excel（`.xlsx`，rust_xlsxwriter），「打开所在位置」；Android 端经系统分享取走

### Added — 数据共账
- 导出 `.cjpack` 数据包（SHA-256 校验和）与 `.db` 全量备份
- 导入合并：7 项前置校验 → 预检弹窗（计数/冲突明细）→ 导入前自动备份 → 合并报告（变更/冲突计数）
- 合并规则：流水 UUID 去重（幂等）、档案按最后修改时间 LWW 裁决，`merge_logs` 合并日志可查

### Added — 工程与双端
- Tauri 2 + Vue 3（`<script setup>` + TS）+ Pinia + rusqlite（bundled SQLite）单连接 WAL
- 8 页面 hash 路由，移动端底部 Tab / 宽屏侧栏自适应
- Rust 数据层单测（48 项）、`vue-tsc --noEmit` 类型检查
- Windows 打包：`npm run tauri build` → NSIS（`CangJi_0.1.0_x64-setup.exe`）+ MSI（`CangJi_0.1.0_x64_en-US.msi`）
- Android 打包：`npx tauri android build --apk` → `app-universal-release-unsigned.apk`（universal，4 ABI，versionCode 1000）

### 已知限制
- APK 当前**未签名**，需自行签名后安装（签名步骤见 `docs/release/RELEASE-v0.1.0.md`）
- MSI 语言标识为 en-US（构建机 locale 所致，安装界面为英文）
- 22 项人工验收点验项（安装/真机/跨设备共账）待执行，清单见 `docs/handoffs/H5-测试打包-交接与验收单.md` 附录 A

[0.1.0]: https://github.com/<your-org>/cangji/releases/tag/v0.1.0
