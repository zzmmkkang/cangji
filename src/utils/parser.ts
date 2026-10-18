/**
 * 自然语言速记 · 规则解析（规划书 5.2，A4 交付）。
 * 纯函数零依赖：输入原始文本 + 货物/往来单位/操作人档案，输出结构化解析结果，
 * 供速记主页生成「待核对单据卡片」（规则 4）。
 *
 * 解析规则：
 * 1. 首词含 入/进/收 → 入库；含 出/发/销 → 出库（首词未命中时退化为扫描后续短词的精确方向词）
 * 2. 第一段数字 → 数量；紧跟的 ≤4 字单位一并吞掉（"20个"、"自攻钉2箱"）
 * 3. 剩余文本对货物名 / 往来单位名 / 操作人名模糊匹配：
 *    唯一命中直接带出（candidates 长度为 1）、多候选待确认、零命中入 leftover
 * 4. 「客户 / 供应商」类词 → 往来类型提示，不参与名称匹配
 *
 * 已知局限（如实约定）：
 * - 不做单位换算表：解析出的单位仅作提示，与货物档案单位不一致时由页面提示，不自动折算
 * - 无中文分词器：靠空格 + 数字切分；全粘连输入（无空格无数字）只能整体模糊匹配
 * - 方向识别是首词启发式：货物/操作人名以方向字开头（如"收音机"）且不写方向词时可能误判
 * - 数量只取第一段数字；多个数字时其余留在 leftover
 */
import type { Item, Operator, Partner, PartnerType } from "../api/types";

/** 解析所需的档案目录（页面把 store 数据传进来，parser 不发请求）。 */
export interface MemoCatalogs {
  items: Item[];
  partners: Partner[];
  operators: Operator[];
}

/** 规则解析结果（供页面组装待核对卡片）。 */
export interface MemoParse {
  /** 识别的方向；null = 未识别（页面默认 IN 并提示核对） */
  direction: "IN" | "OUT" | null;
  /** 识别的数量；null = 未识别 */
  quantity: number | null;
  /** 数量旁吞掉的单位（"20个"→"个"）；null = 无 */
  unit: string | null;
  /** 货物候选（唯一命中时 length===1，页面直接带出） */
  itemCandidates: Item[];
  /** 往来单位候选 */
  partnerCandidates: Partner[];
  /** 操作人候选 */
  operatorCandidates: Operator[];
  /** 「客户/供应商」类词命中的类型提示 */
  partnerTypeHint: PartnerType | null;
  /** 三个目录都零命中的残余文本（可整段转备注） */
  leftover: string;
  /** 解析提示（方向/数量未识别等，页面红字展示） */
  warnings: string[];
}

/** 方向词：首词按前缀命中（规则 1 的"含"，示例均为前缀）。 */
const DIR_IN_RE = /^[入进收]/;
const DIR_OUT_RE = /^[出发销]/;

/** 首词未命中时，后续短词的精确方向词表（顺序灵活的兜底，避免长词误判）。 */
const DIR_IN_WORDS = new Set([
  "入", "进", "收", "入库", "收到", "进货", "收入", "进到", "入账",
]);
const DIR_OUT_WORDS = new Set([
  "出", "发", "销", "出库", "发出", "出货", "发货", "销售", "支出",
]);

/** 往来类型关键词（规则 4 类）：命中即从匹配词中剔除。 */
const PARTNER_TYPE_RULES: { re: RegExp; type: PartnerType }[] = [
  { re: /^(客户|顾客|买方|销方)/, type: "customer" },
  { re: /^(供应商|供货商|供货|供方|厂家|厂商|卖方)/, type: "supplier" },
];

/** 数字段（首段）+ 紧随其后的非空非数字片段（单位候选）。 */
const QTY_RE = /(?<![\w.])(\d+(?:\.\d+)?)\s*([^\s\d]*)/;

/** 首尾需要剔除的标点（中英文混排速记常见）。 */
const PUNCT_RE = /^[，。、,.；;：:！!？?~—\-()[\]【】\s]+|[，。、,.；;：:！!？?~—\-()[\]【】\s]+$/g;

/** 模糊匹配打分：精确 > 前缀 > 后缀 > 包含 > 被包含。0 = 未命中。 */
function score(name: string, query: string): number {
  const n = name.trim().toLowerCase();
  const t = query.trim().toLowerCase();
  if (!n || !t) return 0;
  if (n === t) return 100;
  if (n.startsWith(t)) return 80;
  if (n.endsWith(t)) return 70;
  if (n.includes(t)) return 60;
  if (t.includes(n)) return 40;
  return 0;
}

/** 从文本中取出第一段数字与单位，返回数量、单位与剩余文本。 */
function takeQuantity(text: string): {
  quantity: number | null;
  unit: string | null;
  rest: string;
} {
  const m = QTY_RE.exec(text);
  if (!m) return { quantity: null, unit: null, rest: text };

  const number = m[1];
  const rawUnit = (m[2] ?? "").replace(/[，。、,.；;：:！!？?]+$/, "");
  // 单位最长 4 字：更长的粘连文本视为货物名的一部分（"50螺丝"只取 50）
  const unit = rawUnit.length > 0 && rawUnit.length <= 4 ? rawUnit : null;

  const start = m.index;
  let end = start + number.length;
  if (unit) {
    const ws = /^\s*/.exec(text.slice(end));
    end += (ws ? ws[0].length : 0) + unit.length;
  }
  const rest = `${text.slice(0, start)} ${text.slice(end)}`;
  return { quantity: Number(number), unit, rest };
}

/** 按目录收集候选：每个词各目录打分，取该实体的最高分，0 分剔除后排序。 */
function collectByQuery<T extends { name: string }>(
  catalog: T[],
  queries: string[],
): T[] {
  const best = new Map<T, number>();
  for (const q of queries) {
    for (const entity of catalog) {
      const s = score(entity.name, q);
      if (s > 0 && s > (best.get(entity) ?? 0)) best.set(entity, s);
    }
  }
  return [...best.entries()]
    .filter(([, s]) => s > 0)
    .sort((a, b) => b[1] - a[1] || a[0].name.length - b[0].name.length || a[0].name.localeCompare(b[0].name))
    .map(([e]) => e);
}

/**
 * 规则解析主入口。
 * 空输入返回全 null + 警告；页面据此决定卡片显隐与红字提示。
 */
export function parseMemo(raw: string, catalogs: MemoCatalogs): MemoParse {
  const warnings: string[] = [];
  const input = raw.trim();
  if (!input) {
    return {
      direction: null,
      quantity: null,
      unit: null,
      itemCandidates: [],
      partnerCandidates: [],
      operatorCandidates: [],
      partnerTypeHint: null,
      leftover: "",
      warnings: ["请输入速记内容"],
    };
  }

  const rawTokens = input
    .split(/\s+/)
    .map((t) => t.replace(PUNCT_RE, ""))
    .filter(Boolean);

  // 1. 方向：首词前缀规则；未命中再扫后续 ≤4 字的精确方向词
  let direction: "IN" | "OUT" | null = null;
  const firstToken = rawTokens[0] ?? "";
  if (DIR_IN_RE.test(firstToken)) direction = "IN";
  else if (DIR_OUT_RE.test(firstToken)) direction = "OUT";

  if (direction === null) {
    for (let i = 1; i < rawTokens.length; i++) {
      const t = rawTokens[i];
      if (t.length > 4) continue;
      if (DIR_IN_WORDS.has(t)) { direction = "IN"; break; }
      if (DIR_OUT_WORDS.has(t)) { direction = "OUT"; break; }
    }
  }
  if (direction === null) warnings.push("未识别方向（试试「入库 螺丝 50 张三」）");

  // 2. 数量 + 单位（剩余文本不含数量段）
  const { quantity, unit, rest } = takeQuantity(input);
  if (quantity === null) warnings.push("未识别数量");

  // 3. 待匹配词：剔除方向词与「客户/供应商」类关键词，类型词残留部分继续参与匹配
  let partnerTypeHint: PartnerType | null = null;
  const queries: string[] = [];
  const restTokens = rest
    .split(/\s+/)
    .map((t) => t.replace(PUNCT_RE, ""))
    .filter(Boolean);
  for (let i = 0; i < restTokens.length; i++) {
    const t = restTokens[i];
    // 精确方向词（入库/出/收到…）与首词方向前缀（≤3 字，如"进了"）不参与名称匹配
    if (DIR_IN_WORDS.has(t) || DIR_OUT_WORDS.has(t)) continue;
    if (i === 0 && direction !== null && t.length <= 3 && (DIR_IN_RE.test(t) || DIR_OUT_RE.test(t))) {
      continue;
    }
    const rule = PARTNER_TYPE_RULES.find((r) => r.re.test(t));
    if (rule) {
      partnerTypeHint ??= rule.type;
      const residue = t.replace(rule.re, "");
      if (residue) queries.push(residue); // "客户王五" → 仍匹配"王五"
      continue;
    }
    queries.push(t);
  }
  const uniqQueries = [...new Set(queries.filter(Boolean))];

  // 4. 三目录模糊匹配（仅活跃档案）
  const activeItems = catalogs.items.filter((i) => !i.isArchived);
  const activePartners = catalogs.partners.filter((p) => !p.isArchived);
  const itemCandidates = collectByQuery(activeItems, uniqQueries);
  const partnerCandidates = collectByQuery(activePartners, uniqQueries);
  const operatorCandidates = collectByQuery(catalogs.operators, uniqQueries);

  // 5. leftover：所有目录都零命中的词（页面提供 [改为备注]）
  const pool = [...activeItems, ...activePartners, ...catalogs.operators];
  const leftoverTokens = uniqQueries.filter(
    (q) => !pool.some((e) => score(e.name, q) > 0),
  );

  return {
    direction,
    quantity,
    unit,
    itemCandidates,
    partnerCandidates,
    operatorCandidates,
    partnerTypeHint,
    leftover: leftoverTokens.join(" "),
    warnings,
  };
}
