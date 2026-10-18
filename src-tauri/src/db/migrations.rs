//! 迁移框架（契约 3.6）：
//! - 内嵌迁移数组 `&[(version, &str)]`，version 从 1 起，v1 = 契约 3.1 全量 DDL
//! - 执行器读 schema_migrations 已应用版本 → 单事务内依次执行未应用版本并写入 applied_at
//! - 失败即返回错误（启动侧 panic 中止，不静默带病运行）
//! - 禁止修改已发布的历史迁移 SQL，表结构变更必须新增迁移项

#![allow(dead_code)]

use rusqlite::{params, Connection};

use crate::db::now_rfc3339;
use crate::error::{CommandError, ErrorCode};

/// 迁移清单：v1 = 契约 3.1 DDL 全量（原样照抄，含 IF NOT EXISTS / 视图 / 索引）。
pub(crate) const MIGRATIONS: &[(u32, &str)] = &[(1, V1_DDL)];

/// 框架表（迁移执行器读取前先引导创建；与 v1 DDL 内的定义一致、幂等）。
const SCHEMA_MIGRATIONS_DDL: &str = "CREATE TABLE IF NOT EXISTS schema_migrations (
  version    INTEGER PRIMARY KEY,
  applied_at TEXT NOT NULL
);";

const V1_DDL: &str = r#"
-- 迁移记录（框架表，不入数据包）
CREATE TABLE IF NOT EXISTS schema_migrations (
  version    INTEGER PRIMARY KEY,
  applied_at TEXT NOT NULL
);

-- 设置表：键值对；device_id 等本机身份存这里，【不参与数据包合并】
CREATE TABLE IF NOT EXISTS settings (
  key        TEXT PRIMARY KEY,
  value      TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

-- 操作人表：从列表选名字，无密码；name 全局唯一（重名无业务意义）
CREATE TABLE IF NOT EXISTS operators (
  id         TEXT PRIMARY KEY,           -- UUID v7
  name       TEXT NOT NULL UNIQUE,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL               -- 统一 LWW 字段，合并按 name 归并
);

-- 货物档案（SKU）
CREATE TABLE IF NOT EXISTS items (
  id          TEXT PRIMARY KEY,          -- UUID v7
  name        TEXT NOT NULL,
  code        TEXT,                      -- 自编码，可空
  spec        TEXT,                      -- 规格
  unit        TEXT NOT NULL,             -- 计量单位：个/箱/kg…
  note        TEXT,
  is_archived INTEGER NOT NULL DEFAULT 0,-- 软删除：1=已归档（见 3.2 说明）
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL              -- 档案可改，合并 LWW 裁决字段
);

-- 往来单位（供应商/客户）
CREATE TABLE IF NOT EXISTS partners (
  id          TEXT PRIMARY KEY,          -- UUID v7
  name        TEXT NOT NULL,
  type        TEXT NOT NULL DEFAULT 'both'
              CHECK (type IN ('supplier','customer','both')),
  contact     TEXT,
  note        TEXT,
  is_archived INTEGER NOT NULL DEFAULT 0,
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL
);

-- 进出流水（核心表，append-only：应用层禁止改写旧行，见 4.4/7.6）
CREATE TABLE IF NOT EXISTS transactions (
  id          TEXT PRIMARY KEY,          -- UUID v7（时间有序）
  type        TEXT NOT NULL CHECK (type IN ('IN','OUT','VOID')),
              -- IN=入库 OUT=出库 VOID=作废单（对原单的追加式作废标记）
  item_id     TEXT NOT NULL REFERENCES items(id),
  quantity    REAL NOT NULL
              CHECK (quantity > 0 AND ROUND(quantity, 2) = quantity),
              -- 数量恒为正；VOID 单存原单数量（回冲量）；最多两位小数
  partner_id  TEXT REFERENCES partners(id),   -- 入库=供应商，出库=客户，可空
  operator_id TEXT NOT NULL REFERENCES operators(id),
  device_id   TEXT NOT NULL,             -- 产生本单的设备
  remark      TEXT,
  happened_at TEXT NOT NULL,             -- 业务时间（UTC 存储，前端本地化）
  copied_from TEXT REFERENCES transactions(id), -- 复制来源单（复制上次单据）
  ref_id      TEXT REFERENCES transactions(id), -- VOID 单指向被作废的原单
  created_at  TEXT NOT NULL,             -- 系统记录时间
  updated_at  TEXT NOT NULL              -- LWW 字段（流水合并按 UUID 去重，此字段仅保格式统一）
);

-- 索引：覆盖流水页筛选（按货物/单位/操作人/时间段/方向）与库存 VIEW 聚合
CREATE INDEX IF NOT EXISTS idx_txn_item_time    ON transactions(item_id, happened_at);
CREATE INDEX IF NOT EXISTS idx_txn_time         ON transactions(happened_at DESC);
CREATE INDEX IF NOT EXISTS idx_txn_partner      ON transactions(partner_id);
CREATE INDEX IF NOT EXISTS idx_txn_operator     ON transactions(operator_id);
CREATE INDEX IF NOT EXISTS idx_txn_type         ON transactions(type);
CREATE INDEX IF NOT EXISTS idx_items_name       ON items(name);
CREATE INDEX IF NOT EXISTS idx_items_archived   ON items(is_archived);
CREATE INDEX IF NOT EXISTS idx_partners_name    ON partners(name);
CREATE INDEX IF NOT EXISTS idx_partners_archived ON partners(is_archived);

-- 库存余额：实时推导，不落表。
-- 回冲语义（契约修订 v1.1，2026-10-17）：VOID 按被作废原单方向回冲——
-- 作废 IN 单库存减少，作废 OUT 单库存增加。
-- balance = ΣIN − ΣOUT + ΣVOID(原单为OUT) − ΣVOID(原单为IN)
CREATE VIEW IF NOT EXISTS v_inventory AS
SELECT
  i.id          AS item_id,
  i.name        AS item_name,
  i.spec        AS spec,
  i.unit        AS unit,
  i.is_archived AS is_archived,
  COALESCE(SUM(CASE WHEN t.type = 'IN'   THEN t.quantity END), 0) AS total_in,
  COALESCE(SUM(CASE WHEN t.type = 'OUT'  THEN t.quantity END), 0) AS total_out,
  COALESCE(SUM(CASE WHEN t.type = 'VOID' THEN t.quantity END), 0) AS total_voided,
  COALESCE(SUM(
    CASE
      WHEN t.type = 'IN'  THEN t.quantity
      WHEN t.type = 'OUT' THEN -t.quantity
      WHEN t.type = 'VOID' AND orig.type = 'OUT' THEN  t.quantity
      WHEN t.type = 'VOID' AND orig.type = 'IN'  THEN -t.quantity
    END
  ), 0) AS balance
FROM items i
LEFT JOIN transactions t   ON t.item_id = i.id
LEFT JOIN transactions orig ON orig.id = t.ref_id
GROUP BY i.id;

-- 流水联表视图：流水页/速记主页直接查，避免前端拼名字
CREATE VIEW IF NOT EXISTS v_transaction_detail AS
SELECT
  t.id, t.type, t.quantity, t.remark, t.happened_at, t.device_id,
  t.copied_from, t.ref_id, t.created_at, t.updated_at,
  i.id AS item_id,   i.name AS item_name,   i.unit AS unit,
  p.id AS partner_id, p.name AS partner_name,
  o.id AS operator_id, o.name AS operator_name
FROM transactions t
JOIN items i          ON i.id = t.item_id
LEFT JOIN partners p  ON p.id = t.partner_id
JOIN operators o      ON o.id = t.operator_id;

-- 冲突/合并记录表：每次导入数据包追加一行（审计 + 数据共账页显示"距上次同步"）
CREATE TABLE IF NOT EXISTS merge_logs (
  id                 TEXT PRIMARY KEY,   -- UUID v7
  imported_at        TEXT NOT NULL,
  source_device_id   TEXT NOT NULL,
  source_device_name TEXT,
  source_file        TEXT NOT NULL,
  backup_path        TEXT NOT NULL,      -- 导入前自动备份的 .db 路径
  added_transactions INTEGER NOT NULL,
  updated_items      INTEGER NOT NULL,
  updated_partners   INTEGER NOT NULL,
  new_operators      INTEGER NOT NULL,
  conflicts_json     TEXT NOT NULL DEFAULT '[]'  -- MergeConflict[] JSON
);
"#;

fn db_err(context: &str, e: &rusqlite::Error) -> CommandError {
    CommandError::new(ErrorCode::Db, format!("{context}：{e}"))
}

/// 迁移执行器：读 schema_migrations → 单事务执行全部未应用迁移 → 写 applied_at（UTC 毫秒）。
pub fn run(conn: &mut Connection) -> Result<(), CommandError> {
    conn.execute_batch(SCHEMA_MIGRATIONS_DDL)
        .map_err(|e| db_err("初始化 schema_migrations 失败", &e))?;

    let applied: Vec<u32> = {
        let mut stmt = conn
            .prepare("SELECT version FROM schema_migrations ORDER BY version")
            .map_err(|e| db_err("读取已应用迁移失败", &e))?;
        let rows = stmt
            .query_map([], |r| r.get::<_, u32>(0))
            .map_err(|e| db_err("读取已应用迁移失败", &e))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| db_err("读取已应用迁移失败", &e))?
    };

    let pending: Vec<&(u32, &str)> = MIGRATIONS
        .iter()
        .filter(|(version, _)| !applied.contains(version))
        .collect();
    if pending.is_empty() {
        return Ok(());
    }

    let tx = conn
        .transaction()
        .map_err(|e| db_err("开启迁移事务失败", &e))?;
    let mut applied_now: Vec<u32> = Vec::new();
    for &&(version, sql) in &pending {
        tx.execute_batch(sql)
            .map_err(|e| CommandError::new(ErrorCode::Db, format!("迁移 v{version} 执行失败：{e}")))?;
        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            params![version, now_rfc3339()],
        )
        .map_err(|e| db_err("写入迁移记录失败", &e))?;
        applied_now.push(version);
    }
    tx.commit().map_err(|e| db_err("提交迁移事务失败", &e))?;
    log::info!("数据库迁移完成：新应用版本 {applied_now:?}，当前 schema v{}", current_version(conn)?);
    Ok(())
}

/// 当前 schema 版本（schema_migrations 最大 version；未迁移过为 0）。
pub fn current_version(conn: &Connection) -> Result<u32, CommandError> {
    conn.query_row("SELECT COALESCE(MAX(version), 0) FROM schema_migrations", [], |r| r.get(0))
        .map_err(|e| db_err("读取 schema 版本失败", &e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_run_idempotently() {
        let mut conn = Connection::open_in_memory().unwrap();
        // 连跑两次不报错：第二次无未应用迁移
        run(&mut conn).unwrap();
        run(&mut conn).unwrap();

        assert_eq!(current_version(&conn).unwrap(), 1);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "同一版本只应写入一条 applied 记录");
        // applied_at 为 UTC RFC3339 毫秒
        let applied_at: String = conn
            .query_row("SELECT applied_at FROM schema_migrations WHERE version = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(applied_at.len(), 24, "{applied_at}");
        assert!(applied_at.ends_with('Z'));
        chrono::DateTime::parse_from_rfc3339(&applied_at).unwrap();
    }

    #[test]
    fn v1_creates_all_contract_objects() {
        let db = crate::db::test_db();
        let conn = db.lock();

        let tables = [
            "schema_migrations", "settings", "operators", "items",
            "partners", "transactions", "merge_logs",
        ];
        for t in tables {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [t],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "缺少表 {t}");
        }

        for v in ["v_inventory", "v_transaction_detail"] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'view' AND name = ?1",
                    [v],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "缺少视图 {v}");
        }

        let indexes = [
            "idx_txn_item_time", "idx_txn_time", "idx_txn_partner", "idx_txn_operator",
            "idx_txn_type", "idx_items_name", "idx_items_archived", "idx_partners_name",
            "idx_partners_archived",
        ];
        for i in indexes {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
                    [i],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "缺少索引 {i}");
        }
    }
}
