/**
 * 数量 / 时间本地化格式化（契约 7.3、3.4）。
 * 展示层：dayjs 本地化；提交层：datetime-local 本地值 → toISOString() 转 UTC。
 */
import dayjs from "dayjs";
import "dayjs/locale/zh-cn";

dayjs.locale("zh-cn");

/** UTC RFC3339 → 本地 "YYYY-MM-DD HH:mm"；空值返回占位符。 */
export function formatTime(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = dayjs(iso);
  return d.isValid() ? d.format("YYYY-MM-DD HH:mm") : iso;
}

/** UTC RFC3339 → 本地 "YYYY-MM-DD"。 */
export function formatDate(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = dayjs(iso);
  return d.isValid() ? d.format("YYYY-MM-DD") : iso;
}

/** 数量展示：最多两位小数并去掉尾随 0。 */
export function formatQty(n: number): string {
  if (!Number.isFinite(n)) return String(n);
  return String(Math.round(n * 100) / 100);
}

/** datetime-local 默认值（本地当前时间，格式 YYYY-MM-DDTHH:mm）。 */
export function localInputNow(): string {
  return dayjs().format("YYYY-MM-DDTHH:mm");
}

/** datetime-local 本地值 → UTC ISO 字符串（契约 7.3：前端转 UTC 后提交）。 */
export function toUtcIso(localValue: string): string {
  return new Date(localValue).toISOString();
}

/** 本地"今天"（YYYY-MM-DD）。 */
export function todayStr(): string {
  return dayjs().format("YYYY-MM-DD");
}

/** 本地日期起始（00:00:00.000）→ UTC ISO（用于 dateFrom）。 */
export function dayStartUtc(date: string): string {
  return new Date(`${date}T00:00:00.000`).toISOString();
}

/** 本地日期结束（23:59:59.999）→ UTC ISO（用于 dateTo）。 */
export function dayEndUtc(date: string): string {
  return new Date(`${date}T23:59:59.999`).toISOString();
}

/** 本地日期加减天数（YYYY-MM-DD）。 */
export function shiftDay(date: string, days: number): string {
  return dayjs(date).add(days, "day").format("YYYY-MM-DD");
}

/** 本地月份第一天（YYYY-MM-DD）。 */
export function monthStart(date: string): string {
  return dayjs(date).startOf("month").format("YYYY-MM-DD");
}

/** 距今天数的口语化描述（数据共账"距上次同步"）。 */
export function daysAgoText(iso: string | null | undefined): string {
  if (!iso) return "从未同步";
  const d = dayjs(iso);
  if (!d.isValid()) return "从未同步";
  const days = Math.floor((Date.now() - d.valueOf()) / 86_400_000);
  if (days <= 0) return "今天";
  if (days === 1) return "昨天";
  return `${days} 天前`;
}

/** 常见整数计量单位（整数单位只允许整数输入，契约 3.4）。 */
const INTEGER_UNITS = new Set([
  "个", "只", "箱", "瓶", "包", "袋", "件", "桶", "台", "条",
  "罐", "提", "盒", "捆", "扎", "辆", "套", "组", "本", "支",
]);

/** 该单位是否只允许整数数量。 */
export function isIntegerUnit(unit: string): boolean {
  return INTEGER_UNITS.has(unit.trim());
}

/**
 * 数量校验（与 Rust 侧 validate_quantity 同口径 + 按单位控精度）：
 * 必填 > 0、最多两位小数、整数单位只允许整数。返回错误信息，null 表示通过。
 */
export function validateQuantity(raw: string, unit: string): string | null {
  const s = raw.trim();
  if (!s) return "请输入数量";
  const n = Number(s);
  if (!Number.isFinite(n)) return "数量格式不正确";
  if (n <= 0) return "数量必须大于 0";
  if (Math.abs(n * 100 - Math.round(n * 100)) > 1e-6) return "数量最多保留两位小数";
  if (isIntegerUnit(unit) && !Number.isInteger(n)) {
    return `单位「${unit}」只允许整数`;
  }
  return null;
}
