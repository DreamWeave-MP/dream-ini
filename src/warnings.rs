// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportWarning {
    IgnoredEmptyValue { key: String },
    MalformedIniLine { line: String },
}

impl fmt::Display for ImportWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IgnoredEmptyValue { key } => write!(f, "ignored empty value for key '{key}'."),
            Self::MalformedIniLine { line } => {
                write!(f, "ini file wrongly formatted ({line}). Line ignored.")
            }
        }
    }
}
