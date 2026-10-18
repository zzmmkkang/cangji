//! PackFile 结构 + 校验规则（契约 5.2 / 5.3）。【A4 实现】：
//! 导出侧：四表全量（含已归档）→ 规范 JSON + SHA-256 封印；
//! 导入侧：7 条前置校验（规则 1 在 `parse`，规则 2–6 在 `validate`，规则 7 在导入命令）
//! 任一失败即拒绝、不改库；magic 错按契约 7.5 计 `E_PACK_FORMAT`、
//! schemaVersion≠1 按 7.5 计 `E_PACK_VERSION`（对 5.3 规则 2 的错误码按 7.5 细化）。

use std::collections::HashSet;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::commands::items::Item;
use crate::commands::pack::PackCounts;
use crate::commands::partners::Partner;
use crate::commands::operators::Operator;
use crate::commands::transactions::Transaction;
use crate::error::{CommandError, ErrorCode};

use super::checksum::sha256_hex;

/// 固定标识（契约 5.2）。
pub const MAGIC: &str = "CANGJI_PACK";
/// 包格式版本：v1 只接受 1（契约 5.2/5.3 规则 2）。
pub const SCHEMA_VERSION: u32 = 1;

/// 包内数据区（契约 5.2 data，四表全量）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackData {
    pub items: Vec<Item>,
    pub partners: Vec<Partner>,
    pub operators: Vec<Operator>,
    pub transactions: Vec<Transaction>,
}

/// .cjpack 文件结构（契约 5.2）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackFile {
    pub magic: String,
    pub schema_version: u32,
    pub app_version: String,
    pub exported_device_id: String,
    pub exported_device_name: String,
    pub exported_at: String,
    pub counts: PackCounts,
    pub data: PackData,
    pub checksum: Option<String>,
}

fn pack_format(message: impl Into<String>) -> CommandError {
    CommandError::new(ErrorCode::PackFormat, message)
}

fn is_uuid(s: &str) -> bool {
    uuid::Uuid::parse_str(s).is_ok()
}

fn is_rfc3339(s: &str) -> bool {
    chrono::DateTime::parse_from_rfc3339(s).is_ok()
}

/// 数量口径与 DDL CHECK 一致：> 0 且最多两位小数。
fn is_valid_quantity(q: f64) -> bool {
    q.is_finite() && q > 0.0 && (q * 100.0 - (q * 100.0).round()).abs() <= 1e-6
}

impl PackFile {
    /// 从本库构建全量数据包：四表全量（含已归档档案），counts 按实际行数写入（契约 5.2）。
    pub fn build(
        conn: &Connection,
        app_version: &str,
        device_id: &str,
        device_name: &str,
    ) -> Result<Self, CommandError> {
        let items = crate::repo::items::list(conn, None, true)?;
        let partners = crate::repo::partners::list(conn, None, true)?;
        let operators = crate::repo::operators::list(conn)?;
        let transactions = crate::repo::transactions::all(conn)?;
        let counts = PackCounts {
            items: items.len() as u64,
            partners: partners.len() as u64,
            operators: operators.len() as u64,
            transactions: transactions.len() as u64,
        };
        Ok(Self {
            magic: MAGIC.to_string(),
            schema_version: SCHEMA_VERSION,
            app_version: app_version.to_string(),
            exported_device_id: device_id.to_string(),
            exported_device_name: device_name.to_string(),
            exported_at: crate::db::now_rfc3339(),
            counts,
            data: PackData {
                items,
                partners,
                operators,
                transactions,
            },
            checksum: None,
        })
    }

    /// 规范 JSON 串：`checksum` 置 `None` 后序列化（契约 5.2 校验和算法）。
    fn canonical_json(&self) -> Result<String, CommandError> {
        let mut normalized = self.clone();
        normalized.checksum = None;
        serde_json::to_string(&normalized)
            .map_err(|e| CommandError::new(ErrorCode::Internal, format!("数据包规范化序列化失败：{e}")))
    }

    /// 计算并写回校验和（导出前"封印"，契约 5.2）。
    pub fn seal(mut self) -> Result<Self, CommandError> {
        let sum = sha256_hex(&self.canonical_json()?);
        self.checksum = Some(sum);
        Ok(self)
    }

    /// 校验和重算比对（契约 5.3 规则 3；不一致由调用方按 `E_CHECKSUM` 拒绝）。
    pub fn verify_checksum(&self) -> Result<bool, CommandError> {
        let expected = sha256_hex(&self.canonical_json()?);
        Ok(self.checksum.as_deref() == Some(expected.as_str()))
    }

    /// 规则 1：文件字节 → 合法 UTF-8 JSON（失败 → `E_PACK_FORMAT`，不改库）。
    pub fn parse(bytes: &[u8]) -> Result<Self, CommandError> {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| pack_format("数据包不是合法的 UTF-8 文本"))?;
        serde_json::from_str(text).map_err(|e| pack_format(format!("数据包 JSON 结构非法：{e}")))
    }

    /// 导入前校验（契约 5.3 规则 2–6；任一失败即拒绝，不改库）。
    pub fn validate(&self, conn: &Connection) -> Result<(), CommandError> {
        self.validate_header()?;
        self.validate_checksum()?;
        self.validate_counts()?;
        self.validate_rows()?;
        self.validate_refs(conn)?;
        Ok(())
    }

    /// 规则 2：magic 与 schemaVersion（错误码按契约 7.5 表细分）。
    fn validate_header(&self) -> Result<(), CommandError> {
        if self.magic != MAGIC {
            return Err(pack_format(format!(
                "数据包标识非法：magic 应为 {MAGIC}"
            )));
        }
        if self.schema_version != SCHEMA_VERSION {
            let found = self.schema_version;
            return Err(CommandError::new(
                ErrorCode::PackVersion,
                format!("不支持的数据包版本 {found}（当前仅支持 v{SCHEMA_VERSION}）"),
            ));
        }
        Ok(())
    }

    /// 规则 3：校验和一致（传输损坏 → `E_CHECKSUM`）。
    fn validate_checksum(&self) -> Result<(), CommandError> {
        if !self.verify_checksum()? {
            return Err(CommandError::new(
                ErrorCode::Checksum,
                "数据包校验和不一致（文件可能已损坏或被修改）",
            ));
        }
        Ok(())
    }

    /// 规则 4：counts 与 data 各数组长度一致。
    fn validate_counts(&self) -> Result<(), CommandError> {
        let actual = PackCounts {
            items: self.data.items.len() as u64,
            partners: self.data.partners.len() as u64,
            operators: self.data.operators.len() as u64,
            transactions: self.data.transactions.len() as u64,
        };
        if self.counts != actual {
            return Err(pack_format(format!(
                "counts 与 data 行数不一致：counts={:?} data={:?}",
                self.counts, actual
            )));
        }
        Ok(())
    }

    /// 规则 5：行级校验（必填非空、UUID、枚举、数量、时间戳）。
    fn validate_rows(&self) -> Result<(), CommandError> {
        let bad = |table: &str, id: &str, field: &str, why: &str| {
            pack_format(format!("数据包行校验失败：{table}.{field} {why}（id={id}）"))
                .with_detail(serde_json::json!({ "table": table, "id": id, "field": field }))
        };

        for it in &self.data.items {
            if !is_uuid(&it.id) {
                return Err(bad("items", &it.id, "id", "不是合法 UUID"));
            }
            if it.name.trim().is_empty() {
                return Err(bad("items", &it.id, "name", "不能为空"));
            }
            if it.unit.trim().is_empty() {
                return Err(bad("items", &it.id, "unit", "不能为空"));
            }
            if !is_rfc3339(&it.created_at) {
                return Err(bad("items", &it.id, "createdAt", "不是 RFC3339 时间"));
            }
            if !is_rfc3339(&it.updated_at) {
                return Err(bad("items", &it.id, "updatedAt", "不是 RFC3339 时间"));
            }
        }

        for p in &self.data.partners {
            if !is_uuid(&p.id) {
                return Err(bad("partners", &p.id, "id", "不是合法 UUID"));
            }
            if p.name.trim().is_empty() {
                return Err(bad("partners", &p.id, "name", "不能为空"));
            }
            if !matches!(p.r#type.as_str(), "supplier" | "customer" | "both") {
                return Err(bad("partners", &p.id, "type", "枚举非法"));
            }
            if !is_rfc3339(&p.created_at) {
                return Err(bad("partners", &p.id, "createdAt", "不是 RFC3339 时间"));
            }
            if !is_rfc3339(&p.updated_at) {
                return Err(bad("partners", &p.id, "updatedAt", "不是 RFC3339 时间"));
            }
        }

        for o in &self.data.operators {
            if !is_uuid(&o.id) {
                return Err(bad("operators", &o.id, "id", "不是合法 UUID"));
            }
            if o.name.trim().is_empty() {
                return Err(bad("operators", &o.id, "name", "不能为空"));
            }
            if !is_rfc3339(&o.created_at) {
                return Err(bad("operators", &o.id, "createdAt", "不是 RFC3339 时间"));
            }
            if !is_rfc3339(&o.updated_at) {
                return Err(bad("operators", &o.id, "updatedAt", "不是 RFC3339 时间"));
            }
        }

        for t in &self.data.transactions {
            if !is_uuid(&t.id) {
                return Err(bad("transactions", &t.id, "id", "不是合法 UUID"));
            }
            if !is_uuid(&t.item_id) {
                return Err(bad("transactions", &t.id, "itemId", "不是合法 UUID"));
            }
            if !is_uuid(&t.operator_id) {
                return Err(bad("transactions", &t.id, "operatorId", "不是合法 UUID"));
            }
            if !is_valid_quantity(t.quantity) {
                return Err(bad("transactions", &t.id, "quantity", "必须大于 0 且最多两位小数"));
            }
            if !is_rfc3339(&t.happened_at) {
                return Err(bad("transactions", &t.id, "happenedAt", "不是 RFC3339 时间"));
            }
            if !is_rfc3339(&t.created_at) {
                return Err(bad("transactions", &t.id, "createdAt", "不是 RFC3339 时间"));
            }
            if !is_rfc3339(&t.updated_at) {
                return Err(bad("transactions", &t.id, "updatedAt", "不是 RFC3339 时间"));
            }
            if let Some(pid) = &t.partner_id {
                if !is_uuid(pid) {
                    return Err(bad("transactions", &t.id, "partnerId", "不是合法 UUID"));
                }
            }
            // type 枚举合法性由 serde 反序列化在规则 1 内保证（非法值 → E_PACK_FORMAT）
        }
        Ok(())
    }

    /// 规则 6：引用完整性——流水的 item/partner/operator 及 copied_from/ref_id
    /// 必须能在**包内或本库**解析到（全量包保证包内自洽；失败 → `E_PACK_FORMAT`）。
    fn validate_refs(&self, conn: &Connection) -> Result<(), CommandError> {
        fn local_ids(conn: &Connection, table: &str) -> Result<HashSet<String>, CommandError> {
            // table 仅来自本文件内的固定表名调用点
            let sql = format!("SELECT id FROM {table}");
            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| CommandError::new(ErrorCode::Db, format!("校验引用完整性失败：{e}")))?;
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(|e| CommandError::new(ErrorCode::Db, format!("校验引用完整性失败：{e}")))?;
            rows.collect::<Result<HashSet<String>, _>>()
                .map_err(|e| CommandError::new(ErrorCode::Db, format!("校验引用完整性失败：{e}")))
        }

        let mut item_ids: HashSet<String> = local_ids(conn, "items")?;
        item_ids.extend(self.data.items.iter().map(|i| i.id.clone()));
        let mut partner_ids: HashSet<String> = local_ids(conn, "partners")?;
        partner_ids.extend(self.data.partners.iter().map(|p| p.id.clone()));
        let mut operator_ids: HashSet<String> = local_ids(conn, "operators")?;
        operator_ids.extend(self.data.operators.iter().map(|o| o.id.clone()));
        let mut txn_ids: HashSet<String> = local_ids(conn, "transactions")?;
        txn_ids.extend(self.data.transactions.iter().map(|t| t.id.clone()));

        for t in &self.data.transactions {
            let missing = |field: &str, value: &str| {
                pack_format(format!(
                    "数据包引用完整性校验失败：transactions.{field} 无法在包内或本库解析（id={}，{field}={value}）",
                    t.id
                ))
                .with_detail(serde_json::json!({
                    "table": "transactions", "id": t.id, "field": field, "value": value
                }))
            };
            if !item_ids.contains(&t.item_id) {
                return Err(missing("itemId", &t.item_id));
            }
            if let Some(pid) = &t.partner_id {
                if !partner_ids.contains(pid) {
                    return Err(missing("partnerId", pid));
                }
            }
            if !operator_ids.contains(&t.operator_id) {
                return Err(missing("operatorId", &t.operator_id));
            }
            if let Some(cid) = &t.copied_from {
                if !txn_ids.contains(cid) {
                    return Err(missing("copiedFrom", cid));
                }
            }
            if let Some(rid) = &t.ref_id {
                if !txn_ids.contains(rid) {
                    return Err(missing("refId", rid));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::items::NewItem;
    use crate::commands::partners::NewPartner;
    use crate::commands::transactions::{NewTransaction, TxType};
    use crate::db::test_db;

    /// 造一个含四表数据的源库，并导出封印后的数据包。
    fn sample_pack() -> PackFile {
        let db = test_db();
        {
            let conn = db.lock();
            let item = crate::repo::items::create(
                &conn,
                &NewItem {
                    name: "苹果汁".into(),
                    code: None,
                    spec: None,
                    unit: "箱".into(),
                    note: None,
                },
            )
            .unwrap();
            let partner = crate::repo::partners::create(
                &conn,
                &NewPartner {
                    name: "老王批发部".into(),
                    r#type: "supplier".into(),
                    contact: None,
                    note: None,
                },
            )
            .unwrap();
            let op = crate::repo::operators::create(&conn, "张三").unwrap();
            crate::repo::transactions::create(
                &conn,
                &NewTransaction {
                    tx_type: TxType::In,
                    item_id: item.id,
                    quantity: 10.0,
                    partner_id: Some(partner.id),
                    operator_id: op.id,
                    remark: Some("首批".into()),
                    happened_at: Some("2026-02-01T02:00:00.000Z".into()),
                    copied_from: None,
                },
                "dev-A",
            )
            .unwrap();
        }
        let conn = db.lock();
        PackFile::build(&conn, "0.1.0", "dev-uuid-a", "设备A")
            .unwrap()
            .seal()
            .unwrap()
    }

    fn empty_conn() -> rusqlite::Connection {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&mut conn).unwrap();
        conn
    }

    /// 造包后篡改内容并**重新封印**（跳过规则 3，直达规则 4–6 的目标断言）。
    fn tampered(f: impl FnOnce(&mut PackFile)) -> PackFile {
        let mut pack = sample_pack();
        f(&mut pack);
        pack.seal().unwrap()
    }

    #[test]
    fn build_seal_roundtrip_validates_clean() {
        let pack = sample_pack();
        assert_eq!(pack.magic, MAGIC);
        assert_eq!(pack.schema_version, SCHEMA_VERSION);
        assert_eq!(pack.counts.items, 1);
        assert_eq!(pack.counts.partners, 1);
        assert_eq!(pack.counts.operators, 1);
        assert_eq!(pack.counts.transactions, 1);
        let sum = pack.checksum.clone().expect("seal 后必有校验和");
        assert!(sum.starts_with("sha256:"));

        // 序列化 → 解析 → 校验全通过（规则 1–6）
        let json = serde_json::to_string_pretty(&pack).unwrap();
        let reparsed = PackFile::parse(json.as_bytes()).unwrap();
        assert!(reparsed.verify_checksum().unwrap());
        reparsed.validate(&empty_conn()).unwrap();
    }

    #[test]
    fn rule1_rejects_non_json_and_non_utf8() {
        let err = PackFile::parse(b"not json at all").unwrap_err();
        assert_eq!(err.code, ErrorCode::PackFormat);
        let err = PackFile::parse(&[0xff, 0xfe, 0xfd]).unwrap_err();
        assert_eq!(err.code, ErrorCode::PackFormat);
    }

    #[test]
    fn rule2_bad_magic_is_format_bad_version_is_version() {
        let mut pack = sample_pack();
        pack.magic = "NOT_CANGJI".into();
        // magic 错：按契约 7.5 表归 E_PACK_FORMAT
        let err = pack.validate(&empty_conn()).unwrap_err();
        assert_eq!(err.code, ErrorCode::PackFormat);

        let mut pack = sample_pack();
        pack.schema_version = 2;
        pack.checksum = None;
        let pack = pack.seal().unwrap();
        let err = pack.validate(&empty_conn()).unwrap_err();
        assert_eq!(err.code, ErrorCode::PackVersion);
    }

    #[test]
    fn rule3_tampered_payload_fails_checksum() {
        let mut pack = sample_pack();
        // 改内容但不重算校验和 → E_CHECKSUM
        pack.data.items[0].name = "被篡改".into();
        let err = pack.validate(&empty_conn()).unwrap_err();
        assert_eq!(err.code, ErrorCode::Checksum);
    }

    #[test]
    fn rule4_counts_mismatch_rejected() {
        let pack = tampered(|p| p.counts.items = 99);
        let json = serde_json::to_string(&pack).unwrap();
        let reparsed = PackFile::parse(json.as_bytes()).unwrap();
        let err = reparsed.validate(&empty_conn()).unwrap_err();
        assert_eq!(err.code, ErrorCode::PackFormat);
        assert!(err.message.contains("counts"));
    }

    #[test]
    fn rule5_row_level_violations_rejected() {
        // 空名称
        let pack = tampered(|p| p.data.items[0].name = "  ".into());
        let err = pack.validate(&empty_conn()).unwrap_err();
        assert_eq!(err.code, ErrorCode::PackFormat);
        assert!(err.message.contains("name"));

        // 非法 UUID
        let pack = tampered(|p| p.data.items[0].id = "not-a-uuid".into());
        let err = pack.validate(&empty_conn()).unwrap_err();
        assert_eq!(err.code, ErrorCode::PackFormat);

        // 数量 0 / 超两位小数
        let pack = tampered(|p| p.data.transactions[0].quantity = 0.0);
        let err = pack.validate(&empty_conn()).unwrap_err();
        assert_eq!(err.code, ErrorCode::PackFormat);
        assert!(err.message.contains("quantity"));
        let pack = tampered(|p| p.data.transactions[0].quantity = 1.234);
        let err = pack.validate(&empty_conn()).unwrap_err();
        assert_eq!(err.code, ErrorCode::PackFormat);

        // 非法时间戳
        let pack = tampered(|p| p.data.transactions[0].happened_at = "昨天".into());
        let err = pack.validate(&empty_conn()).unwrap_err();
        assert_eq!(err.code, ErrorCode::PackFormat);

        // 非法 type 枚举（serde 层拒绝，规则 1/5 口径 → E_PACK_FORMAT）
        let json = serde_json::to_string(&sample_pack()).unwrap();
        let tampered_json = json.replace("\"type\":\"IN\"", "\"type\":\"SIDEWAYS\"");
        let err = PackFile::parse(tampered_json.as_bytes()).unwrap_err();
        assert_eq!(err.code, ErrorCode::PackFormat);
    }

    #[test]
    fn rule6_dangling_refs_rejected() {
        // 流水指向包内不存在的货物（本库也无）→ E_PACK_FORMAT
        let pack = tampered(|p| {
            p.data.transactions[0].item_id = "01900000-0000-7000-8000-000000000001".into();
        });
        let err = pack.validate(&empty_conn()).unwrap_err();
        assert_eq!(err.code, ErrorCode::PackFormat);
        assert!(err.message.contains("引用完整性"));

        // 同样的包对"本库已有该货物"的连接合法（包内或本库二选一即可）
        let conn = empty_conn();
        conn.execute(
            "INSERT INTO items (id, name, code, spec, unit, note, is_archived, created_at, updated_at)
             VALUES ('01900000-0000-7000-8000-000000000001', '本库旧货', NULL, NULL, '箱', NULL, 0,
                     '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
            [],
        )
        .unwrap();
        pack.validate(&conn).unwrap();
    }
}
