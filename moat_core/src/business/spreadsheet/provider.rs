use anyhow::Result;

pub trait SpreadsheetProvider: Send + Sync {
    fn append_row(&self, sheet_name: &str, values: Vec<String>) -> Result<()>;
    fn update_row(&self, sheet_name: &str, row_index: usize, values: Vec<String>) -> Result<()>;
    fn lookup_row(&self, sheet_name: &str, query: &str) -> Result<Option<(usize, Vec<String>)>>;
}
