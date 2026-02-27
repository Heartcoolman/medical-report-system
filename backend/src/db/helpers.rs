use crate::crypto;
use crate::error::AppError;
use crate::models::{ExpenseCategory, Gender, ItemStatus, Patient};
use pinyin::ToPinyin;
use rusqlite::{params, Connection};

pub const DEFAULT_PAGE_SIZE: usize = 20;

/// Convert a Chinese string to its full pinyin representation (lowercase, no spaces).
/// Non-Chinese characters are kept as-is.
pub fn to_pinyin_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 3);
    for c in s.chars() {
        match c.to_pinyin() {
            Some(pinyin) => result.push_str(pinyin.plain()),
            None => result.push(c.to_ascii_lowercase()),
        }
    }
    result
}

/// Convert a Chinese string to its pinyin initials (first letter of each character's pinyin).
/// Non-Chinese characters are kept as-is.
pub fn to_pinyin_initials(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c.to_pinyin() {
            Some(pinyin) => {
                if let Some(first) = pinyin.plain().chars().next() {
                    result.push(first);
                }
            }
            None => result.push(c.to_ascii_lowercase()),
        }
    }
    result
}

pub fn gender_to_db(gender: &Gender) -> &'static str {
    match gender {
        Gender::Male => "男",
        Gender::Female => "女",
    }
}

pub fn parse_gender(value: &str) -> Gender {
    match value {
        "男" => Gender::Male,
        "女" => Gender::Female,
        _ => Gender::Male,
    }
}

pub fn status_to_db(status: &ItemStatus) -> &'static str {
    match status {
        ItemStatus::CriticalHigh => "CriticalHigh",
        ItemStatus::Normal => "Normal",
        ItemStatus::High => "High",
        ItemStatus::Low => "Low",
        ItemStatus::CriticalLow => "CriticalLow",
    }
}

pub fn parse_status(value: &str) -> ItemStatus {
    match value.trim().to_lowercase().as_str() {
        "critical_high" | "criticalhigh" => ItemStatus::CriticalHigh,
        "high" => ItemStatus::High,
        "low" => ItemStatus::Low,
        "critical_low" | "criticallow" => ItemStatus::CriticalLow,
        _ => ItemStatus::Normal,
    }
}

pub fn category_to_db(category: &ExpenseCategory) -> &'static str {
    match category {
        ExpenseCategory::Drug => "drug",
        ExpenseCategory::Test => "test",
        ExpenseCategory::Treatment => "treatment",
        ExpenseCategory::Material => "material",
        ExpenseCategory::Nursing => "nursing",
        ExpenseCategory::Other => "other",
    }
}

pub fn parse_category(value: &str) -> ExpenseCategory {
    match value.to_lowercase().as_str() {
        "drug" => ExpenseCategory::Drug,
        "test" => ExpenseCategory::Test,
        "treatment" => ExpenseCategory::Treatment,
        "material" => ExpenseCategory::Material,
        "nursing" => ExpenseCategory::Nursing,
        _ => ExpenseCategory::Other,
    }
}

/// Encrypt a patient field if encryption is enabled. Returns plaintext if not.
pub fn encrypt_patient_field(value: &str) -> Result<String, AppError> {
    if crypto::encryption_enabled() {
        crypto::encrypt_field(value).map_err(|e| AppError::internal(e))
    } else {
        Ok(value.to_string())
    }
}

/// Decrypt a patient field. Passes through plaintext if not encrypted.
pub fn decrypt_patient_field(value: &str) -> String {
    crypto::decrypt_field(value).unwrap_or_else(|_| value.to_string())
}

/// Build a Patient from a row, decrypting sensitive fields.
pub fn patient_from_row(row: &rusqlite::Row) -> rusqlite::Result<Patient> {
    Ok(Patient {
        id: row.get(0)?,
        name: row.get(1)?,
        gender: parse_gender(&row.get::<_, String>(2)?),
        dob: row.get(3)?,
        phone: decrypt_patient_field(&row.get::<_, String>(4)?),
        id_number: decrypt_patient_field(&row.get::<_, String>(5)?),
        notes: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

pub fn upsert_search_index(conn: &Connection, patient: &Patient) -> Result<(), AppError> {
    let name_lower = patient.name.to_lowercase();
    let pinyin_full = to_pinyin_string(&patient.name);
    let pinyin_init = to_pinyin_initials(&patient.name);
    let search_blob = format!(
        "{}\t{}\t{}\t{}\t{}",
        name_lower,
        pinyin_full,
        pinyin_init,
        patient.phone.to_lowercase(),
        patient.id_number.to_lowercase(),
    );
    conn.execute(
        "INSERT INTO patient_search (patient_id, search_blob) VALUES (?1, ?2)
         ON CONFLICT(patient_id) DO UPDATE SET search_blob = excluded.search_blob",
        params![patient.id, search_blob],
    )?;
    Ok(())
}

pub fn has_value_comparator_prefix(value: &str) -> bool {
    matches!(
        value.trim().chars().next(),
        Some('<' | '>' | '≤' | '≥' | '＜' | '＞')
    )
}

pub fn backfill_comparator_statuses(conn: &Connection) -> Result<(), AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, value, reference_range, status
         FROM test_items
         WHERE reference_range <> ''",
    )?;

    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut updated = 0usize;
    for (id, value, reference_range, raw_status) in rows {
        if !has_value_comparator_prefix(&value) {
            continue;
        }

        let fallback = parse_status(&raw_status);
        let computed = crate::ocr::parser::determine_status_from_value_text(
            &value,
            &reference_range,
            fallback,
        );

        if computed != fallback {
            conn.execute(
                "UPDATE test_items SET status = ?1 WHERE id = ?2",
                params![status_to_db(&computed), id],
            )?;
            updated += 1;
        }
    }

    if updated > 0 {
        tracing::info!("修正了 {} 条比较符项目状态", updated);
    }

    Ok(())
}

/// Execute a paginated query: runs COUNT(*) + data query with LIMIT/OFFSET.
/// The `data_sql` must end with `LIMIT ? OFFSET ?` placeholders.
pub fn paginated_query<T, F>(
    conn: &Connection,
    count_sql: &str,
    data_sql: &str,
    count_params: &[&dyn rusqlite::types::ToSql],
    data_params: &[&dyn rusqlite::types::ToSql],
    page: usize,
    page_size: usize,
    row_mapper: F,
) -> Result<crate::models::PaginatedList<T>, AppError>
where
    T: serde::Serialize,
    F: FnMut(&rusqlite::Row) -> rusqlite::Result<T>,
{
    let total: usize = conn
        .query_row(count_sql, count_params, |row| row.get::<_, i64>(0))?
        .try_into()
        .unwrap_or(0);

    let offset = (page - 1) * page_size;
    let limit_i64 = page_size as i64;
    let offset_i64 = offset as i64;
    let mut all_params: Vec<&dyn rusqlite::types::ToSql> = data_params.to_vec();
    all_params.push(&limit_i64);
    all_params.push(&offset_i64);

    let mut stmt = conn.prepare(data_sql)?;
    let items = stmt
        .query_map(all_params.as_slice(), row_mapper)?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(crate::models::PaginatedList {
        items,
        total,
        page,
        page_size,
    })
}

/// Normalize common cross-hospital aliases for trend grouping.
pub fn normalize_trend_item_name(name: &str) -> String {
    crate::algorithm_engine::name_normalizer::normalize_for_trend(name)
}

// Fallback keyword rules for report type categorization.
pub const REPORT_CATEGORY_RULES: &[(&str, &str)] = &[
    ("脑脊液", "脑脊液检查"),
    ("尿常规", "尿常规检查"),
    ("尿液", "尿常规检查"),
    ("尿沉渣", "尿常规检查"),
    ("粪便", "粪便常规检查"),
    ("大便", "粪便常规检查"),
    ("血常规", "血常规检查"),
    ("血细胞", "血常规检查"),
    ("全血细胞", "血常规检查"),
    ("凝血", "凝血功能检查"),
    ("肝功", "肝功能检查"),
    ("肾功", "肾功能检查"),
    ("甲状腺", "甲状腺功能检查"),
    ("甲功", "甲状腺功能检查"),
    ("血脂", "血脂检查"),
    ("血糖", "血糖检查"),
    ("糖化", "血糖检查"),
    ("电解质", "电解质检查"),
    ("乙肝", "乙肝检查"),
    ("乙型肝炎", "乙肝检查"),
    ("HBV", "乙肝检查"),
    ("感染", "感染标志物检查"),
    ("免疫球蛋白", "免疫球蛋白检查"),
    ("补体", "免疫球蛋白检查"),
    ("血沉", "感染标志物检查"),
    ("红细胞沉降", "感染标志物检查"),
    ("血气", "血气分析检查"),
    ("生化", "生化检查"),
    ("肝纤维", "肝纤维化检查"),
    ("肿瘤", "肿瘤标志物检查"),
    ("白带", "体液检查"),
    ("心肌", "心肌标志物检查"),
    ("C反应蛋白", "感染标志物检查"),
    ("CRP", "感染标志物检查"),
];

/// Group similar report_types into categories.
pub fn compute_report_categories(
    report_types: &[String],
) -> std::collections::HashMap<String, String> {
    let mut mapping = std::collections::HashMap::new();
    let mut unmatched: Vec<String> = Vec::new();

    let mut unique: Vec<&String> = report_types.iter().collect();
    unique.sort();
    unique.dedup();

    for rt in &unique {
        if let Some(cat) = crate::algorithm_engine::report_taxonomy::lookup_category_pub(rt) {
            mapping.insert((*rt).clone(), cat);
            continue;
        }
        let upper = rt.to_uppercase();
        let mut found = false;
        for &(keyword, category) in REPORT_CATEGORY_RULES {
            if rt.contains(keyword) || upper.contains(&keyword.to_uppercase()) {
                mapping.insert((*rt).clone(), category.to_string());
                found = true;
                break;
            }
        }
        if !found {
            unmatched.push((*rt).clone());
        }
    }

    unmatched.sort();
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for rt in &unmatched {
        let mut matched = false;
        for group in groups.iter_mut() {
            let prefix: String = group
                .0
                .chars()
                .zip(rt.chars())
                .take_while(|(a, b)| a == b)
                .map(|(a, _)| a)
                .collect();
            if prefix.chars().count() >= 3 {
                group.0 = prefix;
                group.1.push(rt.clone());
                matched = true;
                break;
            }
        }
        if !matched {
            groups.push((rt.clone(), vec![rt.clone()]));
        }
    }

    for (category, members) in groups {
        for member in members {
            mapping.insert(member, category.clone());
        }
    }

    mapping
}
