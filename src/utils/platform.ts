/**
 * 平台差异封装（契约 4.9）：
 * - 桌面：导出类操作先 dialog 选路径，成功后 opener 打开所在位置；
 * - Android：不弹 dialog，直接传 undefined 走后端缺省路径（库同级 exports/）。
 */
import { revealItemInDir } from "@tauri-apps/plugin-opener";

/** 是否 Android（Tauri 移动端 WebView UA 含 Android）。 */
export function isAndroid(): boolean {
  return typeof navigator !== "undefined" && /Android/i.test(navigator.userAgent);
}

/**
 * 导出成功后打开文件所在位置（桌面；Android 跳过）。
 * 打开失败不阻断主流程——文件已落盘，成功 toast 已给出路径。
 */
export async function revealInFolder(path: string): Promise<void> {
  if (isAndroid()) return;
  try {
    await revealItemInDir(path);
  } catch {
    /* 忽略：仅影响"打开位置"的便利操作 */
  }
}
