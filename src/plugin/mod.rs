// SPDX-License-Identifier: GPL-3.0-only

use std::path::Path;

use crate::{ImportError, PluginFormat, TextEncoding};

mod load_order;
mod tes3;

pub(crate) use load_order::{apply_morrowind_expansion_order, dependency_sort};
pub use tes3::PluginHeader;

/// Reads the dependency header from a plugin file.
///
/// # Errors
/// Returns [`ImportError`] if the plugin cannot be read or its header is invalid.
pub fn read_plugin_header(
    path: &Path,
    format: PluginFormat,
    encoding: TextEncoding,
) -> Result<PluginHeader, ImportError> {
    match format {
        PluginFormat::Tes3 => tes3::read_header(path, encoding),
    }
}
