# RELEASE · v0.1.0 发版步骤

> 环节：A6 开源发布 ｜ 项目：仓记 CangJi（repo：`cangji`）｜ 日期：2026-10-18
> 本文档是**供用户在有网络/有 git 的机器上执行**的发版清单；A6 环境无 git 仓库、不联网 push，**未执行任何 git 命令、未创建 GitHub Release**，以下步骤均为待执行项。
>
> **✅ 执行记录（2026-10-19）**：第 1、3、4 节已实际执行完成——仓库 <https://github.com/zzmmkkang/cangji>（master，177 文件）、commit `c52fd4f`、tag `v0.1.0`、Release <https://github.com/zzmmkkang/cangji/releases/tag/v0.1.0>（三资产字节数与第 2 节一致，非 draft）。
> 实操备注：`gh auth login` 与 `git push` 均需 `HTTPS_PROXY=http://127.0.0.1:7890`（github.com 直连被重置，api.github.com 直连可用）；本机未配 git 全局身份，commit 用环境变量 `GIT_AUTHOR_*` 注入；暂存时需排除 `gen/android/.kotlin` 错误日志。
>
> **✅ 签名记录（2026-10-19）**：§3 已实际执行——keystore 生成于**仓库外** `C:\Users\mac\.cangji\cangji-release.keystore`（凭据同目录 `KEYSTORE-CREDENTIALS.txt`），签名产物 `app-universal-release.apk`（27,942,424 B，v2/v3 方案，`apksigner verify` 通过）已上传 Release 并替换原 unsigned 资产。§0 中人工验收与截图仍待执行。

---

## 0. 发版前检查（已完成的与待办的）

- [x] 三产物已构建并核对大小（见第 2 节，来源 H5 §1.1，A6 已复测存在性与字节数一致）
- [x] README / LICENSE / CHANGELOG / 本文档版本号统一 `0.1.0`，日期 `2026-10-18`
- [x] `.gitignore` 覆盖密钥三模式：`*.keystore`、`*.jks`、`keystore.properties`（另 `src-tauri/gen/android/.gitignore` 覆盖 `key.properties`、`keystore.properties`、`build`、`.gradle`）
- [ ] 人工验收：H5 附录 A 的 22 项 checklist（安装/速记/共账/统计/回归/真机/跨设备）——建议**至少完成 A、E 两组**再公开发布
- [ ] 截图补齐（README「截图」节 TODO 占位）

---

## 1. 初始化 git 仓库并推送（本机执行，PowerShell，仓库根 `e:\仓库管理`）

```powershell
cd e:\仓库管理

git init
git status                      # 预期：node_modules/ dist/ src-tauri/target/ *.db *.cjpack 均被忽略，不出现 keystore/密钥/数据库文件
git add README.md LICENSE CHANGELOG.md package.json package-lock.json ^
          index.html vite.config.ts tsconfig.json tsconfig.node.json ^
          .gitignore src src-tauri docs 仓库货物进出管理系统-规划书.md
git status                      # 再确认：无密钥、无大体积 target/、无 *.db / *.cjpack
git commit -m "feat: 仓记 CangJi v0.1.0 — 双端本地记账与数据包共账首发"
```

> 说明：`git status` 若出现 `src-tauri/gen/android/app/build/`、`*.db`、`*.keystore` 等条目**即为异常**，先停下检查 `.gitignore`。

### GitHub 建仓与推送

```powershell
# 方式一：gh CLI（api.github.com 在本机实测可用，见 H1 §4 #5）
gh repo create cangji --public --source=. --remote=origin
git push -u origin main          # 新建仓库默认分支若为 master，先 git branch -M main
```

方式二（无 gh）：网页 https://github.com/new 创建仓库 `cangji`（公开），然后：

```powershell
git remote add origin https://github.com/<your-org>/cangji.git
git branch -M main
git push -u origin main
```

> ⚠️ 本机 GitHub 网络实测不稳：`api.github.com` 可用，但 release 直连下载曾被重置（H1 §4 #5）、WiX 工具直连中断（H5 §1.3）。推送失败时换网络/稍后重试，如实记录，不要反复硬试。

---

## 2. 打 tag 并创建 GitHub Release（v0.1.0）

### 2.1 待上传的三个产物（绝对路径 + 字节数，H5 §1.1 实测，A6 复测一致）

| 产物 | 绝对路径 | 字节数 |
|---|---|---|
| Windows NSIS 安装包（**主推**） | `E:\仓库管理\src-tauri\target\release\bundle\nsis\CangJi_0.1.0_x64-setup.exe` | 2,178,866 |
| Windows MSI 安装包（en-US） | `E:\仓库管理\src-tauri\target\release\bundle\msi\CangJi_0.1.0_x64_en-US.msi` | 2,924,544 |
| Android APK（universal） | 构建产物 `…\app-universal-release-unsigned.apk` 27,879,549 → 签名后 `…\app-universal-release.apk` | **27,942,424（已签名，Release 实际发布此文件）** |

### 2.2 打 tag

```powershell
cd e:\仓库管理
git tag -a v0.1.0 -m "v0.1.0 首发：Windows(NSIS/MSI) + Android APK"
git push origin v0.1.0
```

### 2.3 创建 Release 并上传产物

方式一（gh CLI）：

```powershell
gh release create v0.1.0 `
  "E:\仓库管理\src-tauri\target\release\bundle\nsis\CangJi_0.1.0_x64-setup.exe#CangJi_0.1.0_x64-setup.exe（Windows 主推）" `
  "E:\仓库管理\src-tauri\target\release\bundle\msi\CangJi_0.1.0_x64_en-US.msi#CangJi_0.1.0_x64_en-US.msi（Windows MSI，界面英文属 locale 正常现象）" `
  "E:\仓库管理\src-tauri\gen\android\app\build\outputs\apk\universal\release\app-universal-release-unsigned.apk#app-universal-release-unsigned.apk（Android，未签名，需自行签名后安装）" `
  --title "仓记 CangJi v0.1.0" `
  --notes-file e:\仓库管理\CHANGELOG.md
```

方式二（网页）：Releases → Draft a new release → tag `v0.1.0` → 标题「仓记 CangJi v0.1.0」→ 正文粘贴 CHANGELOG 的 `0.1.0` 段 → Attach 三个产物。

**Release 正文必须包含的如实说明**（与 README/CHANGELOG 一致）：

1. APK 已签名（v2/v3，自签名证书），可直接安装到 Android 7.0+；从源码重新构建出的产物仍为 unsigned，需按 §3 签名；
2. MSI 文件名 `en-US` 为构建机 locale 所致，安装界面英文；
3. 22 项人工验收 checklist 尚未全部执行（见 `docs/handoffs/H5-测试打包-交接与验收单.md` 附录 A）。

---

## 3. APK 签名 keystore 生成步骤（已于 2026-10-19 执行完成；下列模板留作后续版本复用）

> 前提：`.gitignore` 已覆盖 `*.keystore`、`*.jks`、`keystore.properties`（`gen/android/.gitignore` 另覆盖 `key.properties`），**密钥与密码严禁提交**。生成后 `git status` 确认未被跟踪。

### 3.1 生成 keystore（keytool 命令模板，JDK 17 自带）

```powershell
keytool -genkeypair -v `
  -keystore e:\仓库管理\cangji.keystore `
  -alias cangji `
  -keyalg RSA -keysize 2048 `
  -validity 10000 `
  -keypass <至少6位密码，勿用引号包中文> `
  -storepass <至少6位密码>
```

### 3.2 对产物 APK 签名（apksigner 模板，推荐——不改动 `gen/android` 生成物，契约 1.4 禁手改）

```powershell
$buildTools = "$env:ANDROID_HOME\build-tools\35.0.0"   # 实测 build-tools;35.0.0
$apkIn  = "E:\仓库管理\src-tauri\gen\android\app\build\outputs\apk\universal\release\app-universal-release-unsigned.apk"
$apkOut = "E:\仓库管理\src-tauri\gen\android\app\build\outputs\apk\universal\release\app-universal-release.apk"

& "$buildTools\zipalign.exe" -p 4 $apkIn $apkOut
& "$buildTools\apksigner.bat" sign --ks e:\仓库管理\cangji.keystore --ks-key-alias cangji $apkOut
& "$buildTools\apksigner.bat" verify $apkOut
```

（或按 [Tauri 官方 Android 签名文档](https://v2.tauri.app/distribute/sign/android/) 配置。）上述路径**已于 2026-10-19 实测通过**，见 §3.4。

### 3.3 验证

真机 `adb install -r $apkOut` 或直接安装，确认能建库启动（对应 H5 checklist F1/F2）。

### 3.4 执行记录（2026-10-19 实测）

keystore 生成在**仓库外**，避免任何误提交风险：

```powershell
# 1) 生成（32 位随机密码，不入库；商店密码与 keystore 必须一起离线备份）
keytool -genkeypair -v -keystore "C:\Users\mac\.cangji\cangji-release.keystore" `
  -alias cangji -keyalg RSA -keysize 2048 -validity 10000 `
  -storepass <32位随机密码> -keypass <同上> `
  -dname "CN=CangJi, OU=Mobile, O=CangJi Contributors, L=Unknown, ST=Unknown, C=CN"

# 2) 对齐 + 签名（apksigner 0.9 / build-tools 35.0.0）
$bt = "C:\Android\Sdk\build-tools\35.0.0"
& "$bt\zipalign.exe" -p -f 4 $apkIn "<仓库外临时目录>\aligned-tmp.apk"
& "$bt\apksigner.bat" sign --ks "C:\Users\mac\.cangji\cangji-release.keystore" `
  --ks-key-alias cangji --ks-pass "pass:<密码>" --key-pass "pass:<密码>" --out $apkOut "<临时对齐包>"

# 3) 验证 → v2/v3 均为 true
& "$bt\apksigner.bat" verify --verbose --print-certs $apkOut
```

结果：`app-universal-release.apk` **27,942,424 B**；v1=false / **v2=true / v3=true**（minSdk 24+ 足够）；签名证书 SHA-256 `36b1e6089c861daff7d6bcba588abf515143a282b8808d6b7a6c45a73d321bab`。

凭据与 keystore 同存 `C:\Users\mac\.cangji\`（`KEYSTORE-CREDENTIALS.txt` 含别名/密码/指纹）。**丢失即无法给已安装用户推送同签名升级包**，务必离线备份。

补充说明：`keypass = storepass`（JDK17 默认 PKCS12 密钥库要求一致）；`-dname` 为自签名证书主题，非实名，仅供 Android 校验签名一致性。

---

## 4. 后续发布者注意事项（本机实测环境，换机复现必读）

1. **GitHub 网络不稳**：本机 `api.github.com` 可用，但 github.com 直连下载/推送多次中断（H1 §4 #5、H5 §1.3）。gh/直连失败先换网络重试；如需下载 GitHub 上的工具，走镜像前缀（见第 3 条）。
2. **WiX 工具（MSI 构建）直连 GitHub 中断**（H5 §1.3 #4）：
   ```powershell
   $env:TAURI_BUNDLER_TOOLS_GITHUB_MIRROR = "https://ghfast.top/https://github.com"
   npm run tauri build    # 下载后 tauri 仍做官方哈希校验
   ```
3. **gradle wrapper 下载 10s 超时 / 仅 60KB/s**（H5 §1.3 #2）：从腾讯镜像手动下载 gradle 分发包（实测 9.6.1），放入 `%USERPROFILE%\.gradle\wrapper\dists\gradle-9.6.1-bin\<hash>\` 并补 `.ok` 标记。
4. **仓库路径含中文**（`E:\仓库管理`，H5 踩坑 #3）：用户级 `%USERPROFILE%\.gradle\gradle.properties` 写 `android.overridePathCheck=true`（另加「IPv4 优先」配置），不改 `gen/android` 生成物。
5. **Windows 构建 Android 需开发者模式**（H5 踩坑 #1）：tauri 软链 `.so` 进 jniLibs 需要符号链接权限（`AllowDevelopmentWithoutDevLicense=1`、`AllowAllTrustedApps=1`）。
6. **环境侧配置均在仓库外**（开发者模式、用户 gradle.properties、wrapper 缓存、镜像 env），换机需按 H5 §1.3 重做；仓库零污染。
7. **版本号一致性**：升版本时同步改 `package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`（如适用）、CHANGELOG、README 徽章与产物文件名，再打包、打 tag。

---

## 5. 回滚 / 撤下

- 发错版：GitHub Release 页面 Delete release（tag 可保留或一并删除）→ 修正后重建；
- 本地产物重出：删 `src-tauri\target\release\bundle\` 与 `gen\android\app\build\outputs\` 下对应文件后重新 `npm run tauri build` / `npx tauri android build --apk`。
