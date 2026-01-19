use std::fmt::{self, Display};
use std::io::Error as IoErr;
use std::num::ParseIntError;
use struson::reader::ReaderError;

macro_rules! from_error {
    ($dis:ident, $errorType:ty, $targetError:ty) => {
        impl From<$errorType> for $targetError {
            fn from(value: $errorType) -> Self {
                Self::$dis(value)
            }
        }
    };
}

#[derive(Debug)]
pub enum PackParseError {
    JsonParse(struson::reader::ReaderError),
    IoError(IoErr),
    InvalidManifest(&'static str),
    VersionParse(std::num::ParseIntError),
}
from_error!(IoError, IoErr, PackParseError);
from_error!(JsonParse, ReaderError, PackParseError);
from_error!(VersionParse, ParseIntError, PackParseError);
impl Display for PackParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::JsonParse(e) => write!(f, "Manifest parsing error {e}"),
            Self::IoError(e) => write!(f, "Io error while reading: {e}"),
            Self::InvalidManifest(e) => write!(f, "Manifest file is missing a value: {e}"),
            Self::VersionParse(e) => write!(f, "Failed parsing version: {e}"),
        }
    }
}

#[derive(Debug)]
pub enum DataError {
    InvalidData(&'static str),
    JsonParse(ReaderError),
    IoError(IoErr),
    IntConvert(ParseIntError),
    ManifestParse(PackParseError),
}
impl Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidData(missing) => {
                write!(f, "Data file is invalid, field {missing} is missing")
            }
            Self::JsonParse(e) => write!(f, "Data file parsing error: {e}"),
            Self::IoError(e) => write!(f, "Io error while reading data: {e}"),
            Self::IntConvert(e) => write!(f, "Error wgile parsing int: {e}"),
            Self::ManifestParse(e) => write!(f, "Error while parsing manifest file: {e}"),
        }
    }
}
from_error!(IoError, IoErr, DataError);
from_error!(ManifestParse, PackParseError, DataError);
from_error!(JsonParse, ReaderError, DataError);
from_error!(IntConvert, ParseIntError, DataError);
