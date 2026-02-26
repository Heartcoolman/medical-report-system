use serde::{Deserialize, Serialize};
use std::fmt;

// --- Typed Enums ---

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Gender {
    #[serde(rename = "男")]
    Male,
    #[serde(rename = "女")]
    Female,
}

impl fmt::Display for Gender {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Gender::Male => write!(f, "男"),
            Gender::Female => write!(f, "女"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemStatus {
    #[serde(rename = "critical_high")]
    CriticalHigh,
    #[serde(rename = "high")]
    High,
    #[serde(rename = "normal")]
    Normal,
    #[serde(rename = "low")]
    Low,
    #[serde(rename = "critical_low")]
    CriticalLow,
}

impl fmt::Display for ItemStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ItemStatus::CriticalHigh => write!(f, "critical_high"),
            ItemStatus::Normal => write!(f, "normal"),
            ItemStatus::High => write!(f, "high"),
            ItemStatus::CriticalLow => write!(f, "critical_low"),
            ItemStatus::Low => write!(f, "low"),
        }
    }
}

impl ItemStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ItemStatus::CriticalHigh => "critical_high",
            ItemStatus::Normal => "normal",
            ItemStatus::High => "high",
            ItemStatus::CriticalLow => "critical_low",
            ItemStatus::Low => "low",
        }
    }

    pub fn is_abnormal(&self) -> bool {
        !matches!(self, ItemStatus::Normal)
    }
}

// --- Domain Models ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Patient {
    pub id: String,
    pub name: String,
    pub gender: Gender,
    pub dob: String,
    pub phone: String,
    pub id_number: String,
    pub notes: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub id: String,
    pub patient_id: String,
    pub report_type: String,
    pub hospital: String,
    pub report_date: String,
    #[serde(default)]
    pub sample_date: String,
    pub file_path: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestItem {
    pub id: String,
    pub report_id: String,
    pub name: String,
    pub value: String,
    pub unit: String,
    pub reference_range: String,
    pub status: ItemStatus,
    #[serde(default)]
    pub canonical_name: String, // LLM 标准化名称，空则回退到 name
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportDetail {
    #[serde(flatten)]
    pub report: Report,
    pub test_items: Vec<TestItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReportSummary {
    #[serde(flatten)]
    pub report: Report,
    pub item_count: usize,
    pub abnormal_count: usize,
    pub abnormal_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PatientWithStats {
    #[serde(flatten)]
    pub patient: Patient,
    pub report_count: usize,
    pub last_report_date: String,
    pub total_abnormal: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendPoint {
    pub report_date: String,
    #[serde(default)]
    pub sample_date: String,
    pub value: String,
    pub unit: String,
    pub status: ItemStatus,
    pub reference_range: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendItemInfo {
    pub report_type: String,
    pub item_name: String,
    pub count: usize,
}

// --- Temperature ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemperatureRecord {
    pub id: String,
    pub patient_id: String,
    pub recorded_at: String,
    pub value: f64,
    #[serde(default)]
    pub note: String,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct CreateTemperatureReq {
    pub recorded_at: String,
    pub value: f64,
    #[serde(default)]
    pub note: String,
}

impl CreateTemperatureReq {
    pub fn validate(&self) -> Result<(), String> {
        if self.recorded_at.trim().is_empty() {
            return Err("记录时间不能为空".to_string());
        }
        // Validate format: YYYY-MM-DD HH:MM
        let parts: Vec<&str> = self.recorded_at.split(' ').collect();
        if parts.len() != 2 {
            return Err("记录时间格式应为 YYYY-MM-DD HH:MM".to_string());
        }
        validate_date(parts[0], "记录时间")?;
        let time_parts: Vec<&str> = parts[1].split(':').collect();
        if time_parts.len() != 2 {
            return Err("时间格式应为 HH:MM".to_string());
        }
        let hour: u8 = time_parts[0].parse().map_err(|_| "小时无效".to_string())?;
        let minute: u8 = time_parts[1].parse().map_err(|_| "分钟无效".to_string())?;
        if hour > 23 {
            return Err("小时应在 0-23 之间".to_string());
        }
        if minute > 59 {
            return Err("分钟应在 0-59 之间".to_string());
        }
        if self.value < 34.0 || self.value > 43.0 {
            return Err("体温应在 34.0-43.0℃ 之间".to_string());
        }
        Ok(())
    }
}

// --- API Response ---

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    pub data: Option<T>,
    pub message: String,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T, message: &str) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: message.to_string(),
        }
    }

    pub fn ok_msg(message: &str) -> Self {
        Self {
            success: true,
            data: None,
            message: message.to_string(),
        }
    }
}

// --- Request DTOs ---

#[derive(Deserialize)]
pub struct PatientReq {
    pub name: String,
    pub gender: Gender,
    pub dob: String,
    pub phone: String,
    pub id_number: String,
    #[serde(default)]
    pub notes: String,
}

/// Validate a date string in YYYY-MM-DD format.
/// `label` is used in error messages (e.g. "出生日期", "报告日期").
pub fn validate_date(date: &str, label: &str) -> Result<(), String> {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        return Err(format!("{}格式应为 YYYY-MM-DD", label));
    }
    let year: u16 = parts[0].parse().map_err(|_| format!("{}年份无效", label))?;
    let month: u8 = parts[1].parse().map_err(|_| format!("{}月份无效", label))?;
    let day: u8 = parts[2].parse().map_err(|_| format!("{}日期无效", label))?;
    if year < 1900 || year > 2100 {
        return Err(format!("{}年份应在 1900-2100 之间", label));
    }
    if month < 1 || month > 12 {
        return Err(format!("{}月份应在 1-12 之间", label));
    }
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
        _ => return Err(format!("{}月份无效", label)),
    };
    if day < 1 || day > max_day {
        return Err(format!("{}日期应在 1-{} 之间", label, max_day));
    }
    Ok(())
}

impl PatientReq {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("姓名不能为空".to_string());
        }
        // Gender is validated by serde deserialization (enum type)
        if !self.dob.is_empty() {
            validate_date(&self.dob, "出生日期")?;
        }
        Ok(())
    }
}

#[derive(Deserialize)]
pub struct CreateReportReq {
    pub report_type: String,
    pub hospital: String,
    pub report_date: String,
    #[serde(default)]
    pub sample_date: String,
    #[serde(default)]
    pub file_path: String,
}

impl CreateReportReq {
    pub fn validate(&self) -> Result<(), String> {
        if self.report_type.trim().is_empty() {
            return Err("报告类型不能为空".to_string());
        }
        if self.report_date.trim().is_empty() {
            return Err("报告日期不能为空".to_string());
        }
        validate_date(&self.report_date, "报告日期")?;
        if !self.sample_date.trim().is_empty() {
            validate_date(&self.sample_date, "检查日期")?;
        }
        Ok(())
    }
}

#[derive(Deserialize)]
pub struct CreateTestItemReq {
    pub report_id: String,
    pub name: String,
    pub value: String,
    pub unit: String,
    pub reference_range: String,
    pub status: ItemStatus,
}

// --- Edit Log ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldChange {
    pub field: String,
    pub old_value: String,
    pub new_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditLog {
    pub id: String,
    pub report_id: String,
    pub patient_id: String,
    pub action: String,
    pub target_type: String,
    pub target_id: String,
    pub summary: String,
    pub changes: Vec<FieldChange>,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct UpdateTestItemReq {
    pub name: Option<String>,
    pub value: Option<String>,
    pub unit: Option<String>,
    pub reference_range: Option<String>,
    pub status: Option<ItemStatus>,
}

// --- Daily Expense ---

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExpenseCategory {
    Drug,
    Test,
    Treatment,
    Material,
    Nursing,
    Other,
}

impl fmt::Display for ExpenseCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExpenseCategory::Drug => write!(f, "drug"),
            ExpenseCategory::Test => write!(f, "test"),
            ExpenseCategory::Treatment => write!(f, "treatment"),
            ExpenseCategory::Material => write!(f, "material"),
            ExpenseCategory::Nursing => write!(f, "nursing"),
            ExpenseCategory::Other => write!(f, "other"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyExpense {
    pub id: String,
    pub patient_id: String,
    pub expense_date: String,
    pub total_amount: f64,
    #[serde(default)]
    pub drug_analysis: String,
    #[serde(default)]
    pub treatment_analysis: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpenseItem {
    pub id: String,
    pub expense_id: String,
    pub name: String,
    pub category: ExpenseCategory,
    pub quantity: String,
    pub amount: f64,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyExpenseDetail {
    #[serde(flatten)]
    pub expense: DailyExpense,
    pub items: Vec<ExpenseItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyExpenseSummary {
    #[serde(flatten)]
    pub expense: DailyExpense,
    pub item_count: usize,
    pub drug_count: usize,
    pub test_count: usize,
    pub treatment_count: usize,
}

#[derive(Deserialize)]
pub struct ConfirmExpenseReq {
    pub expense_date: String,
    pub total_amount: f64,
    #[serde(default)]
    pub drug_analysis: String,
    #[serde(default)]
    pub treatment_analysis: String,
    pub items: Vec<ConfirmExpenseItemReq>,
}

#[derive(Deserialize)]
pub struct ConfirmExpenseItemReq {
    pub name: String,
    pub category: ExpenseCategory,
    pub quantity: String,
    pub amount: f64,
    #[serde(default)]
    pub note: String,
}

#[derive(Deserialize)]
pub struct BatchConfirmExpenseReq {
    pub days: Vec<ConfirmExpenseReq>,
}

// --- Medication ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Medication {
    pub id: String,
    pub patient_id: String,
    pub name: String,
    pub dosage: String,
    pub frequency: String,
    pub start_date: String,
    #[serde(default)]
    pub end_date: Option<String>,
    #[serde(default)]
    pub note: String,
    #[serde(default = "default_true")]
    pub active: bool,
    pub created_at: String,
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize)]
pub struct CreateMedicationReq {
    pub name: String,
    pub dosage: String,
    pub frequency: String,
    pub start_date: String,
    pub end_date: Option<String>,
    #[serde(default)]
    pub note: String,
}

#[derive(Deserialize)]
pub struct UpdateMedicationReq {
    pub name: Option<String>,
    pub dosage: Option<String>,
    pub frequency: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub note: Option<String>,
    pub active: Option<bool>,
}

// --- Pagination ---

#[derive(Debug, Clone, Serialize)]
pub struct PaginatedList<T: Serialize> {
    pub items: Vec<T>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_dates() {
        assert!(validate_date("2024-01-15", "测试").is_ok());
        assert!(validate_date("1900-01-01", "测试").is_ok());
        assert!(validate_date("2100-12-31", "测试").is_ok());
    }

    #[test]
    fn leap_year_feb29() {
        assert!(validate_date("2024-02-29", "测试").is_ok());
        assert!(validate_date("2000-02-29", "测试").is_ok());
        assert!(validate_date("2023-02-29", "测试").is_err());
        assert!(validate_date("1900-02-29", "测试").is_err());
    }

    #[test]
    fn invalid_format() {
        assert!(validate_date("2024/01/15", "测试").is_err());
        assert!(validate_date("20240115", "测试").is_err());
        assert!(validate_date("2024-1", "测试").is_err());
        assert!(validate_date("", "测试").is_err());
    }

    #[test]
    fn out_of_range_year() {
        assert!(validate_date("1899-01-01", "测试").is_err());
        assert!(validate_date("2101-01-01", "测试").is_err());
    }

    #[test]
    fn invalid_month() {
        assert!(validate_date("2024-00-15", "测试").is_err());
        assert!(validate_date("2024-13-15", "测试").is_err());
    }

    #[test]
    fn invalid_day() {
        assert!(validate_date("2024-01-00", "测试").is_err());
        assert!(validate_date("2024-01-32", "测试").is_err());
        assert!(validate_date("2024-04-31", "测试").is_err());
    }

    #[test]
    fn error_message_contains_label() {
        let err = validate_date("bad", "报告日期").unwrap_err();
        assert!(err.contains("报告日期"));
    }
}
