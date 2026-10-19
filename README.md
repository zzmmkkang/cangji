<div align="center">

# 仓记 CangJi

**小商户仓库进出记账 · 本地优先 · 单文件数据包共账 · 无服务器无账号**

![Version](https://img.shields.io/badge/version-0.1.0-blue)
![License](https://img.shields.io/badge/license-MIT-green)
![Tauri](https://img.shields.io/badge/Tauri-2-orange)
![Vue](https://img.shields.io/badge/Vue-3.5-42b883)
![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Android-lightgrey)

</div>

---

## 简介

仓记 CangJi 是一款开源的仓库货物进出记账应用：所有数据保存在设备本地的 SQLite 单文件里，**没有服务器、没有账号、没有云同步**。多人共账不靠联网，而是导出一个单文件数据包（`.cjpack`），通过微信/网盘/U 盘传递后在主库合并，再回发完整账本。同一套代码（Tauri 2 + Vue 3 + Rust）同时构建 Windows 与 Android 双端，手机电脑记同一本账。

- 每一笔入库/出库都有记录，库存余量由流水实时推导，永远一致
- 流水**只增不改**：错了只能「作废」，原始记录永久保留，可追溯
- 合并幂等：同一包重复导入 0 新增 0 覆盖，冲突按「最后修改时间」裁决（LWW）

## 截图

> 📷 **TODO（待补）**：以下截图将在 Release 前补齐，路径建议 `docs/screenshots/`。

- [ ] 速记页（自然语言解析卡）
- [ ] 记一笔 / 库存总览（负库存标红）
- [ ] 流水记录（筛选与作废）
- [ ] 数据共账（导入预检与合并报告）
- [ ] 统计导出 / Android 真机

## 核心特性

- **记一笔**：方向（入库/出库）→ 货物 → 数量 → 往来单位 → 备注，表单直录
- **自然语言速记**：输入「卖10箱矿泉水给张三」即可解析出方向/数量/货物/往来单位，多候选项打分切换，一键预填提交；也可随时退回表单兜底
- **库存总览**：全部货物当前余量、今日进出；**负库存放行但标红提示**（小商户先出货后补录的真实场景）
- **流水作废（append-only）**：流水表只增不改，作废追加标记记录，原单保留、库存自动回冲，不提供物理删除
- **统计与 Excel 导出**：按单位/货物/时间段汇总，导出 `.xlsx`，一键打开所在位置
- **`.cjpack` 数据共账**：导出本库快照 → 主库导入（7 项前置校验 + 预检弹窗 + 合并报告）→ UUID 去重、档案 LWW 冲突消解、合并日志可查；导入前自动备份
- **Win + Android 双端**：同一代码库构建 Windows 安装包与 Android APK，移动端底部 Tab / 宽屏侧栏自适应

## 下载

GitHub Release（`v0.1.0`）提供三个产物，**均为本机实测构建**：

| 产物 | 说明 |
|---|---|
| `CangJi_0.1.0_x64-setup.exe` | Windows NSIS 安装包（**主推**，2,178,866 B） |
| `CangJi_0.1.0_x64_en-US.msi` | Windows MSI 安装包（2,924,544 B；文件名 `en-US` 因构建机 locale，安装界面为英文；**需管理员权限**——为全机安装模式，会触发 UAC；免管理员的用户级安装请用上面的 NSIS 包） |
| `app-universal-release.apk` | Android universal APK，4 ABI 合并（27,942,424 B；**已签名**，可直接安装到 Android 7.0+） |

> ⚠️ 如实说明：Release 上的 APK 已于 2026-10-19 用自签名证书签名（APK Signature Scheme v2/v3），可直接安装；**从源码自行构建出的 APK 仍默认是 unsigned**，需按签名步骤处理。MSI 语言标识为 en-US。签名步骤与实测记录见 [`docs/release/RELEASE-v0.1.0.md`](docs/release/RELEASE-v0.1.0.md)。

## 从源码构建

### 通用前置（Windows 实测环境）

| 组件 | 实测版本/位置 |
|---|---|
| Rust | rustup + stable-msvc（rustc 1.99.1） |
| Visual Studio BuildTools 2022 | VC Tools + Win SDK |
| Node.js + npm | vue 3.5.44 / vite 6.4.4 / @tauri-apps/cli 2.12.3 |
| JDK 17（Android 构建需要） | Temurin 17.0.20.1，`JAVA_HOME` 已设 |
| Android SDK | `ANDROID_HOME`（platform-tools、platforms;android-35、build-tools;35.0.0） |
| Android NDK | 27.0.12077973 |

> 💡 国内网络提示：rustup 安装/更新工具链过慢或失败时，可将 `RUSTUP_DIST_SERVER` / `RUSTUP_UPDATE_ROOT` 环境变量指向国内镜像源后重试（通用做法，本机未实测）。

### Windows

```powershell
npm install
npm run build                # = vue-tsc --noEmit && vite build
cd src-tauri
cargo test --quiet           # 实测 48 passed / 0 failed
cd ..
npm run tauri dev            # 开发冒烟：窗口拉起、DB 打开、日志无 panic
npm run tauri build          # 产出 NSIS + MSI（bundle.targets: ["nsis","msi"]）
```

产物路径：

```
src-tauri\target\release\bundle\nsis\CangJi_0.1.0_x64-setup.exe
src-tauri\target\release\bundle\msi\CangJi_0.1.0_x64_en-US.msi
```

**已知构建坑（实测踩过，均已解决）**：

1. **WiX 工具下载直连 GitHub 中断** → 设置环境变量后重跑 `npm run tauri build`（下载后 tauri 仍做官方哈希校验）：
   ```powershell
   $env:TAURI_BUNDLER_TOOLS_GITHUB_MIRROR = "https://ghfast.top/https://github.com"
   ```
2. 若 Rust 工具链下载受网络限制，参考上文 rustup 镜像提示。

### Android

前置：JDK 17 + Android SDK/NDK（见通用前置表），`gen/android` 为 Tauri 生成物（**请勿手工改动**）。

```powershell
npx tauri android init      # 一次性；本仓库已包含生成好的 src-tauri\gen\android
npx tauri android build --apk
```

产物路径：

```
src-tauri\gen\android\app\build\outputs\apk\universal\release\app-universal-release-unsigned.apk
```

**已知构建坑（实测踩过，均已解决）**：

1. **`SymlinkNotAllowed`（tauri 把 `.so` 软链进 jniLibs）** → Windows 需开启「开发者模式」（`AllowDevelopmentWithoutDevLicense=1`、`AllowAllTrustedApps=1`）。
2. **仓库路径含中文**（AGP 报 `project path contains non-ASCII characters`）→ 在用户级 `%USERPROFILE%\.gradle\gradle.properties` 写入：
   ```properties
   android.overridePathCheck=true
   ```
   （同文件另加「IPv4 优先」配置；只改用户级配置，不动生成物。）
3. **gradle wrapper 下载 10s 超时/仅 60KB/s** → 从腾讯镜像手动下载对应版本分发包（实测 9.6.1），放入 `%USERPROFILE%\.gradle\wrapper\dists\gradle-9.6.1-bin\<hash>\` 并创建 `.ok` 标记。
4. **APK 未签名** → 生成 keystore 后签名（keytool 命令模板见 [`docs/release/RELEASE-v0.1.0.md`](docs/release/RELEASE-v0.1.0.md)）；或临时用 `npx tauri android build --apk --debug` 出 debug 签名包。

### 验收命令（实测全绿）

```powershell
cargo test --quiet           # 48 passed / 0 failed
npm run build                # vue-tsc --noEmit 0 错 && vite build ✓
npm run tauri dev            # 冒烟：无 panic/error
```

## 使用简述（8 个页面）

| 页面 | 一句话 |
|---|---|
| 速记（首页） | 自然语言一句话出单：解析方向/数量/货物/单位，确认即提交，另带最近单据快览 |
| 记一笔 | 标准表单录单：方向 → 货物 → 数量 → 往来单位 → 备注 |
| 库存总览 | 全部货物当前余量与今日进出，负库存标红（放行不拦截） |
| 流水记录 | 按货物/单位/操作人/时间段筛选，每行可「复制此单」或「作废」 |
| 货物管理 | 货物档案（SKU：名称/编号/规格/单位）增删改查 |
| 往来单位管理 | 供应商/客户档案增删改查，录单时下拉选择 |
| 统计导出 | 按单位/货物/时间段汇总，导出 Excel（`.xlsx`）并打开所在位置 |
| 数据共账 | 导出/导入 `.cjpack` 数据包（校验 → 预检 → 合并报告）、合并日志、数据库备份导出 |

**日常共账流程**：成员导出 `.cjpack` → 主库导入合并（自动校验、预检、备份）→ 主库导出完整包回发 → 各成员导入，全账本一致。重复导入同一包幂等；档案冲突按最后修改时间裁决并出具报告。

## Roadmap / 一期非目标

一期（0.1.x）**明确不做**（依据 [A0 架构契约第 8 章](docs/contracts/A0-架构与接口契约.md)）：

- ❌ 多用户/账号/密码/权限（身份仅为从操作人列表选名字）
- ❌ 云同步/服务器/实时协同（共账走人工数据包）
- ❌ Excel 旧账**导入**（导出 Excel 属已交付的统计功能）
- ❌ iOS / macOS 首发（首发 Windows + Android）
- ❌ 批次/序列号追溯（按品种/SKU 总量制）
- ❌ 低库存预警（负库存标红 ≠ 预警）
- ❌ 增量同步包（一期全量包）
- ❌ 自动更新、多语言/主题切换（中文单语、跟随系统浅色）、数据加密（本地明文 SQLite）

后续可选方向：低库存预警、按单位对账单、增量同步包、macOS/iOS 适配（见《规划书》三期）。

## 文档

- [总控与审批文档](docs/00-总控与审批文档.md) — 流水线唯一入口
- [架构与接口契约 A0](docs/contracts/A0-架构与接口契约.md) — 技术栈与非目标的事实来源
- [发版步骤 RELEASE-v0.1.0](docs/release/RELEASE-v0.1.0.md) — 打 tag、传 Release、APK 签名
- [变更日志 CHANGELOG](CHANGELOG.md)

## 许可证

[MIT](LICENSE) © 2026 CangJi Contributors
