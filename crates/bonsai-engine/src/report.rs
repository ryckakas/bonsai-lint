use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Report {
    pub thresholds: BTreeMap<String, u32>,
    pub breaches: usize,
    pub findings: Vec<ReportedFinding>,
}

#[derive(Debug, Serialize)]
pub struct ReportedFinding {
    pub path: String,
    pub line: usize,
    pub name: String,
    pub score: u32,
    pub language: &'static str,
    pub domain: String,
}
