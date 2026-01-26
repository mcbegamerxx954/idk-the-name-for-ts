use super::errors::OptionsError;
use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum StorageLocation {
    Internal,
    External,
}
impl StorageLocation {
    pub const fn from_i8(int: i8) -> Option<Self> {
        match int {
            1 => Some(Self::External),
            2 => Some(Self::Internal),
            _ => None,
        }
    }
}
pub fn parse_storage_location(opt_path: &Path) -> Result<i8, OptionsError> {
    let file = File::open(opt_path)?;
    let reader = BufReader::new(file);
    for line in reader.lines().map_while(|e| e.ok()) {
        let Some((key, value)) = line.split_once(':') else {
            log::error!("Cannot find separator ':' in line of options.txt");
            continue;
        };
        if key == "dvce_filestoragelocation" {
            return Ok(value.parse::<i8>()?);
        }
    }
    Err(OptionsError::NotFound)
}
