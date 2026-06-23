pub mod google;
pub mod provider;

#[cfg(test)]
pub mod mock_provider;
#[cfg(test)]
pub use mock_provider::MockSpreadsheetProvider;

pub use google::GoogleSheetsProvider;
pub use provider::SpreadsheetProvider;
