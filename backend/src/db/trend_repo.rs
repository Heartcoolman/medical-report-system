use crate::error::AppError;
use crate::models::{TestItem, TrendItemInfo, TrendPoint};
use std::collections::{HashMap, HashSet};

use super::helpers::*;
use super::Database;

impl Database {
    pub fn get_trends(
        &self,
        patient_id: &str,
        item_name: &str,
        report_type: Option<&str>,
    ) -> Result<Vec<TrendPoint>, AppError> {
        let reports = self.list_reports_by_patient(patient_id)?;
        let target_name_key = normalize_trend_item_name(item_name);

        self.with_conn(|conn| {
            let mut name_cache: HashMap<String, String> = HashMap::new();
            let mut seen_dates: HashSet<String> = HashSet::new();
            let mut points = Vec::new();

            let mut item_stmt = conn.prepare(
                "SELECT id, report_id, name, value, unit, reference_range, status, canonical_name
                 FROM test_items WHERE report_id = ?1 ORDER BY id",
            )?;

            for report in reports {
                if let Some(rt) = report_type {
                    if !report.report_type.starts_with(rt) {
                        continue;
                    }
                }

                let effective_date = if report.sample_date.is_empty() {
                    report.report_date.clone()
                } else {
                    report.sample_date.clone()
                };
                if seen_dates.contains(&effective_date) {
                    continue;
                }

                let rows = item_stmt.query_map([&report.id], |row| {
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

                for item in rows {
                    let item = item?;
                    let effective_name = if item.canonical_name.is_empty() {
                        item.name
                    } else {
                        item.canonical_name
                    };
                    let candidate_name_key = name_cache
                        .entry(effective_name.clone())
                        .or_insert_with(|| normalize_trend_item_name(&effective_name))
                        .clone();
                    if candidate_name_key != target_name_key {
                        continue;
                    }

                    points.push(TrendPoint {
                        report_date: report.report_date,
                        sample_date: report.sample_date,
                        value: item.value,
                        unit: item.unit,
                        status: item.status,
                        reference_range: item.reference_range,
                    });
                    seen_dates.insert(effective_date);
                    break;
                }
            }

            points.sort_by(|a, b| {
                let da = if a.sample_date.is_empty() {
                    &a.report_date
                } else {
                    &a.sample_date
                };
                let db = if b.sample_date.is_empty() {
                    &b.report_date
                } else {
                    &b.sample_date
                };
                da.cmp(db)
            });
            Ok(points)
        })
    }

    pub fn list_trend_items_by_patient(
        &self,
        patient_id: &str,
    ) -> Result<Vec<TrendItemInfo>, AppError> {
        let report_ids = self.list_report_ids_by_patient(patient_id)?;
        if report_ids.is_empty() {
            return Ok(Vec::new());
        }

        self.with_conn(|conn| {
            // Optimized: single batch query for all report info instead of N individual queries
            let mut report_types = Vec::with_capacity(report_ids.len());
            let mut report_info: HashMap<String, (String, String)> = HashMap::new();

            {
                let mut stmt = conn.prepare(
                    "SELECT id, report_type, report_date, sample_date FROM reports WHERE patient_id = ?1 ORDER BY report_date ASC, id ASC",
                )?;
                let rows = stmt.query_map([patient_id], |row| {
                    let id: String = row.get(0)?;
                    let report_type: String = row.get(1)?;
                    let report_date: String = row.get(2)?;
                    let sample_date: String = row.get(3)?;
                    Ok((id, report_type, report_date, sample_date))
                })?;
                for row in rows {
                    let (id, report_type, report_date, sample_date) = row?;
                    report_types.push(report_type.clone());
                    report_info.insert(
                        id,
                        (report_type, if sample_date.is_empty() { report_date } else { sample_date }),
                    );
                }
            }

            let category_map = compute_report_categories(&report_types);
            let mut data_points: Vec<(String, String, String)> = Vec::new();
            let mut seen = HashSet::new();

            let mut item_stmt = conn.prepare(
                "SELECT report_id, name, canonical_name
                 FROM test_items
                 WHERE report_id = ?1
                 ORDER BY id",
            )?;

            for rid in &report_ids {
                let (report_type, trend_date) = report_info.get(rid).cloned().unwrap_or_default();
                let category = category_map
                    .get(&report_type)
                    .cloned()
                    .unwrap_or_else(|| report_type.clone());

                let rows = item_stmt.query_map([rid], |row| {
                    let name: String = row.get(1)?;
                    let canonical_name: String = row.get(2)?;
                    let effective = if canonical_name.is_empty() {
                        normalize_trend_item_name(&name)
                    } else {
                        normalize_trend_item_name(&canonical_name)
                    };
                    Ok((rid.clone(), effective))
                })?;

                for row in rows {
                    let (_report, effective_name) = row?;
                    let key = (category.clone(), trend_date.clone(), effective_name.clone());
                    if seen.insert(key) {
                        data_points.push((category.clone(), trend_date.clone(), effective_name));
                    }
                }
            }

            let mut counts: HashMap<(String, String), HashSet<String>> = HashMap::new();
            for (category, date, item_name) in data_points {
                counts
                    .entry((category, item_name))
                    .or_default()
                    .insert(date);
            }

            let mut result: Vec<TrendItemInfo> = counts
                .into_iter()
                .map(|((report_type, item_name), dates)| TrendItemInfo {
                    report_type,
                    item_name,
                    count: dates.len(),
                })
                .collect();

            result.sort_by(|a, b| {
                a.report_type
                    .cmp(&b.report_type)
                    .then(b.count.cmp(&a.count))
                    .then(a.item_name.cmp(&b.item_name))
            });

            Ok(result)
        })
    }
}
