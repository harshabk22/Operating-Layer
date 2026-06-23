use super::provider::SpreadsheetProvider;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::Mutex;

pub struct MockSpreadsheetProvider {
    sheets: Mutex<HashMap<String, Vec<Vec<String>>>>,
}

impl Default for MockSpreadsheetProvider {
    fn default() -> Self {
        Self {
            sheets: Mutex::new(HashMap::new()),
        }
    }
}

impl MockSpreadsheetProvider {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_row(&self, sheet_name: &str, row_index: usize) -> Option<Vec<String>> {
        let sheets = self.sheets.lock().unwrap();
        sheets
            .get(sheet_name)
            .and_then(|rows| rows.get(row_index).cloned())
    }

    pub fn get_rows(&self, sheet_name: &str) -> Option<Vec<Vec<String>>> {
        let sheets = self.sheets.lock().unwrap();
        sheets.get(sheet_name).cloned()
    }
}

impl SpreadsheetProvider for MockSpreadsheetProvider {
    fn append_row(&self, sheet_name: &str, values: Vec<String>) -> Result<()> {
        let mut sheets = self.sheets.lock().unwrap();
        sheets
            .entry(sheet_name.to_string())
            .or_default()
            .push(values);
        Ok(())
    }

    fn update_row(&self, sheet_name: &str, row_index: usize, values: Vec<String>) -> Result<()> {
        let mut sheets = self.sheets.lock().unwrap();
        if let Some(rows) = sheets.get_mut(sheet_name) {
            if row_index < rows.len() {
                rows[row_index] = values;
                Ok(())
            } else {
                Err(anyhow!(
                    "Row index out of bounds: {} (len: {})",
                    row_index,
                    rows.len()
                ))
            }
        } else {
            Err(anyhow!("Sheet not found: {}", sheet_name))
        }
    }

    fn lookup_row(&self, sheet_name: &str, query: &str) -> Result<Option<(usize, Vec<String>)>> {
        let sheets = self.sheets.lock().unwrap();
        if let Some(rows) = sheets.get(sheet_name) {
            for (index, row) in rows.iter().enumerate() {
                for cell in row {
                    if cell.contains(query) {
                        return Ok(Some((index, row.clone())));
                    }
                }
            }
        }
        Ok(None)
    }
}
