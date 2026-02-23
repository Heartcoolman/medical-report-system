pub mod image;
pub mod parser;
pub mod vision;

use serde::{Deserialize, Serialize};

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
    pub value: String,
    pub unit: String,
    pub reference_range: String,
    pub status: String,
}
