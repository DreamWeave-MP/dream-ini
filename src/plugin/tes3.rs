// SPDX-License-Identifier: GPL-3.0-only

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use crate::{ImportError, TextEncoding};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginHeader {
    pub name: String,
    pub masters: Vec<String>,
}

pub(crate) fn read_header(
    path: &Path,
    encoding: TextEncoding,
) -> Result<PluginHeader, ImportError> {
    let mut file = File::open(path).map_err(|source| ImportError::Io {
        path: path.to_owned(),
        source,
    })?;
    let mut record_header = [0; 16];
    read_exact_plugin(
        &mut file,
        path,
        &mut record_header,
        "unexpected end of file",
    )?;

    if &record_header[0..4] != b"TES3" {
        return Err(ImportError::InvalidPluginHeader {
            path: path.to_owned(),
            message: "missing TES3 record".to_owned(),
        });
    }

    let record_size = u64::from(u32::from_le_bytes(
        record_header[4..8]
            .try_into()
            .expect("slice length checked"),
    ));
    let record_end =
        16u64
            .checked_add(record_size)
            .ok_or_else(|| ImportError::InvalidPluginHeader {
                path: path.to_owned(),
                message: "TES3 record size overflow".to_owned(),
            })?;

    let file_len = file
        .metadata()
        .map_err(|source| ImportError::Io {
            path: path.to_owned(),
            source,
        })?
        .len();
    if file_len < record_end {
        return Err(ImportError::InvalidPluginHeader {
            path: path.to_owned(),
            message: "TES3 record extends past end of file".to_owned(),
        });
    }

    let mut offset = 16u64;
    let mut masters = Vec::new();

    while offset + 8 <= record_end {
        let (name, size) = read_subrecord_header(&mut file, path)?;
        offset += 8;

        let subrecord_end =
            offset
                .checked_add(size)
                .ok_or_else(|| ImportError::InvalidPluginHeader {
                    path: path.to_owned(),
                    message: "subrecord size overflow".to_owned(),
                })?;
        if subrecord_end > record_end {
            return Err(ImportError::InvalidPluginHeader {
                path: path.to_owned(),
                message: "subrecord extends past TES3 record".to_owned(),
            });
        }

        if name == *b"MAST" {
            let mut data = vec![
                0;
                usize::try_from(size).map_err(|_| {
                    ImportError::InvalidPluginHeader {
                        path: path.to_owned(),
                        message: "subrecord size does not fit in memory".to_owned(),
                    }
                })?
            ];
            read_exact_plugin(
                &mut file,
                path,
                &mut data,
                "TES3 record extends past end of file",
            )?;
            masters.push(read_c_string(&data, encoding));
        } else {
            skip_subrecord_data(&mut file, path, size)?;
        }

        offset = subrecord_end;
    }

    if offset != record_end {
        return Err(ImportError::InvalidPluginHeader {
            path: path.to_owned(),
            message: "trailing partial subrecord header in TES3 record".to_owned(),
        });
    }

    Ok(PluginHeader {
        name: path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        ),
        masters,
    })
}

fn read_subrecord_header(file: &mut File, path: &Path) -> Result<([u8; 4], u64), ImportError> {
    let mut header = [0; 8];
    read_exact_plugin(
        file,
        path,
        &mut header,
        "TES3 record extends past end of file",
    )?;
    let name = header[0..4].try_into().expect("slice length checked");
    let size = u64::from(u32::from_le_bytes(
        header[4..8].try_into().expect("slice length checked"),
    ));
    Ok((name, size))
}

fn skip_subrecord_data(file: &mut File, path: &Path, size: u64) -> Result<(), ImportError> {
    let offset = i64::try_from(size).map_err(|_| ImportError::InvalidPluginHeader {
        path: path.to_owned(),
        message: "subrecord size does not fit in seek offset".to_owned(),
    })?;
    file.seek(SeekFrom::Current(offset))
        .map(|_| ())
        .map_err(|source| ImportError::Io {
            path: path.to_owned(),
            source,
        })
}

fn read_exact_plugin(
    file: &mut File,
    path: &Path,
    buffer: &mut [u8],
    eof_message: &str,
) -> Result<(), ImportError> {
    file.read_exact(buffer).map_err(|source| {
        if source.kind() == io::ErrorKind::UnexpectedEof {
            ImportError::InvalidPluginHeader {
                path: path.to_owned(),
                message: eof_message.to_owned(),
            }
        } else {
            ImportError::Io {
                path: path.to_owned(),
                source,
            }
        }
    })
}

fn read_c_string(bytes: &[u8], encoding: TextEncoding) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    let (decoded, _, _) = encoding.encoding_rs().decode(&bytes[..end]);
    decoded.into_owned()
}
