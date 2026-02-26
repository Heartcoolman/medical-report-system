use rusqlite::params;

use crate::error::AppError;
use crate::models::Medication;

use super::Database;

impl Database {
    pub fn create_medication(&self, med: &Medication) -> Result<(), AppError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO medications (id, patient_id, name, dosage, frequency, start_date, end_date, note, active, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    med.id,
                    med.patient_id,
                    med.name,
                    med.dosage,
                    med.frequency,
                    med.start_date,
                    med.end_date,
                    med.note,
                    med.active as i32,
                    med.created_at,
                ],
            )?;
            Ok(())
        })
    }

    pub fn list_medications_by_patient(&self, patient_id: &str) -> Result<Vec<Medication>, AppError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, patient_id, name, dosage, frequency, start_date, end_date, note, active, created_at
                 FROM medications WHERE patient_id = ?1 ORDER BY active DESC, start_date DESC",
            )?;
            let rows = stmt.query_map(params![patient_id], |row| {
                Ok(Medication {
                    id: row.get(0)?,
                    patient_id: row.get(1)?,
                    name: row.get(2)?,
                    dosage: row.get(3)?,
                    frequency: row.get(4)?,
                    start_date: row.get(5)?,
                    end_date: row.get(6)?,
                    note: row.get(7)?,
                    active: row.get::<_, i32>(8)? != 0,
                    created_at: row.get(9)?,
                })
            })?;
            let mut result = Vec::new();
            for r in rows {
                result.push(r?);
            }
            Ok(result)
        })
    }

    pub fn update_medication(&self, med: &Medication) -> Result<(), AppError> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE medications SET name=?1, dosage=?2, frequency=?3, start_date=?4, end_date=?5, note=?6, active=?7
                 WHERE id=?8",
                params![
                    med.name,
                    med.dosage,
                    med.frequency,
                    med.start_date,
                    med.end_date,
                    med.note,
                    med.active as i32,
                    med.id,
                ],
            )?;
            Ok(())
        })
    }

    pub fn delete_medication(&self, id: &str) -> Result<(), AppError> {
        self.with_conn(|conn| {
            conn.execute("DELETE FROM medications WHERE id=?1", params![id])?;
            Ok(())
        })
    }

    // --- User Management ---

    pub fn list_users(&self) -> Result<Vec<UserInfo>, AppError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, username, role, created_at FROM users ORDER BY created_at DESC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(UserInfo {
                    id: row.get(0)?,
                    username: row.get(1)?,
                    role: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })?;
            let mut result = Vec::new();
            for r in rows {
                result.push(r?);
            }
            Ok(result)
        })
    }

    pub fn update_user_role(&self, user_id: &str, role: &str) -> Result<(), AppError> {
        self.with_conn(|conn| {
            let changed = conn.execute(
                "UPDATE users SET role=?1 WHERE id=?2",
                params![role, user_id],
            )?;
            if changed == 0 {
                return Err(AppError::NotFound("用户不存在".to_string()));
            }
            Ok(())
        })
    }

    pub fn delete_user(&self, user_id: &str) -> Result<(), AppError> {
        self.with_conn(|conn| {
            let changed = conn.execute("DELETE FROM users WHERE id=?1", params![user_id])?;
            if changed == 0 {
                return Err(AppError::NotFound("用户不存在".to_string()));
            }
            Ok(())
        })
    }

    // --- Timeline ---

    pub fn get_patient_timeline(&self, patient_id: &str) -> Result<Vec<TimelineEvent>, AppError> {
        self.with_conn(|conn| {
            let mut events: Vec<TimelineEvent> = Vec::new();

            // Reports
            let mut stmt = conn.prepare(
                "SELECT id, report_type, report_date, hospital, created_at FROM reports WHERE patient_id=?1",
            )?;
            let rows = stmt.query_map(params![patient_id], |row| {
                let id: String = row.get(0)?;
                let report_type: String = row.get(1)?;
                let report_date: String = row.get(2)?;
                let hospital: String = row.get(3)?;
                let created_at: String = row.get(4)?;
                Ok(TimelineEvent {
                    event_type: "report".to_string(),
                    event_date: report_date,
                    title: report_type,
                    description: hospital,
                    related_id: id,
                    created_at,
                })
            })?;
            for r in rows {
                events.push(r?);
            }

            // Temperature records
            let mut stmt = conn.prepare(
                "SELECT id, recorded_at, value, note, created_at FROM temperature_records WHERE patient_id=?1",
            )?;
            let rows = stmt.query_map(params![patient_id], |row| {
                let id: String = row.get(0)?;
                let recorded_at: String = row.get(1)?;
                let value: f64 = row.get(2)?;
                let note: String = row.get(3)?;
                let created_at: String = row.get(4)?;
                Ok(TimelineEvent {
                    event_type: "temperature".to_string(),
                    event_date: recorded_at.split(' ').next().unwrap_or(&recorded_at).to_string(),
                    title: format!("体温 {}℃", value),
                    description: note,
                    related_id: id,
                    created_at,
                })
            })?;
            for r in rows {
                events.push(r?);
            }

            // Expenses
            let mut stmt = conn.prepare(
                "SELECT id, expense_date, total_amount, created_at FROM daily_expenses WHERE patient_id=?1",
            )?;
            let rows = stmt.query_map(params![patient_id], |row| {
                let id: String = row.get(0)?;
                let expense_date: String = row.get(1)?;
                let total_amount: f64 = row.get(2)?;
                let created_at: String = row.get(3)?;
                Ok(TimelineEvent {
                    event_type: "expense".to_string(),
                    event_date: expense_date,
                    title: format!("消费 ¥{:.2}", total_amount),
                    description: String::new(),
                    related_id: id,
                    created_at,
                })
            })?;
            for r in rows {
                events.push(r?);
            }

            // Medications
            let mut stmt = conn.prepare(
                "SELECT id, name, dosage, start_date, created_at FROM medications WHERE patient_id=?1",
            )?;
            let rows = stmt.query_map(params![patient_id], |row| {
                let id: String = row.get(0)?;
                let name: String = row.get(1)?;
                let dosage: String = row.get(2)?;
                let start_date: String = row.get(3)?;
                let created_at: String = row.get(4)?;
                Ok(TimelineEvent {
                    event_type: "medication".to_string(),
                    event_date: start_date,
                    title: format!("用药 {}", name),
                    description: dosage,
                    related_id: id,
                    created_at,
                })
            })?;
            for r in rows {
                events.push(r?);
            }

            // Sort by event_date descending
            events.sort_by(|a, b| b.event_date.cmp(&a.event_date));
            Ok(events)
        })
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
    pub role: String,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TimelineEvent {
    pub event_type: String,
    pub event_date: String,
    pub title: String,
    pub description: String,
    pub related_id: String,
    pub created_at: String,
}
