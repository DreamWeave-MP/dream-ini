// SPDX-License-Identifier: GPL-3.0-only

use std::path::{Path, PathBuf};

pub(super) fn optional_path(value: &str) -> Option<PathBuf> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
}

pub(super) fn same_cfg_context(left: &Path, right: &Path) -> bool {
    equivalent_dirs(cfg_parent(left), cfg_parent(right))
}

pub(super) fn cfg_parent(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn equivalent_dirs(left: &Path, right: &Path) -> bool {
    let left = std::fs::canonicalize(left).unwrap_or_else(|_| left.to_owned());
    let right = std::fs::canonicalize(right).unwrap_or_else(|_| right.to_owned());
    left == right
}
