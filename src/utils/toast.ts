/**
 * 轻量自建 toast（契约 1.3 不引重型组件库 / 7.5 错误统一提示）。
 * 错误统一读 CommandError.message（E_VALIDATION / E_DUPLICATE 等红字展示）。
 */
import { reactive } from "vue";
import { toCommandError } from "../api/commands";

export type ToastKind = "success" | "error" | "info";

export interface ToastItem {
  id: number;
  kind: ToastKind;
  text: string;
}

export const toasts = reactive<ToastItem[]>([]);

let seq = 0;

export function toast(text: string, kind: ToastKind = "info"): void {
  const id = ++seq;
  toasts.push({ id, kind, text });
  // 最多同时展示 4 条，超出丢弃最旧的
  if (toasts.length > 4) toasts.shift();
  window.setTimeout(() => {
    const i = toasts.findIndex((t) => t.id === id);
    if (i >= 0) toasts.splice(i, 1);
  }, 3500);
}

export function toastSuccess(text: string): void {
  toast(text, "success");
}

/** 命令错误统一 toast：读 message 红字展示。 */
export function toastError(e: unknown): void {
  const err = toCommandError(e);
  toast(err.message, "error");
}
