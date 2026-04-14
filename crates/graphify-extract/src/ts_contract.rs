use std::path::PathBuf;

use graphify_core::contract::Contract;

#[derive(Debug, Clone, PartialEq)]
pub struct TsContractParseError {
    pub message: String,
}

impl std::fmt::Display for TsContractParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for TsContractParseError {}

pub fn extract_ts_contract(source: &str, export: &str) -> Result<Contract, TsContractParseError> {
    extract_ts_contract_at(source, export, PathBuf::from("<inline>"))
}

pub fn extract_ts_contract_at(
    _source: &str,
    _export: &str,
    _source_file: PathBuf,
) -> Result<Contract, TsContractParseError> {
    Err(TsContractParseError {
        message: "not yet implemented".into(),
    })
}
