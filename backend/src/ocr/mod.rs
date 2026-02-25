pub mod image;
pub mod parser;
pub mod vision;

use serde::{Deserialize, Deserializer, Serialize};

fn string_or_number<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let v = serde_json::Value::deserialize(deserializer)?;
    Ok(match v {
        serde_json::Value::String(s) => s,
        serde_json::Value::Number(n) => n.to_string(),
        other => other.to_string(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedReport {
    pub report_type: String,
    pub hospital: String,
    pub report_date: String,
    #[serde(default)]
    pub sample_date: String,
    pub items: Vec<ParsedItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedItem {
    pub name: String,
    #[serde(deserialize_with = "string_or_number")]
    pub value: String,
    pub unit: String,
    pub reference_range: String,
    pub status: String,
}
