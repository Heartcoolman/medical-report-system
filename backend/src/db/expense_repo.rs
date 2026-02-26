use crate::error::AppError;
use crate::models::{
    DailyExpense, DailyExpenseDetail, DailyExpenseSummary, ExpenseCategory, ExpenseItem,
};
use rusqlite::{params, OptionalExtension};

use super::helpers::*;
use super::Database;

impl Database {
    pub fn create_expense(
        &self,
        expense: &DailyExpense,
        items: &[ExpenseItem],
    ) -> Result<(), AppError> {
        self.with_conn(|conn| {
            let tx = conn.transaction()?;
            tx.execute(
                "INSERT INTO daily_expenses (id, patient_id, expense_date, total_amount, drug_analysis, treatment_analysis, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    expense.id,
                    expense.patient_id,
                    expense.expense_date,
                    expense.total_amount,
                    expense.drug_analysis,
                    expense.treatment_analysis,
                    expense.created_at
                ],
            )?;

            {
                let mut stmt = tx.prepare(
                    "INSERT INTO expense_items (id, expense_id, name, category, quantity, amount, note)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                )?;
                for item in items {
                    stmt.execute(params![
                        item.id,
                        expense.id,
                        item.name,
                        category_to_db(&item.category),
                        item.quantity,
                        item.amount,
                        item.note,
                    ])?;
                }
            }

            tx.commit()?;
            Ok(())
        })
    }

    pub fn list_expenses_by_patient(
        &self,
        patient_id: &str,
    ) -> Result<Vec<DailyExpenseSummary>, AppError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, patient_id, expense_date, total_amount, drug_analysis,
                        treatment_analysis, created_at
                 FROM daily_expenses
                 WHERE patient_id = ?1
                 ORDER BY expense_date DESC, id DESC",
            )?;

            let expense_rows = stmt.query_map([patient_id], |row| {
                Ok(DailyExpense {
                    id: row.get(0)?,
                    patient_id: row.get(1)?,
                    expense_date: row.get(2)?,
                    total_amount: row.get(3)?,
                    drug_analysis: row.get(4)?,
                    treatment_analysis: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })?;

            let mut summaries = Vec::new();
            let mut item_stmt = conn.prepare(
                "SELECT id, expense_id, name, category, quantity, amount, note
                 FROM expense_items WHERE expense_id = ?1 ORDER BY id",
            )?;

            for expense in expense_rows {
                let expense = expense?;
                let item_rows = item_stmt.query_map([&expense.id], |row| {
                    Ok(ExpenseItem {
                        id: row.get(0)?,
                        expense_id: row.get(1)?,
                        name: row.get(2)?,
                        category: parse_category(&row.get::<_, String>(3)?),
                        quantity: row.get(4)?,
                        amount: row.get(5)?,
                        note: row.get(6)?,
                    })
                })?;

                let mut item_count = 0usize;
                let mut drug_count = 0usize;
                let mut test_count = 0usize;
                let mut treatment_count = 0usize;

                for item in item_rows {
                    item_count += 1;
                    match item?.category {
                        ExpenseCategory::Drug => drug_count += 1,
                        ExpenseCategory::Test => test_count += 1,
                        ExpenseCategory::Treatment => treatment_count += 1,
                        _ => {}
                    }
                }

                summaries.push(DailyExpenseSummary {
                    expense,
                    item_count,
                    drug_count,
                    test_count,
                    treatment_count,
                });
            }

            Ok(summaries)
        })
    }

    pub fn get_expense_detail(&self, id: &str) -> Result<Option<DailyExpenseDetail>, AppError> {
        self.with_conn(|conn| {
            let expense = conn
                .query_row(
                    "SELECT id, patient_id, expense_date, total_amount, drug_analysis,
                            treatment_analysis, created_at
                     FROM daily_expenses WHERE id = ?1",
                    [id],
                    |row| {
                        Ok(DailyExpense {
                            id: row.get(0)?,
                            patient_id: row.get(1)?,
                            expense_date: row.get(2)?,
                            total_amount: row.get(3)?,
                            drug_analysis: row.get(4)?,
                            treatment_analysis: row.get(5)?,
                            created_at: row.get(6)?,
                        })
                    },
                )
                .optional()?;

            let Some(expense) = expense else {
                return Ok(None);
            };

            let mut stmt = conn.prepare(
                "SELECT id, expense_id, name, category, quantity, amount, note
                 FROM expense_items WHERE expense_id = ?1 ORDER BY id",
            )?;
            let rows = stmt.query_map([id], |row| {
                Ok(ExpenseItem {
                    id: row.get(0)?,
                    expense_id: row.get(1)?,
                    name: row.get(2)?,
                    category: parse_category(&row.get::<_, String>(3)?),
                    quantity: row.get(4)?,
                    amount: row.get(5)?,
                    note: row.get(6)?,
                })
            })?;
            let items = rows.collect::<rusqlite::Result<Vec<_>>>()?;

            Ok(Some(DailyExpenseDetail { expense, items }))
        })
    }

    pub fn list_detected_drugs(&self, patient_id: &str) -> Result<Vec<crate::models::DetectedDrug>, AppError> {
        use std::collections::HashMap;
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT ei.name, ei.quantity, de.expense_date
                 FROM expense_items ei
                 JOIN daily_expenses de ON ei.expense_id = de.id
                 WHERE de.patient_id = ?1 AND ei.category = 'drug'
                 ORDER BY de.expense_date ASC",
            )?;
            let rows = stmt.query_map([patient_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?;

            let mut drug_map: HashMap<String, (Vec<String>, HashMap<String, usize>)> = HashMap::new();
            for row in rows {
                let (name, quantity, date) = row?;
                let entry = drug_map.entry(name).or_insert_with(|| (Vec::new(), HashMap::new()));
                if entry.0.last().map_or(true, |d| d != &date) {
                    entry.0.push(date);
                }
                if !quantity.is_empty() {
                    *entry.1.entry(quantity).or_insert(0) += 1;
                }
            }

            let mut result: Vec<crate::models::DetectedDrug> = drug_map
                .into_iter()
                .map(|(name, (dates, qty_map))| {
                    let typical_quantity = qty_map
                        .into_iter()
                        .max_by_key(|(_, count)| *count)
                        .map(|(q, _)| q)
                        .unwrap_or_default();
                    crate::models::DetectedDrug {
                        name,
                        first_date: dates.first().cloned().unwrap_or_default(),
                        last_date: dates.last().cloned().unwrap_or_default(),
                        occurrence_count: dates.len(),
                        typical_quantity,
                        dates,
                    }
                })
                .collect();

            result.sort_by(|a, b| b.occurrence_count.cmp(&a.occurrence_count).then(b.last_date.cmp(&a.last_date)));
            Ok(result)
        })
    }

    pub fn delete_expense(&self, id: &str) -> Result<(), AppError> {
        self.with_conn(|conn| {
            let exists: Option<String> = conn
                .query_row("SELECT id FROM daily_expenses WHERE id = ?1", [id], |row| {
                    row.get(0)
                })
                .optional()?;
            if exists.is_none() {
                return Err(AppError::NotFound("消费记录不存在".to_string()));
            }

            let tx = conn.transaction()?;
            tx.execute("DELETE FROM expense_items WHERE expense_id = ?1", [id])?;
            tx.execute("DELETE FROM daily_expenses WHERE id = ?1", [id])?;
            tx.commit()?;
            Ok(())
        })
    }
}
