//! 统计域命令（契约 4.6）。共 2 条：get_stats / export_stats_excel。
//! Excel 导出：rust_xlsxwriter 渲染到内存 buffer → resolve_export_path 落 .xlsx。

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::commands::{file_name_of, resolve_export_path};
use crate::db::Db;
use crate::error::{CommandError, ErrorCode};

/// 统计查询入参。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsQuery {
    /// "item" | "partner"
    pub group_by: String,
    /// UTC RFC3339
    pub date_from: String,
    pub date_to: String,
    pub item_id: Option<String>,
    pub partner_id: Option<String>,
}

/// 统计汇总行。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsRow {
    pub group_key: String,
    pub group_name: String,
    pub in_qty: f64,
    pub out_qty: f64,
    pub tx_count: i64,
    pub void_count: i64,
}

/// 导出文件结果（统计导出、数据包、备份共用）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedFile {
    pub path: String,
    pub file_name: String,
    pub size_bytes: u64,
}

/// 按维度汇总时间段内进出台账。
#[tauri::command]
pub fn get_stats(
    db: State<'_, Db>,
    query: StatsQuery,
) -> Result<Vec<StatsRow>, CommandError> {
    let conn = db.lock();
    crate::repo::stats::query(&conn, &query)
}

/// 维度中文标签（Excel 标题用）。
fn dim_label(group_by: &str) -> &'static str {
    match group_by {
        "partner" => "按往来单位",
        "operator" => "按操作人",
        _ => "按货物",
    }
}

/// 渲染统计 Excel（逻辑体抽出来便于单测）：
/// 标题 + 时间范围/导出时间说明 + 表头 + 数据行 + 合计行。
pub(crate) fn render_stats_xlsx(
    query: &StatsQuery,
    rows: &[StatsRow],
) -> Result<Vec<u8>, CommandError> {
    use rust_xlsxwriter::{Color, Format, Workbook};

    let xlsx_err = |e: rust_xlsxwriter::XlsxError| {
        CommandError::new(ErrorCode::Io, format!("生成 Excel 失败：{e}"))
    };

    let title_fmt = Format::new().set_bold().set_font_size(14);
    let meta_fmt = Format::new().set_font_color(Color::RGB(0x64748B));
    let head_fmt = Format::new()
        .set_bold()
        .set_background_color(Color::RGB(0xE2E8F0));
    let num_fmt = Format::new().set_num_format("0.##");
    let total_fmt = Format::new().set_bold();

    let mut wb = Workbook::new();
    let sheet = wb.add_worksheet();

    sheet
        .merge_range(0, 0, 0, 4, &format!("仓记 CangJi 统计台账 · {}", dim_label(&query.group_by)), &title_fmt)
        .map_err(xlsx_err)?;
    sheet
        .write_string_with_format(
            1,
            0,
            format!("时间范围：{} ~ {}", query.date_from, query.date_to),
            &meta_fmt,
        )
        .map_err(xlsx_err)?;
    sheet
        .write_string_with_format(
            2,
            0,
            format!(
                "导出时间：{} · 共 {} 组",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                rows.len()
            ),
            &meta_fmt,
        )
        .map_err(xlsx_err)?;

    let headers = ["名称", "入库合计", "出库合计", "单据数", "作废数"];
    for (i, h) in headers.iter().enumerate() {
        sheet
            .write_string_with_format(4, i as u16, *h, &head_fmt)
            .map_err(xlsx_err)?;
    }

    let mut total_in = 0.0f64;
    let mut total_out = 0.0f64;
    let mut total_tx = 0i64;
    let mut total_void = 0i64;
    for (i, row) in rows.iter().enumerate() {
        let r = 5 + i as u32;
        sheet.write_string(r, 0, row.group_name.clone()).map_err(xlsx_err)?;
        sheet.write_number(r, 1, row.in_qty).map_err(xlsx_err)?;
        sheet.write_number(r, 2, row.out_qty).map_err(xlsx_err)?;
        sheet.write_number(r, 3, row.tx_count as f64).map_err(xlsx_err)?;
        sheet.write_number(r, 4, row.void_count as f64).map_err(xlsx_err)?;
        total_in += row.in_qty;
        total_out += row.out_qty;
        total_tx += row.tx_count;
        total_void += row.void_count;
    }

    // 合计行（即便空表也写，方便一眼看出"本期无数据"）
    let tr = 5 + rows.len() as u32;
    sheet
        .write_string_with_format(tr, 0, "合计", &total_fmt)
        .map_err(xlsx_err)?;
    sheet.write_number_with_format(tr, 1, total_in, &num_fmt).map_err(xlsx_err)?;
    sheet.write_number_with_format(tr, 2, total_out, &num_fmt).map_err(xlsx_err)?;
    sheet.write_number_with_format(tr, 3, total_tx as f64, &total_fmt).map_err(xlsx_err)?;
    sheet.write_number_with_format(tr, 4, total_void as f64, &total_fmt).map_err(xlsx_err)?;

    sheet.set_column_width(0, 28).map_err(xlsx_err)?;
    sheet.set_column_width(1, 12).map_err(xlsx_err)?;
    sheet.set_column_width(2, 12).map_err(xlsx_err)?;
    sheet.set_column_width(3, 10).map_err(xlsx_err)?;
    sheet.set_column_width(4, 10).map_err(xlsx_err)?;

    wb.save_to_buffer().map_err(xlsx_err)
}

/// 导出统计 Excel（契约 4.6）：查询汇总 → 渲染 xlsx → 落盘（缺省写 exports/，契约 4.9）。
pub(crate) fn export_stats(
    db: &Db,
    query: &StatsQuery,
    target_path: Option<&str>,
) -> Result<ExportedFile, CommandError> {
    let rows = {
        let conn = db.lock();
        crate::repo::stats::query(&conn, query)?
    };
    let buffer = render_stats_xlsx(query, &rows)?;
    let path = resolve_export_path(target_path, db.path(), "xlsx", "stats")?;
    std::fs::write(&path, &buffer)
        .map_err(|e| CommandError::new(ErrorCode::Io, format!("写入 Excel 失败：{e}")))?;
    let size_bytes = std::fs::metadata(&path)
        .map(|m| m.len())
        .map_err(|e| CommandError::new(ErrorCode::Io, format!("读取 Excel 文件大小失败：{e}")))?;
    log::info!(
        "导出统计 Excel：{}（{}，{} 组）",
        path.display(),
        dim_label(&query.group_by),
        rows.len()
    );
    Ok(ExportedFile {
        file_name: file_name_of(&path),
        path: path.to_string_lossy().into_owned(),
        size_bytes,
    })
}

/// 导出统计 Excel（契约 4.6；target_path 缺省时写入库同级 exports/ 目录）。
#[tauri::command]
pub fn export_stats_excel(
    db: State<'_, Db>,
    query: StatsQuery,
    target_path: Option<String>,
) -> Result<ExportedFile, CommandError> {
    export_stats(&db, &query, target_path.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::items::NewItem;
    use crate::commands::transactions::{NewTransaction, TxType};

    fn wide_range(group_by: &str) -> StatsQuery {
        StatsQuery {
            group_by: group_by.into(),
            date_from: "2000-01-01T00:00:00.000Z".into(),
            date_to: "2100-01-01T00:00:00.000Z".into(),
            item_id: None,
            partner_id: None,
        }
    }

    #[test]
    fn export_stats_excel_writes_valid_xlsx() {
        let dir = std::env::temp_dir().join(format!("cangji-xlsx-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = crate::db::Db::open(dir.join("cangji.db")).unwrap();
        {
            let conn = db.lock();
            let item = crate::repo::items::create(
                &conn,
                &NewItem { name: "螺丝".into(), code: None, spec: None, unit: "个".into(), note: None },
            )
            .unwrap();
            let op = crate::repo::operators::create(&conn, "张三").unwrap();
            crate::repo::transactions::create(
                &conn,
                &NewTransaction {
                    tx_type: TxType::In,
                    item_id: item.id,
                    quantity: 12.5,
                    partner_id: None,
                    operator_id: op.id,
                    remark: None,
                    happened_at: Some("2026-03-01T02:00:00.000Z".into()),
                    copied_from: None,
                },
                "dev",
            )
            .unwrap();
        }

        // 缺省路径 → exports/cangji-stats-*.xlsx
        let query = wide_range("item");
        let out = export_stats(&db, &query, None).unwrap();
        assert!(out.file_name.starts_with("cangji-stats-"), "{}", out.file_name);
        assert!(out.file_name.ends_with(".xlsx"), "{}", out.file_name);
        assert!(out.size_bytes > 0);
        assert!(out.path.contains("exports"));

        // OOXML 容器魔数 PK\x03\x04
        let bytes = std::fs::read(&out.path).unwrap();
        assert_eq!(&bytes[..4], b"PK\x03\x04", "xlsx 必须是 zip/OOXML 容器");

        // 显式路径扩展名校验 → E_PATH_INVALID
        let err = export_stats(&db, &query, Some("E:\\tmp\\a.csv")).unwrap_err();
        assert_eq!(err.code, ErrorCode::PathInvalid);
        // 空路径 → E_PATH_INVALID
        let err = export_stats(&db, &query, Some("   ")).unwrap_err();
        assert_eq!(err.code, ErrorCode::PathInvalid);

        // 非法维度由统计查询拦截 → E_VALIDATION（不落盘）
        let err = export_stats(&db, &wide_range("device"), None).unwrap_err();
        assert_eq!(err.code, ErrorCode::Validation);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn render_stats_xlsx_marks_empty_table_with_total_row() {
        let q = wide_range("item");
        let buffer = render_stats_xlsx(&q, &[]).unwrap();
        assert_eq!(&buffer[..4], b"PK\x03\x04");
        assert!(buffer.len() > 100);
    }
}
