use crate::error::AppError;
use crate::models::TestItem;
use rusqlite::{params, OptionalExtension};
use std::collections::HashMap;

use super::helpers::*;
use super::Database;

impl Database {
    pub fn create_test_item(&self, item: &TestItem) -> Result<(), AppError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO test_items (id, report_id, name, value, unit, reference_range, status, canonical_name)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    item.id,
                    item.report_id,
                    item.name,
                    item.value,
                    item.unit,
                    item.reference_range,
                    status_to_db(&item.status),
                    item.canonical_name
                ],
            )?;
            Ok(())
        })
    }

    pub fn get_test_items_by_report(&self, report_id: &str) -> Result<Vec<TestItem>, AppError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, report_id, name, value, unit, reference_range, status, canonical_name
                 FROM test_items
                 WHERE report_id = ?1
                 ORDER BY id",
            )?;
            let rows = stmt.query_map([report_id], |row| {
                Ok(TestItem {
                    id: row.get(0)?,
                    report_id: row.get(1)?,
                    name: row.get(2)?,
                    value: row.get(3)?,
                    unit: row.get(4)?,
                    reference_range: row.get(5)?,
                    status: parse_status(&row.get::<_, String>(6)?),
                    canonical_name: row.get(7)?,
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    pub fn get_test_items_by_report_ids(
        &self,
        report_ids: &[String],
    ) -> Result<HashMap<String, Vec<TestItem>>, AppError> {
        if report_ids.is_empty() {
            return Ok(HashMap::new());
        }

        self.with_conn(|conn| {
            let placeholders = (1..=report_ids.len())
                .map(|idx| format!("?{}", idx))
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "SELECT id, report_id, name, value, unit, reference_range, status, canonical_name
                 FROM test_items
                 WHERE report_id IN ({})
                 ORDER BY report_id ASC, id ASC",
                placeholders,
            );
            let mut stmt = conn.prepare(&sql)?;
            let mut items_by_report: HashMap<String, Vec<TestItem>> = HashMap::new();
            let rows = stmt.query_map(rusqlite::params_from_iter(report_ids.iter()), |row| {
                Ok(TestItem {
                    id: row.get(0)?,
                    report_id: row.get(1)?,
                    name: row.get(2)?,
                    value: row.get(3)?,
                    unit: row.get(4)?,
                    reference_range: row.get(5)?,
                    status: parse_status(&row.get::<_, String>(6)?),
                    canonical_name: row.get(7)?,
                })
            })?;

            for row in rows {
                let item = row?;
                items_by_report
                    .entry(item.report_id.clone())
                    .or_default()
                    .push(item);
            }

            Ok(items_by_report)
        })
    }

    pub fn get_test_item(&self, id: &str) -> Result<Option<TestItem>, AppError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, report_id, name, value, unit, reference_range, status, canonical_name
                 FROM test_items
                 WHERE id = ?1",
                [id],
                |row| {
                    Ok(TestItem {
                        id: row.get(0)?,
                        report_id: row.get(1)?,
                        name: row.get(2)?,
                        value: row.get(3)?,
                        unit: row.get(4)?,
                        reference_range: row.get(5)?,
                        status: parse_status(&row.get::<_, String>(6)?),
                        canonical_name: row.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(AppError::from)
        })
    }

    pub fn update_test_item(&self, item: &TestItem) -> Result<(), AppError> {
        self.with_conn(|conn| {
            let affected = conn.execute(
                "UPDATE test_items
                 SET report_id = ?2, name = ?3, value = ?4, unit = ?5, reference_range = ?6, status = ?7, canonical_name = ?8
                 WHERE id = ?1",
                params![
                    item.id,
                    item.report_id,
                    item.name,
                    item.value,
                    item.unit,
                    item.reference_range,
                    status_to_db(&item.status),
                    item.canonical_name,
                ],
            )?;
            if affected == 0 {
                return Err(AppError::test_item_not_found());
            }
            Ok(())
        })
    }

    pub fn delete_test_item(&self, id: &str) -> Result<(), AppError> {
        self.with_conn(|conn| {
            let existing: Option<String> = conn
                .query_row("SELECT id FROM test_items WHERE id = ?1", [id], |row| {
                    row.get(0)
                })
                .optional()?;
            if existing.is_none() {
                return Err(AppError::test_item_not_found());
            }
            conn.execute("DELETE FROM test_items WHERE id = ?1", [id])?;
            Ok(())
        })
    }

    /// Return all items needed by normalization backfill.
    pub fn list_test_items_for_normalization(&self) -> Result<Vec<(TestItem, String)>, AppError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT t.id, t.report_id, t.name, t.value, t.unit, t.reference_range, t.status, t.canonical_name,
                        r.report_type
                 FROM test_items t
                 INNER JOIN reports r ON t.report_id = r.id
                 ORDER BY t.id",
            )?;

            let rows = stmt.query_map([], |row| {
                let item = TestItem {
                    id: row.get(0)?,
                    report_id: row.get(1)?,
                    name: row.get(2)?,
                    value: row.get(3)?,
                    unit: row.get(4)?,
                    reference_range: row.get(5)?,
                    status: parse_status(&row.get::<_, String>(6)?),
                    canonical_name: row.get(7)?,
                };
                let report_type: String = row.get(8)?;
                Ok((item, report_type))
            })?;

            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    pub fn update_test_item_canonical_names(
        &self,
        updates: Vec<(String, String)>,
    ) -> Result<usize, AppError> {
        self.with_conn(|conn| {
            let tx = conn.transaction()?;
            let mut updated = 0usize;
            {
                let mut stmt = tx.prepare(
                    "UPDATE test_items SET canonical_name = ?1 WHERE id = ?2 AND canonical_name <> ?1",
                )?;
                for (id, canonical) in updates {
                    if stmt.execute([canonical.as_str(), id.as_str()])? > 0 {
                        updated += 1;
                    }
                }
            }
            tx.commit()?;
            Ok(updated)
        })
    }
}
