//! 流水域命令（契约 4.4）。共 3 条：create_transaction / void_transaction / list_transactions。
//! append-only 边界：repo 层仅 INSERT/SELECT；作废 = 追加 VOID 单（ref_id 指原单），
//! 不存在改写旧行的语句（契约 3.3 偏差 #1、7.4：出库不做库存拦截）。

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::Db;
use crate::error::CommandError;

/// 流水方向（契约 4.4 TxType，序列化为 "IN" | "OUT" | "VOID"）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TxType {
    In,
    Out,
    Void,
}

/// 流水明细（契约 4.4 Transaction，查询基于 v_transaction_detail 视图）；
/// Deserialize/Clone 供数据包导入（pack/model.rs 反序列化）复用。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transaction {
    pub id: String,
    pub r#type: TxType,
    pub item_id: String,
    pub item_name: String,
    pub unit: String,
    pub quantity: f64,
    pub partner_id: Option<String>,
    pub partner_name: Option<String>,
    pub operator_id: String,
    pub operator_name: String,
    pub device_id: String,
    pub remark: Option<String>,
    pub happened_at: String,
    pub copied_from: Option<String>,
    pub ref_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 新建流水入参。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewTransaction {
    #[serde(rename = "type")]
    pub tx_type: TxType,
    pub item_id: String,
    pub quantity: f64,
    pub partner_id: Option<String>,
    pub operator_id: String,
    pub remark: Option<String>,
    /// 缺省 = 当前 UTC；前端把用户选择的业务时间转 UTC 后提交（契约 7.3）。
    pub happened_at: Option<String>,
    /// 复制上次单据时传原单 id。
    pub copied_from: Option<String>,
}

/// 流水筛选（前端把本地日起止转 UTC 传 date_from / date_to）。
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TransactionFilter {
    pub item_id: Option<String>,
    pub partner_id: Option<String>,
    pub operator_id: Option<String>,
    #[serde(rename = "type")]
    pub tx_type: Option<TxType>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    /// 匹配货物名/单位名/备注。
    pub keyword: Option<String>,
    /// 默认 100，上限 500。
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// 分页结果（契约 4.4 TransactionPage）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionPage {
    pub rows: Vec<Transaction>,
    pub total: i64,
    pub limit: u32,
    pub offset: u32,
}

/// 记一笔：追加一条流水（OUT 不做库存校验，直接落库，契约 7.4 放行判定）。
#[tauri::command]
pub fn create_transaction(
    db: State<'_, Db>,
    input: NewTransaction,
) -> Result<Transaction, CommandError> {
    let conn = db.lock();
    let device_id = crate::device::device_id(&conn)?;
    crate::repo::transactions::create(&conn, &input, &device_id)
}

/// 作废：追加 type='VOID'、quantity=原单数量、ref_id=原单 id 的流水。
#[tauri::command]
pub fn void_transaction(
    db: State<'_, Db>,
    id: String,
    reason: Option<String>,
) -> Result<Transaction, CommandError> {
    let conn = db.lock();
    let device_id = crate::device::device_id(&conn)?;
    crate::repo::transactions::void(&conn, &id, reason.as_deref(), &device_id)
}

/// 流水分页查询，默认按 happened_at DESC（契约 7.7）。
#[tauri::command]
pub fn list_transactions(
    db: State<'_, Db>,
    filter: TransactionFilter,
) -> Result<TransactionPage, CommandError> {
    let conn = db.lock();
    crate::repo::transactions::list(&conn, &filter)
}
