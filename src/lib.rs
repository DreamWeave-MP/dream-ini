use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use encoding_rs::{Encoding, WINDOWS_1250, WINDOWS_1251, WINDOWS_1252};
use openmw_config::{EncodingSetting, EncodingType, OpenMWConfiguration};

pub type MultiMap = BTreeMap<String, Vec<String>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Game {
    Morrowind,
}

impl Game {
    fn plugin_format(self) -> PluginFormat {
        match self {
            Self::Morrowind => PluginFormat::Tes3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginFormat {
    Tes3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEncoding {
    Win1250,
    Win1251,
    Win1252,
}

impl TextEncoding {
    /// Parses an `OpenMW` encoding label.
    ///
    /// # Errors
    /// Returns [`ImportError::UnsupportedEncoding`] if `value` is not supported.
    pub fn parse(value: &str) -> Result<Self, ImportError> {
        match value.to_ascii_lowercase().as_str() {
            "win1250" | "windows-1250" => Ok(Self::Win1250),
            "win1251" | "windows-1251" => Ok(Self::Win1251),
            "win1252" | "windows-1252" => Ok(Self::Win1252),
            _ => Err(ImportError::UnsupportedEncoding(value.to_owned())),
        }
    }

    fn as_label(self) -> &'static str {
        match self {
            Self::Win1250 => "win1250",
            Self::Win1251 => "win1251",
            Self::Win1252 => "win1252",
        }
    }

    fn encoding_rs(self) -> &'static Encoding {
        match self {
            Self::Win1250 => WINDOWS_1250,
            Self::Win1251 => WINDOWS_1251,
            Self::Win1252 => WINDOWS_1252,
        }
    }

    fn from_openmw(encoding: EncodingType) -> Self {
        match encoding {
            EncodingType::WIN1250 => Self::Win1250,
            EncodingType::WIN1251 => Self::Win1251,
            _ => Self::Win1252,
        }
    }
}

#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct ImportOptions {
    pub game: Game,
    pub import_game_files: bool,
    pub import_fonts: bool,
    pub import_archives: bool,
    pub encoding: Option<TextEncoding>,
    pub verbose: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            game: Game::Morrowind,
            import_game_files: false,
            import_fonts: false,
            import_archives: true,
            encoding: None,
            verbose: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportResult {
    pub cfg: MultiMap,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub struct ConfigImportResult {
    pub config: OpenMWConfiguration,
    pub imported: ImportResult,
}

#[derive(Debug)]
pub enum ImportError {
    Io { path: PathBuf, source: io::Error },
    UnsupportedEncoding(String),
    InvalidPluginHeader { path: PathBuf, message: String },
    OpenMwConfig(String),
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "{}: {}", path.display(), source),
            Self::UnsupportedEncoding(value) => write!(f, "unsupported encoding: {value}"),
            Self::InvalidPluginHeader { path, message } => {
                write!(f, "invalid plugin header in {}: {message}", path.display())
            }
            Self::OpenMwConfig(message) => write!(f, "openmw-config error: {message}"),
        }
    }
}

impl std::error::Error for ImportError {}

#[derive(Debug, Clone)]
pub struct IniImporter {
    options: ImportOptions,
}

impl IniImporter {
    #[must_use]
    pub fn new(options: ImportOptions) -> Self {
        Self { options }
    }

    /// Imports from paths into the lightweight map model.
    ///
    /// # Errors
    /// Returns [`ImportError`] when files cannot be read, encoding is unsupported, or plugin headers are invalid.
    pub fn import_paths(
        &self,
        ini_path: &Path,
        cfg_path: &Path,
    ) -> Result<ImportResult, ImportError> {
        let cfg_text = read_to_string(cfg_path)?;
        let mut cfg = parse_cfg_str(&cfg_text);

        let encoding = self.effective_encoding(&cfg)?;
        set_single_value(&mut cfg, "encoding", encoding.as_label().to_owned());

        let ini_bytes = read_bytes(ini_path)?;
        let ini = parse_ini_bytes(&ini_bytes, encoding);
        self.import_maps(&mut cfg, &ini, ini_path)
    }

    /// Imports from paths into an [`OpenMWConfiguration`].
    ///
    /// # Errors
    /// Returns [`ImportError`] when config loading fails, files cannot be read, encoding is unsupported, or plugin headers are invalid.
    pub fn import_config_paths(
        &self,
        ini_path: &Path,
        cfg_path: &Path,
    ) -> Result<ConfigImportResult, ImportError> {
        let mut config = OpenMWConfiguration::new(Some(cfg_path.to_owned()))
            .map_err(|error| ImportError::OpenMwConfig(error.to_string()))?;
        let mut cfg = config_to_multimap(&config);

        let encoding = self.effective_encoding_from_config(&config);
        apply_encoding(&mut config, cfg_path, encoding)?;
        set_single_value(&mut cfg, "encoding", encoding.as_label().to_owned());

        let ini_bytes = read_bytes(ini_path)?;
        let ini = parse_ini_bytes(&ini_bytes, encoding);
        let imported = self.import_maps(&mut cfg, &ini, ini_path)?;
        apply_imported_cfg(&mut config, cfg_path, &imported.cfg)?;

        Ok(ConfigImportResult { config, imported })
    }

    /// Saves an imported configuration to an arbitrary output path.
    ///
    /// # Errors
    /// Returns [`ImportError`] when `openmw_config` cannot write the file.
    pub fn save_config_output(
        &self,
        output_path: &Path,
        config: &OpenMWConfiguration,
    ) -> Result<(), ImportError> {
        config
            .save_to_path(output_path)
            .map_err(|error| ImportError::OpenMwConfig(error.to_string()))
    }

    /// Imports already parsed maps into the lightweight map model.
    ///
    /// # Errors
    /// Returns [`ImportError`] when plugin headers cannot be read or decoded.
    pub fn import_maps(
        &self,
        cfg: &mut MultiMap,
        ini: &MultiMap,
        ini_path: &Path,
    ) -> Result<ImportResult, ImportError> {
        let mut warnings = Vec::new();
        let mut ini = ini.clone();

        if !self.options.import_fonts {
            ini.remove("Fonts:Font 0");
            ini.remove("Fonts:Font 1");
            ini.remove("Fonts:Font 2");
        }

        merge(cfg, &ini);
        merge_fallback(cfg, &ini);

        if self.options.import_game_files {
            self.import_game_files(cfg, &ini, ini_path, &mut warnings)?;
        }

        if self.options.import_archives {
            import_archives(cfg, &ini);
        }

        Ok(ImportResult {
            cfg: cfg.clone(),
            warnings,
        })
    }

    fn effective_encoding(&self, cfg: &MultiMap) -> Result<TextEncoding, ImportError> {
        if let Some(encoding) = self.options.encoding {
            return Ok(encoding);
        }

        if let Some(value) = cfg.get("encoding").and_then(|values| values.last()) {
            return TextEncoding::parse(value);
        }

        Ok(TextEncoding::Win1252)
    }

    fn effective_encoding_from_config(&self, config: &OpenMWConfiguration) -> TextEncoding {
        if let Some(encoding) = self.options.encoding {
            return encoding;
        }

        let Some(setting) = config.encoding() else {
            return TextEncoding::Win1252;
        };

        TextEncoding::from_openmw(setting.value())
    }

    fn import_game_files(
        &self,
        cfg: &mut MultiMap,
        ini: &MultiMap,
        ini_path: &Path,
        warnings: &mut Vec<String>,
    ) -> Result<(), ImportError> {
        let mut data_paths = Vec::new();
        if let Some(paths) = cfg.get("data") {
            add_paths(&mut data_paths, paths);
        }
        if let Some(paths) = cfg.get("data-local") {
            add_paths(&mut data_paths, paths);
        }
        data_paths.push(
            ini_path
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .join("Data Files"),
        );

        let mut content_files = Vec::new();
        for file in sequential_ini_values(ini, "Game Files:GameFile") {
            if !ends_with_ignore_ascii_case(file, ".esm")
                && !ends_with_ignore_ascii_case(file, ".esp")
            {
                continue;
            }

            let mut found = None;
            for data_path in &data_paths {
                let candidate = data_path.join(file);
                if let Ok(metadata) = fs::metadata(&candidate) {
                    let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
                    found = Some((system_time_key(modified), candidate));
                    break;
                }
            }

            if let Some(entry) = found {
                content_files.push(entry);
            } else {
                warnings.push(format!("{file} not found, ignoring"));
            }
        }

        content_files
            .sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));

        let format = self.options.game.plugin_format();
        let mut dependencies = Vec::new();
        for (_, path) in content_files {
            let header = read_plugin_header(&path, format, self.effective_encoding(cfg)?)?;
            dependencies.push((header.name, header.masters));
        }

        let mut sorted = dependency_sort(dependencies);
        apply_morrowind_expansion_order(&mut sorted);
        cfg.insert("content".to_owned(), sorted);

        Ok(())
    }
}

#[must_use]
pub fn parse_ini_bytes(bytes: &[u8], encoding: TextEncoding) -> MultiMap {
    let (decoded, _, _) = encoding.encoding_rs().decode(bytes);
    parse_ini_str(&decoded)
}

#[must_use]
pub fn parse_ini_str(text: &str) -> MultiMap {
    let mut section = String::new();
    let mut map = MultiMap::new();

    for raw_line in text.lines() {
        let mut line = raw_line;
        if let Some(stripped) = line.strip_suffix('\r') {
            line = stripped;
        }

        if line.is_empty() {
            continue;
        }

        if line.starts_with('[') {
            let Some(end) = line.find(']') else {
                continue;
            };
            if end < 2 {
                continue;
            }
            line[1..end].clone_into(&mut section);
            continue;
        }

        if let Some(comment) = line.find(';') {
            line = &line[..comment];
        }

        let Some(equals) = line.find('=') else {
            continue;
        };
        if equals < 1 {
            continue;
        }

        let key = format!("{}:{}", section, &line[..equals]);
        let value = &line[equals + 1..];
        if value.is_empty() {
            continue;
        }
        insert_multimap(&mut map, key, value.to_owned());
    }

    map
}

#[must_use]
pub fn parse_cfg_str(text: &str) -> MultiMap {
    let mut map = MultiMap::new();

    for line in text.lines() {
        if line.find('#') == first_non_ws(line) {
            continue;
        }
        if line.is_empty() {
            continue;
        }

        let Some(equals) = line.find('=') else {
            continue;
        };
        if equals < 1 {
            continue;
        }

        let key = line[..equals].trim().to_owned();
        let value = line[equals + 1..].trim().to_owned();
        insert_multimap(&mut map, key, value);
    }

    map
}

#[must_use]
pub fn serialize_cfg(cfg: &MultiMap) -> String {
    let mut output = String::new();
    for (key, values) in cfg {
        for value in values {
            output.push_str(key);
            output.push('=');
            output.push_str(value);
            output.push('\n');
        }
    }
    output
}

fn merge(cfg: &mut MultiMap, ini: &MultiMap) {
    if let Some(values) = ini.get("General:Disable Audio") {
        cfg.remove("no-sound");
        for value in values {
            insert_multimap(cfg, "no-sound".to_owned(), value.clone());
        }
    }
}

fn merge_fallback(cfg: &mut MultiMap, ini: &MultiMap) {
    cfg.remove("fallback");
    for key in MORROWIND_FALLBACK_KEYS {
        if let Some(values) = ini.get(*key) {
            for value in values {
                let fallback_key = key.replace([' ', ':'], "_");
                insert_multimap(
                    cfg,
                    "fallback".to_owned(),
                    format!("{fallback_key},{value}"),
                );
            }
        }
    }
}

fn import_archives(cfg: &mut MultiMap, ini: &MultiMap) {
    let mut archives = vec!["Morrowind.bsa".to_owned()];
    archives.extend(sequential_ini_values(ini, "Archives:Archive ").cloned());
    cfg.insert("fallback-archive".to_owned(), archives);
}

fn sequential_ini_values<'a>(ini: &'a MultiMap, prefix: &str) -> impl Iterator<Item = &'a String> {
    (0..)
        .map(move |index| format!("{prefix}{index}"))
        .map_while(move |key| ini.get(&key))
        .flat_map(|values| values.iter())
}

fn add_paths(output: &mut Vec<PathBuf>, input: &[String]) {
    for path in input {
        let unquoted = path
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .unwrap_or(path);
        output.push(PathBuf::from(unquoted));
    }
}

fn dependency_sort(mut source: Vec<(String, Vec<String>)>) -> Vec<String> {
    let mut result = Vec::new();
    while let Some((element, _)) = source.first() {
        let element = element.clone();
        dependency_sort_step(&element, &mut source, &mut result);
    }
    result
}

fn dependency_sort_step(
    element: &str,
    source: &mut Vec<(String, Vec<String>)>,
    result: &mut Vec<String>,
) {
    let Some(index) = source.iter().position(|(name, _)| name == element) else {
        return;
    };
    let (name, dependencies) = source.remove(index);
    for dependency in dependencies {
        dependency_sort_step(&dependency, source, result);
    }
    result.push(name);
}

fn apply_morrowind_expansion_order(files: &mut Vec<String>) {
    if !contains_ignore_ascii_case(files, "Morrowind.esm") {
        return;
    }

    let Some(tribunal_index) = position_ignore_ascii_case(files, "Tribunal.esm") else {
        return;
    };
    let Some(bloodmoon_index) = position_ignore_ascii_case(files, "Bloodmoon.esm") else {
        return;
    };

    if bloodmoon_index < tribunal_index {
        let tribunal = files.remove(tribunal_index);
        let bloodmoon_index = position_ignore_ascii_case(files, "Bloodmoon.esm")
            .expect("Bloodmoon.esm remains present");
        files.insert(bloodmoon_index, tribunal);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginHeader {
    pub name: String,
    pub masters: Vec<String>,
}

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
        PluginFormat::Tes3 => read_tes3_header(path, encoding),
    }
}

fn read_tes3_header(path: &Path, encoding: TextEncoding) -> Result<PluginHeader, ImportError> {
    let bytes = read_bytes(path)?;
    if bytes.len() < 16 || &bytes[0..4] != b"TES3" {
        return Err(ImportError::InvalidPluginHeader {
            path: path.to_owned(),
            message: "missing TES3 record".to_owned(),
        });
    }

    let record_size = read_u32_le(&bytes, 4, path)? as usize;
    let record_start = 16usize;
    let record_end =
        record_start
            .checked_add(record_size)
            .ok_or_else(|| ImportError::InvalidPluginHeader {
                path: path.to_owned(),
                message: "TES3 record size overflow".to_owned(),
            })?;

    if bytes.len() < record_end {
        return Err(ImportError::InvalidPluginHeader {
            path: path.to_owned(),
            message: "TES3 record extends past end of file".to_owned(),
        });
    }

    let mut offset = record_start;
    let mut masters = Vec::new();
    while offset + 8 <= record_end {
        let name = &bytes[offset..offset + 4];
        let size = read_u32_le(&bytes, offset + 4, path)? as usize;
        offset += 8;

        if offset + size > record_end {
            return Err(ImportError::InvalidPluginHeader {
                path: path.to_owned(),
                message: "subrecord extends past TES3 record".to_owned(),
            });
        }

        if name == b"MAST" {
            masters.push(read_c_string(&bytes[offset..offset + size], encoding));
        }

        offset += size;
    }

    Ok(PluginHeader {
        name: path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        ),
        masters,
    })
}

fn config_to_multimap(config: &OpenMWConfiguration) -> MultiMap {
    let mut cfg = MultiMap::new();

    if let Some(encoding) = config.encoding() {
        insert_multimap(
            &mut cfg,
            "encoding".to_owned(),
            TextEncoding::from_openmw(encoding.value())
                .as_label()
                .to_owned(),
        );
    }

    if let Some(data_local) = config.data_local() {
        insert_multimap(
            &mut cfg,
            "data-local".to_owned(),
            data_local.parsed().to_string_lossy().into_owned(),
        );
    }

    for data in config.data_directories_iter() {
        insert_multimap(
            &mut cfg,
            "data".to_owned(),
            data.parsed().to_string_lossy().into_owned(),
        );
    }

    for archive in config.fallback_archives_iter() {
        insert_multimap(
            &mut cfg,
            "fallback-archive".to_owned(),
            archive.value().clone(),
        );
    }

    for content in config.content_files_iter() {
        insert_multimap(&mut cfg, "content".to_owned(), content.value().clone());
    }

    for setting in config.game_settings() {
        insert_multimap(
            &mut cfg,
            "fallback".to_owned(),
            format!("{},{}", setting.key(), setting.value()),
        );
    }

    for setting in config.generic_settings_iter() {
        insert_multimap(
            &mut cfg,
            setting.key().to_owned(),
            setting.value().to_owned(),
        );
    }

    cfg
}

fn apply_imported_cfg(
    config: &mut OpenMWConfiguration,
    cfg_path: &Path,
    cfg: &MultiMap,
) -> Result<(), ImportError> {
    if let Some(values) = cfg.get("no-sound") {
        config.set_generic_settings("no-sound", Some(values.clone()));
    }

    let encoding = cfg
        .get("encoding")
        .and_then(|values| values.last())
        .map_or(Ok(TextEncoding::Win1252), |value| {
            TextEncoding::parse(value)
        })?;
    apply_encoding(config, cfg_path, encoding)?;

    config
        .set_game_settings(cfg.get("fallback").cloned())
        .map_err(|error| ImportError::OpenMwConfig(error.to_string()))?;

    if let Some(values) = cfg.get("fallback-archive") {
        config.set_fallback_archives(Some(values.clone()));
    }

    if let Some(values) = cfg.get("content") {
        config.set_content_files(Some(values.clone()));
    }

    Ok(())
}

fn apply_encoding(
    config: &mut OpenMWConfiguration,
    cfg_path: &Path,
    encoding: TextEncoding,
) -> Result<(), ImportError> {
    let mut empty = String::new();
    let setting = EncodingSetting::try_from((encoding.as_label().to_owned(), cfg_path, &mut empty))
        .map_err(|error| ImportError::OpenMwConfig(error.to_string()))?;
    config.set_encoding(Some(setting));
    Ok(())
}

fn read_to_string(path: &Path) -> Result<String, ImportError> {
    fs::read_to_string(path).map_err(|source| ImportError::Io {
        path: path.to_owned(),
        source,
    })
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, ImportError> {
    fs::read(path).map_err(|source| ImportError::Io {
        path: path.to_owned(),
        source,
    })
}

fn insert_multimap(map: &mut MultiMap, key: String, value: String) {
    map.entry(key).or_default().push(value);
}

fn set_single_value(map: &mut MultiMap, key: &str, value: String) {
    map.insert(key.to_owned(), vec![value]);
}

fn first_non_ws(value: &str) -> Option<usize> {
    value
        .char_indices()
        .find_map(|(index, ch)| (!matches!(ch, ' ' | '\t' | '\r' | '\n')).then_some(index))
}

fn ends_with_ignore_ascii_case(value: &str, suffix: &str) -> bool {
    value.len() >= suffix.len() && value[value.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
}

fn contains_ignore_ascii_case(values: &[String], needle: &str) -> bool {
    position_ignore_ascii_case(values, needle).is_some()
}

fn position_ignore_ascii_case(values: &[String], needle: &str) -> Option<usize> {
    values
        .iter()
        .position(|value| value.eq_ignore_ascii_case(needle))
}

fn system_time_key(time: SystemTime) -> u128 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn read_u32_le(bytes: &[u8], offset: usize, path: &Path) -> Result<u32, ImportError> {
    let bytes = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| ImportError::InvalidPluginHeader {
            path: path.to_owned(),
            message: "unexpected end of file".to_owned(),
        })?;
    Ok(u32::from_le_bytes(
        bytes.try_into().expect("slice length checked"),
    ))
}

fn read_c_string(bytes: &[u8], encoding: TextEncoding) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    let (decoded, _, _) = encoding.encoding_rs().decode(&bytes[..end]);
    decoded.into_owned()
}

#[must_use]
pub fn known_fallback_keys() -> &'static [&'static str] {
    MORROWIND_FALLBACK_KEYS
}

const MORROWIND_FALLBACK_KEYS: &[&str] = &[
    "LightAttenuation:UseConstant",
    "LightAttenuation:ConstantValue",
    "LightAttenuation:UseLinear",
    "LightAttenuation:LinearMethod",
    "LightAttenuation:LinearValue",
    "LightAttenuation:LinearRadiusMult",
    "LightAttenuation:UseQuadratic",
    "LightAttenuation:QuadraticMethod",
    "LightAttenuation:QuadraticValue",
    "LightAttenuation:QuadraticRadiusMult",
    "LightAttenuation:OutQuadInLin",
    "Inventory:DirectionalDiffuseR",
    "Inventory:DirectionalDiffuseG",
    "Inventory:DirectionalDiffuseB",
    "Inventory:DirectionalAmbientR",
    "Inventory:DirectionalAmbientG",
    "Inventory:DirectionalAmbientB",
    "Inventory:DirectionalRotationX",
    "Inventory:DirectionalRotationY",
    "Inventory:UniformScaling",
    "Map:Travel Siltstrider Red",
    "Map:Travel Siltstrider Green",
    "Map:Travel Siltstrider Blue",
    "Map:Travel Boat Red",
    "Map:Travel Boat Green",
    "Map:Travel Boat Blue",
    "Map:Travel Magic Red",
    "Map:Travel Magic Green",
    "Map:Travel Magic Blue",
    "Map:Show Travel Lines",
    "Water:Map Alpha",
    "Water:World Alpha",
    "Water:SurfaceTextureSize",
    "Water:SurfaceTileCount",
    "Water:SurfaceFPS",
    "Water:SurfaceTexture",
    "Water:SurfaceFrameCount",
    "Water:TileTextureDivisor",
    "Water:RippleTexture",
    "Water:RippleFrameCount",
    "Water:RippleLifetime",
    "Water:MaxNumberRipples",
    "Water:RippleScale",
    "Water:RippleRotSpeed",
    "Water:RippleAlphas",
    "Water:PSWaterReflectTerrain",
    "Water:PSWaterReflectUpdate",
    "Water:NearWaterRadius",
    "Water:NearWaterPoints",
    "Water:NearWaterUnderwaterFreq",
    "Water:NearWaterUnderwaterVolume",
    "Water:NearWaterIndoorTolerance",
    "Water:NearWaterOutdoorTolerance",
    "Water:NearWaterIndoorID",
    "Water:NearWaterOutdoorID",
    "Water:UnderwaterSunriseFog",
    "Water:UnderwaterDayFog",
    "Water:UnderwaterSunsetFog",
    "Water:UnderwaterNightFog",
    "Water:UnderwaterIndoorFog",
    "Water:UnderwaterColor",
    "Water:UnderwaterColorWeight",
    "PixelWater:SurfaceFPS",
    "PixelWater:TileCount",
    "PixelWater:Resolution",
    "Fonts:Font 0",
    "Fonts:Font 1",
    "Fonts:Font 2",
    "FontColor:color_normal",
    "FontColor:color_normal_over",
    "FontColor:color_normal_pressed",
    "FontColor:color_active",
    "FontColor:color_active_over",
    "FontColor:color_active_pressed",
    "FontColor:color_disabled",
    "FontColor:color_disabled_over",
    "FontColor:color_disabled_pressed",
    "FontColor:color_link",
    "FontColor:color_link_over",
    "FontColor:color_link_pressed",
    "FontColor:color_journal_link",
    "FontColor:color_journal_link_over",
    "FontColor:color_journal_link_pressed",
    "FontColor:color_journal_topic",
    "FontColor:color_journal_topic_over",
    "FontColor:color_journal_topic_pressed",
    "FontColor:color_answer",
    "FontColor:color_answer_over",
    "FontColor:color_answer_pressed",
    "FontColor:color_header",
    "FontColor:color_notify",
    "FontColor:color_big_normal",
    "FontColor:color_big_normal_over",
    "FontColor:color_big_normal_pressed",
    "FontColor:color_big_link",
    "FontColor:color_big_link_over",
    "FontColor:color_big_link_pressed",
    "FontColor:color_big_answer",
    "FontColor:color_big_answer_over",
    "FontColor:color_big_answer_pressed",
    "FontColor:color_big_header",
    "FontColor:color_big_notify",
    "FontColor:color_background",
    "FontColor:color_focus",
    "FontColor:color_health",
    "FontColor:color_magic",
    "FontColor:color_fatigue",
    "FontColor:color_misc",
    "FontColor:color_weapon_fill",
    "FontColor:color_magic_fill",
    "FontColor:color_positive",
    "FontColor:color_negative",
    "FontColor:color_count",
    "Level Up:Level2",
    "Level Up:Level3",
    "Level Up:Level4",
    "Level Up:Level5",
    "Level Up:Level6",
    "Level Up:Level7",
    "Level Up:Level8",
    "Level Up:Level9",
    "Level Up:Level10",
    "Level Up:Level11",
    "Level Up:Level12",
    "Level Up:Level13",
    "Level Up:Level14",
    "Level Up:Level15",
    "Level Up:Level16",
    "Level Up:Level17",
    "Level Up:Level18",
    "Level Up:Level19",
    "Level Up:Level20",
    "Level Up:Default",
    "Question 1:Question",
    "Question 1:AnswerOne",
    "Question 1:AnswerTwo",
    "Question 1:AnswerThree",
    "Question 1:Sound",
    "Question 2:Question",
    "Question 2:AnswerOne",
    "Question 2:AnswerTwo",
    "Question 2:AnswerThree",
    "Question 2:Sound",
    "Question 3:Question",
    "Question 3:AnswerOne",
    "Question 3:AnswerTwo",
    "Question 3:AnswerThree",
    "Question 3:Sound",
    "Question 4:Question",
    "Question 4:AnswerOne",
    "Question 4:AnswerTwo",
    "Question 4:AnswerThree",
    "Question 4:Sound",
    "Question 5:Question",
    "Question 5:AnswerOne",
    "Question 5:AnswerTwo",
    "Question 5:AnswerThree",
    "Question 5:Sound",
    "Question 6:Question",
    "Question 6:AnswerOne",
    "Question 6:AnswerTwo",
    "Question 6:AnswerThree",
    "Question 6:Sound",
    "Question 7:Question",
    "Question 7:AnswerOne",
    "Question 7:AnswerTwo",
    "Question 7:AnswerThree",
    "Question 7:Sound",
    "Question 8:Question",
    "Question 8:AnswerOne",
    "Question 8:AnswerTwo",
    "Question 8:AnswerThree",
    "Question 8:Sound",
    "Question 9:Question",
    "Question 9:AnswerOne",
    "Question 9:AnswerTwo",
    "Question 9:AnswerThree",
    "Question 9:Sound",
    "Question 10:Question",
    "Question 10:AnswerOne",
    "Question 10:AnswerTwo",
    "Question 10:AnswerThree",
    "Question 10:Sound",
    "Blood:Model 0",
    "Blood:Model 1",
    "Blood:Model 2",
    "Blood:Texture 0",
    "Blood:Texture 1",
    "Blood:Texture 2",
    "Blood:Texture 3",
    "Blood:Texture 4",
    "Blood:Texture 5",
    "Blood:Texture 6",
    "Blood:Texture 7",
    "Blood:Texture Name 0",
    "Blood:Texture Name 1",
    "Blood:Texture Name 2",
    "Blood:Texture Name 3",
    "Blood:Texture Name 4",
    "Blood:Texture Name 5",
    "Blood:Texture Name 6",
    "Blood:Texture Name 7",
    "Movies:Company Logo",
    "Movies:Morrowind Logo",
    "Movies:New Game",
    "Movies:Loading",
    "Movies:Options Menu",
    "Weather Thunderstorm:Thunder Sound ID 0",
    "Weather Thunderstorm:Thunder Sound ID 1",
    "Weather Thunderstorm:Thunder Sound ID 2",
    "Weather Thunderstorm:Thunder Sound ID 3",
    "Weather:Sunrise Time",
    "Weather:Sunset Time",
    "Weather:Sunrise Duration",
    "Weather:Sunset Duration",
    "Weather:Hours Between Weather Changes",
    "Weather Thunderstorm:Thunder Frequency",
    "Weather Thunderstorm:Thunder Threshold",
    "Weather:EnvReduceColor",
    "Weather:LerpCloseColor",
    "Weather:BumpFadeColor",
    "Weather:AlphaReduce",
    "Weather:Minimum Time Between Environmental Sounds",
    "Weather:Maximum Time Between Environmental Sounds",
    "Weather:Sun Glare Fader Max",
    "Weather:Sun Glare Fader Angle Max",
    "Weather:Sun Glare Fader Color",
    "Weather:Timescale Clouds",
    "Weather:Precip Gravity",
    "Weather:Rain Ripples",
    "Weather:Rain Ripple Radius",
    "Weather:Rain Ripples Per Drop",
    "Weather:Rain Ripple Scale",
    "Weather:Rain Ripple Speed",
    "Weather:Fog Depth Change Speed",
    "Weather:Sky Pre-Sunrise Time",
    "Weather:Sky Post-Sunrise Time",
    "Weather:Sky Pre-Sunset Time",
    "Weather:Sky Post-Sunset Time",
    "Weather:Ambient Pre-Sunrise Time",
    "Weather:Ambient Post-Sunrise Time",
    "Weather:Ambient Pre-Sunset Time",
    "Weather:Ambient Post-Sunset Time",
    "Weather:Fog Pre-Sunrise Time",
    "Weather:Fog Post-Sunrise Time",
    "Weather:Fog Pre-Sunset Time",
    "Weather:Fog Post-Sunset Time",
    "Weather:Sun Pre-Sunrise Time",
    "Weather:Sun Post-Sunrise Time",
    "Weather:Sun Pre-Sunset Time",
    "Weather:Sun Post-Sunset Time",
    "Weather:Stars Post-Sunset Start",
    "Weather:Stars Pre-Sunrise Finish",
    "Weather:Stars Fading Duration",
    "Weather:Snow Ripples",
    "Weather:Snow Ripple Radius",
    "Weather:Snow Ripples Per Flake",
    "Weather:Snow Ripple Scale",
    "Weather:Snow Ripple Speed",
    "Weather:Snow Gravity Scale",
    "Weather:Snow High Kill",
    "Weather:Snow Low Kill",
    "Weather Clear:Cloud Texture",
    "Weather Clear:Clouds Maximum Percent",
    "Weather Clear:Transition Delta",
    "Weather Clear:Sky Sunrise Color",
    "Weather Clear:Sky Day Color",
    "Weather Clear:Sky Sunset Color",
    "Weather Clear:Sky Night Color",
    "Weather Clear:Fog Sunrise Color",
    "Weather Clear:Fog Day Color",
    "Weather Clear:Fog Sunset Color",
    "Weather Clear:Fog Night Color",
    "Weather Clear:Ambient Sunrise Color",
    "Weather Clear:Ambient Day Color",
    "Weather Clear:Ambient Sunset Color",
    "Weather Clear:Ambient Night Color",
    "Weather Clear:Sun Sunrise Color",
    "Weather Clear:Sun Day Color",
    "Weather Clear:Sun Sunset Color",
    "Weather Clear:Sun Night Color",
    "Weather Clear:Sun Disc Sunset Color",
    "Weather Clear:Land Fog Day Depth",
    "Weather Clear:Land Fog Night Depth",
    "Weather Clear:Wind Speed",
    "Weather Clear:Cloud Speed",
    "Weather Clear:Glare View",
    "Weather Clear:Ambient Loop Sound ID",
    "Weather Cloudy:Cloud Texture",
    "Weather Cloudy:Clouds Maximum Percent",
    "Weather Cloudy:Transition Delta",
    "Weather Cloudy:Sky Sunrise Color",
    "Weather Cloudy:Sky Day Color",
    "Weather Cloudy:Sky Sunset Color",
    "Weather Cloudy:Sky Night Color",
    "Weather Cloudy:Fog Sunrise Color",
    "Weather Cloudy:Fog Day Color",
    "Weather Cloudy:Fog Sunset Color",
    "Weather Cloudy:Fog Night Color",
    "Weather Cloudy:Ambient Sunrise Color",
    "Weather Cloudy:Ambient Day Color",
    "Weather Cloudy:Ambient Sunset Color",
    "Weather Cloudy:Ambient Night Color",
    "Weather Cloudy:Sun Sunrise Color",
    "Weather Cloudy:Sun Day Color",
    "Weather Cloudy:Sun Sunset Color",
    "Weather Cloudy:Sun Night Color",
    "Weather Cloudy:Sun Disc Sunset Color",
    "Weather Cloudy:Land Fog Day Depth",
    "Weather Cloudy:Land Fog Night Depth",
    "Weather Cloudy:Wind Speed",
    "Weather Cloudy:Cloud Speed",
    "Weather Cloudy:Glare View",
    "Weather Cloudy:Ambient Loop Sound ID",
    "Weather Foggy:Cloud Texture",
    "Weather Foggy:Clouds Maximum Percent",
    "Weather Foggy:Transition Delta",
    "Weather Foggy:Sky Sunrise Color",
    "Weather Foggy:Sky Day Color",
    "Weather Foggy:Sky Sunset Color",
    "Weather Foggy:Sky Night Color",
    "Weather Foggy:Fog Sunrise Color",
    "Weather Foggy:Fog Day Color",
    "Weather Foggy:Fog Sunset Color",
    "Weather Foggy:Fog Night Color",
    "Weather Foggy:Ambient Sunrise Color",
    "Weather Foggy:Ambient Day Color",
    "Weather Foggy:Ambient Sunset Color",
    "Weather Foggy:Ambient Night Color",
    "Weather Foggy:Sun Sunrise Color",
    "Weather Foggy:Sun Day Color",
    "Weather Foggy:Sun Sunset Color",
    "Weather Foggy:Sun Night Color",
    "Weather Foggy:Sun Disc Sunset Color",
    "Weather Foggy:Land Fog Day Depth",
    "Weather Foggy:Land Fog Night Depth",
    "Weather Foggy:Wind Speed",
    "Weather Foggy:Cloud Speed",
    "Weather Foggy:Glare View",
    "Weather Foggy:Ambient Loop Sound ID",
    "Weather Thunderstorm:Cloud Texture",
    "Weather Thunderstorm:Clouds Maximum Percent",
    "Weather Thunderstorm:Transition Delta",
    "Weather Thunderstorm:Sky Sunrise Color",
    "Weather Thunderstorm:Sky Day Color",
    "Weather Thunderstorm:Sky Sunset Color",
    "Weather Thunderstorm:Sky Night Color",
    "Weather Thunderstorm:Fog Sunrise Color",
    "Weather Thunderstorm:Fog Day Color",
    "Weather Thunderstorm:Fog Sunset Color",
    "Weather Thunderstorm:Fog Night Color",
    "Weather Thunderstorm:Ambient Sunrise Color",
    "Weather Thunderstorm:Ambient Day Color",
    "Weather Thunderstorm:Ambient Sunset Color",
    "Weather Thunderstorm:Ambient Night Color",
    "Weather Thunderstorm:Sun Sunrise Color",
    "Weather Thunderstorm:Sun Day Color",
    "Weather Thunderstorm:Sun Sunset Color",
    "Weather Thunderstorm:Sun Night Color",
    "Weather Thunderstorm:Sun Disc Sunset Color",
    "Weather Thunderstorm:Land Fog Day Depth",
    "Weather Thunderstorm:Land Fog Night Depth",
    "Weather Thunderstorm:Wind Speed",
    "Weather Thunderstorm:Cloud Speed",
    "Weather Thunderstorm:Glare View",
    "Weather Thunderstorm:Rain Loop Sound ID",
    "Weather Thunderstorm:Using Precip",
    "Weather Thunderstorm:Rain Diameter",
    "Weather Thunderstorm:Rain Height Min",
    "Weather Thunderstorm:Rain Height Max",
    "Weather Thunderstorm:Rain Threshold",
    "Weather Thunderstorm:Max Raindrops",
    "Weather Thunderstorm:Rain Entrance Speed",
    "Weather Thunderstorm:Ambient Loop Sound ID",
    "Weather Thunderstorm:Flash Decrement",
    "Weather Rain:Cloud Texture",
    "Weather Rain:Clouds Maximum Percent",
    "Weather Rain:Transition Delta",
    "Weather Rain:Sky Sunrise Color",
    "Weather Rain:Sky Day Color",
    "Weather Rain:Sky Sunset Color",
    "Weather Rain:Sky Night Color",
    "Weather Rain:Fog Sunrise Color",
    "Weather Rain:Fog Day Color",
    "Weather Rain:Fog Sunset Color",
    "Weather Rain:Fog Night Color",
    "Weather Rain:Ambient Sunrise Color",
    "Weather Rain:Ambient Day Color",
    "Weather Rain:Ambient Sunset Color",
    "Weather Rain:Ambient Night Color",
    "Weather Rain:Sun Sunrise Color",
    "Weather Rain:Sun Day Color",
    "Weather Rain:Sun Sunset Color",
    "Weather Rain:Sun Night Color",
    "Weather Rain:Sun Disc Sunset Color",
    "Weather Rain:Land Fog Day Depth",
    "Weather Rain:Land Fog Night Depth",
    "Weather Rain:Wind Speed",
    "Weather Rain:Cloud Speed",
    "Weather Rain:Glare View",
    "Weather Rain:Rain Loop Sound ID",
    "Weather Rain:Using Precip",
    "Weather Rain:Rain Diameter",
    "Weather Rain:Rain Height Min",
    "Weather Rain:Rain Height Max",
    "Weather Rain:Rain Threshold",
    "Weather Rain:Rain Entrance Speed",
    "Weather Rain:Ambient Loop Sound ID",
    "Weather Rain:Max Raindrops",
    "Weather Overcast:Cloud Texture",
    "Weather Overcast:Clouds Maximum Percent",
    "Weather Overcast:Transition Delta",
    "Weather Overcast:Sky Sunrise Color",
    "Weather Overcast:Sky Day Color",
    "Weather Overcast:Sky Sunset Color",
    "Weather Overcast:Sky Night Color",
    "Weather Overcast:Fog Sunrise Color",
    "Weather Overcast:Fog Day Color",
    "Weather Overcast:Fog Sunset Color",
    "Weather Overcast:Fog Night Color",
    "Weather Overcast:Ambient Sunrise Color",
    "Weather Overcast:Ambient Day Color",
    "Weather Overcast:Ambient Sunset Color",
    "Weather Overcast:Ambient Night Color",
    "Weather Overcast:Sun Sunrise Color",
    "Weather Overcast:Sun Day Color",
    "Weather Overcast:Sun Sunset Color",
    "Weather Overcast:Sun Night Color",
    "Weather Overcast:Sun Disc Sunset Color",
    "Weather Overcast:Land Fog Day Depth",
    "Weather Overcast:Land Fog Night Depth",
    "Weather Overcast:Wind Speed",
    "Weather Overcast:Cloud Speed",
    "Weather Overcast:Glare View",
    "Weather Overcast:Ambient Loop Sound ID",
    "Weather Ashstorm:Cloud Texture",
    "Weather Ashstorm:Clouds Maximum Percent",
    "Weather Ashstorm:Transition Delta",
    "Weather Ashstorm:Sky Sunrise Color",
    "Weather Ashstorm:Sky Day Color",
    "Weather Ashstorm:Sky Sunset Color",
    "Weather Ashstorm:Sky Night Color",
    "Weather Ashstorm:Fog Sunrise Color",
    "Weather Ashstorm:Fog Day Color",
    "Weather Ashstorm:Fog Sunset Color",
    "Weather Ashstorm:Fog Night Color",
    "Weather Ashstorm:Ambient Sunrise Color",
    "Weather Ashstorm:Ambient Day Color",
    "Weather Ashstorm:Ambient Sunset Color",
    "Weather Ashstorm:Ambient Night Color",
    "Weather Ashstorm:Sun Sunrise Color",
    "Weather Ashstorm:Sun Day Color",
    "Weather Ashstorm:Sun Sunset Color",
    "Weather Ashstorm:Sun Night Color",
    "Weather Ashstorm:Sun Disc Sunset Color",
    "Weather Ashstorm:Land Fog Day Depth",
    "Weather Ashstorm:Land Fog Night Depth",
    "Weather Ashstorm:Wind Speed",
    "Weather Ashstorm:Cloud Speed",
    "Weather Ashstorm:Glare View",
    "Weather Ashstorm:Ambient Loop Sound ID",
    "Weather Ashstorm:Storm Threshold",
    "Weather Blight:Cloud Texture",
    "Weather Blight:Clouds Maximum Percent",
    "Weather Blight:Transition Delta",
    "Weather Blight:Sky Sunrise Color",
    "Weather Blight:Sky Day Color",
    "Weather Blight:Sky Sunset Color",
    "Weather Blight:Sky Night Color",
    "Weather Blight:Fog Sunrise Color",
    "Weather Blight:Fog Day Color",
    "Weather Blight:Fog Sunset Color",
    "Weather Blight:Fog Night Color",
    "Weather Blight:Ambient Sunrise Color",
    "Weather Blight:Ambient Day Color",
    "Weather Blight:Ambient Sunset Color",
    "Weather Blight:Ambient Night Color",
    "Weather Blight:Sun Sunrise Color",
    "Weather Blight:Sun Day Color",
    "Weather Blight:Sun Sunset Color",
    "Weather Blight:Sun Night Color",
    "Weather Blight:Sun Disc Sunset Color",
    "Weather Blight:Land Fog Day Depth",
    "Weather Blight:Land Fog Night Depth",
    "Weather Blight:Wind Speed",
    "Weather Blight:Cloud Speed",
    "Weather Blight:Glare View",
    "Weather Blight:Ambient Loop Sound ID",
    "Weather Blight:Storm Threshold",
    "Weather Blight:Disease Chance",
    "Weather Snow:Cloud Texture",
    "Weather Snow:Clouds Maximum Percent",
    "Weather Snow:Transition Delta",
    "Weather Snow:Sky Sunrise Color",
    "Weather Snow:Sky Day Color",
    "Weather Snow:Sky Sunset Color",
    "Weather Snow:Sky Night Color",
    "Weather Snow:Fog Sunrise Color",
    "Weather Snow:Fog Day Color",
    "Weather Snow:Fog Sunset Color",
    "Weather Snow:Fog Night Color",
    "Weather Snow:Ambient Sunrise Color",
    "Weather Snow:Ambient Day Color",
    "Weather Snow:Ambient Sunset Color",
    "Weather Snow:Ambient Night Color",
    "Weather Snow:Sun Sunrise Color",
    "Weather Snow:Sun Day Color",
    "Weather Snow:Sun Sunset Color",
    "Weather Snow:Sun Night Color",
    "Weather Snow:Sun Disc Sunset Color",
    "Weather Snow:Land Fog Day Depth",
    "Weather Snow:Land Fog Night Depth",
    "Weather Snow:Wind Speed",
    "Weather Snow:Cloud Speed",
    "Weather Snow:Glare View",
    "Weather Snow:Snow Diameter",
    "Weather Snow:Snow Height Min",
    "Weather Snow:Snow Height Max",
    "Weather Snow:Snow Entrance Speed",
    "Weather Snow:Max Snowflakes",
    "Weather Snow:Ambient Loop Sound ID",
    "Weather Snow:Snow Threshold",
    "Weather Blizzard:Cloud Texture",
    "Weather Blizzard:Clouds Maximum Percent",
    "Weather Blizzard:Transition Delta",
    "Weather Blizzard:Sky Sunrise Color",
    "Weather Blizzard:Sky Day Color",
    "Weather Blizzard:Sky Sunset Color",
    "Weather Blizzard:Sky Night Color",
    "Weather Blizzard:Fog Sunrise Color",
    "Weather Blizzard:Fog Day Color",
    "Weather Blizzard:Fog Sunset Color",
    "Weather Blizzard:Fog Night Color",
    "Weather Blizzard:Ambient Sunrise Color",
    "Weather Blizzard:Ambient Day Color",
    "Weather Blizzard:Ambient Sunset Color",
    "Weather Blizzard:Ambient Night Color",
    "Weather Blizzard:Sun Sunrise Color",
    "Weather Blizzard:Sun Day Color",
    "Weather Blizzard:Sun Sunset Color",
    "Weather Blizzard:Sun Night Color",
    "Weather Blizzard:Sun Disc Sunset Color",
    "Weather Blizzard:Land Fog Day Depth",
    "Weather Blizzard:Land Fog Night Depth",
    "Weather Blizzard:Wind Speed",
    "Weather Blizzard:Cloud Speed",
    "Weather Blizzard:Glare View",
    "Weather Blizzard:Ambient Loop Sound ID",
    "Weather Blizzard:Storm Threshold",
    "Moons:Secunda Size",
    "Moons:Secunda Axis Offset",
    "Moons:Secunda Speed",
    "Moons:Secunda Daily Increment",
    "Moons:Secunda Moon Shadow Early Fade Angle",
    "Moons:Secunda Fade Start Angle",
    "Moons:Secunda Fade End Angle",
    "Moons:Secunda Fade In Start",
    "Moons:Secunda Fade In Finish",
    "Moons:Secunda Fade Out Start",
    "Moons:Secunda Fade Out Finish",
    "Moons:Masser Size",
    "Moons:Masser Axis Offset",
    "Moons:Masser Speed",
    "Moons:Masser Daily Increment",
    "Moons:Masser Moon Shadow Early Fade Angle",
    "Moons:Masser Fade Start Angle",
    "Moons:Masser Fade End Angle",
    "Moons:Masser Fade In Start",
    "Moons:Masser Fade In Finish",
    "Moons:Masser Fade Out Start",
    "Moons:Masser Fade Out Finish",
    "Moons:Script Color",
    "General:Werewolf FOV",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn values<'a>(map: &'a MultiMap, key: &str) -> &'a [String] {
        map.get(key).map_or(&[], Vec::as_slice)
    }

    #[test]
    fn parses_ini_sections_comments_duplicates_and_equals() {
        let parsed = parse_ini_str(
            "[General]\nDisable Audio=1 ; comment\nName=a=b\nName=c\n=ignored\nEmpty=\n[bad\nignored\n",
        );

        assert_eq!(values(&parsed, "General:Disable Audio"), &["1 ".to_owned()]);
        assert_eq!(
            values(&parsed, "General:Name"),
            &["a=b".to_owned(), "c".to_owned()]
        );
        assert!(!parsed.contains_key("General:Empty"));
    }

    #[test]
    fn parses_ini_keys_before_section_like_cpp_importer() {
        let parsed = parse_ini_str("Loose=value\n");
        assert_eq!(values(&parsed, ":Loose"), &["value".to_owned()]);
    }

    #[test]
    fn parses_cfg_trims_and_preserves_inline_hash() {
        let parsed = parse_cfg_str(" # comment\nkey = value # not comment\nkey= second\ninvalid\n");
        assert_eq!(
            values(&parsed, "key"),
            &["value # not comment".to_owned(), "second".to_owned()]
        );
    }

    #[test]
    fn decodes_ini_with_selected_codepage() {
        let parsed = parse_ini_bytes(b"[Movies]\nNew Game=caf\xe9.bik\n", TextEncoding::Win1252);
        assert_eq!(
            values(&parsed, "Movies:New Game"),
            &["caf\u{e9}.bik".to_owned()]
        );
    }

    #[test]
    fn imports_merge_fallback_and_archives() {
        let importer = IniImporter::new(ImportOptions::default());
        let mut cfg = parse_cfg_str("no-sound=0\nfallback=old\n");
        let ini = parse_ini_str(
            "[General]\nDisable Audio=1\n[Fonts]\nFont 0=magic\n[Archives]\nArchive 0=Tribunal.bsa\nArchive 1=Bloodmoon.bsa\n[Movies]\nNew Game=intro.bik\n",
        );

        let result = importer
            .import_maps(&mut cfg, &ini, Path::new("Morrowind.ini"))
            .unwrap();

        assert_eq!(values(&result.cfg, "no-sound"), &["1".to_owned()]);
        assert_eq!(
            values(&result.cfg, "fallback-archive"),
            &[
                "Morrowind.bsa".to_owned(),
                "Tribunal.bsa".to_owned(),
                "Bloodmoon.bsa".to_owned()
            ]
        );
        assert_eq!(
            values(&result.cfg, "fallback"),
            &["Movies_New_Game,intro.bik".to_owned()]
        );
    }

    #[test]
    fn font_import_is_option_gated() {
        let ini = parse_ini_str("[Fonts]\nFont 0=magic\n[Movies]\nNew Game=intro.bik\n");
        let mut cfg = MultiMap::new();
        let importer = IniImporter::new(ImportOptions::default());
        let result = importer
            .import_maps(&mut cfg, &ini, Path::new("Morrowind.ini"))
            .unwrap();
        assert_eq!(
            values(&result.cfg, "fallback"),
            &["Movies_New_Game,intro.bik".to_owned()]
        );

        let mut cfg = MultiMap::new();
        let importer = IniImporter::new(ImportOptions {
            import_fonts: true,
            ..ImportOptions::default()
        });
        let result = importer
            .import_maps(&mut cfg, &ini, Path::new("Morrowind.ini"))
            .unwrap();
        assert_eq!(
            values(&result.cfg, "fallback"),
            &[
                "Fonts_Font_0,magic".to_owned(),
                "Movies_New_Game,intro.bik".to_owned()
            ]
        );
    }

    #[test]
    fn archive_import_stops_at_first_missing_index() {
        let ini = parse_ini_str("[Archives]\nArchive 0=First.bsa\nArchive 2=Skipped.bsa\n");
        let mut cfg = MultiMap::new();
        import_archives(&mut cfg, &ini);
        assert_eq!(
            values(&cfg, "fallback-archive"),
            &["Morrowind.bsa".to_owned(), "First.bsa".to_owned()]
        );
    }

    #[test]
    fn dependency_sort_places_masters_before_dependents() {
        let sorted = dependency_sort(vec![
            ("Patch.esp".to_owned(), vec!["Base.esm".to_owned()]),
            ("Base.esm".to_owned(), vec![]),
        ]);
        assert_eq!(sorted, vec!["Base.esm".to_owned(), "Patch.esp".to_owned()]);
    }

    #[test]
    fn applies_morrowind_expansion_order() {
        let mut files = vec![
            "Morrowind.esm".to_owned(),
            "Bloodmoon.esm".to_owned(),
            "Tribunal.esm".to_owned(),
        ];
        apply_morrowind_expansion_order(&mut files);
        assert_eq!(
            files,
            vec!["Morrowind.esm", "Tribunal.esm", "Bloodmoon.esm"]
        );
    }

    #[test]
    fn reads_tes3_header_masters() {
        let dir = unique_test_dir("tes3-header");
        fs::create_dir_all(&dir).unwrap();
        let plugin = dir.join("Patch.esp");
        fs::write(&plugin, tes3_bytes(&["Morrowind.esm", "Tribunal.esm"])).unwrap();

        let header =
            read_plugin_header(&plugin, PluginFormat::Tes3, TextEncoding::Win1252).unwrap();

        assert_eq!(header.name, "Patch.esp");
        assert_eq!(header.masters, vec!["Morrowind.esm", "Tribunal.esm"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reads_tes3_header_masters_with_selected_encoding() {
        let dir = unique_test_dir("tes3-header-encoding");
        fs::create_dir_all(&dir).unwrap();
        let plugin = dir.join("Patch.esp");
        fs::write(&plugin, tes3_bytes_from_master_bytes(&[b"caf\xe9.esm"])).unwrap();

        let header =
            read_plugin_header(&plugin, PluginFormat::Tes3, TextEncoding::Win1252).unwrap();

        assert_eq!(header.masters, vec!["caf\u{e9}.esm"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_invalid_tes3_header() {
        let dir = unique_test_dir("tes3-invalid");
        fs::create_dir_all(&dir).unwrap();
        let plugin = dir.join("Bad.esp");
        fs::write(&plugin, b"TES4").unwrap();

        let error =
            read_plugin_header(&plugin, PluginFormat::Tes3, TextEncoding::Win1252).unwrap_err();
        assert!(matches!(error, ImportError::InvalidPluginHeader { .. }));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn imports_game_files_using_tes3_dependencies() {
        let dir = unique_test_dir("game-files");
        let data_dir = dir.join("Data Files");
        fs::create_dir_all(&data_dir).unwrap();
        fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();
        fs::write(data_dir.join("Patch.esp"), tes3_bytes(&["Base.esm"])).unwrap();

        let mut cfg = parse_cfg_str(&format!("data={}\n", data_dir.display()));
        let ini = parse_ini_str("[Game Files]\nGameFile0=Patch.esp\nGameFile1=Base.esm\n");
        let importer = IniImporter::new(ImportOptions {
            import_game_files: true,
            import_archives: false,
            ..ImportOptions::default()
        });

        let result = importer
            .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
            .unwrap();

        assert_eq!(
            values(&result.cfg, "content"),
            &["Base.esm".to_owned(), "Patch.esp".to_owned()]
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn imports_into_openmw_config_with_generic_no_sound() {
        let dir = unique_test_dir("openmw-config-import");
        fs::create_dir_all(&dir).unwrap();
        let cfg = dir.join("openmw.cfg");
        let ini = dir.join("Morrowind.ini");
        let output = dir.join("imported.cfg");
        fs::write(
            &cfg,
            "no-sound=0\nfallback=Old_Setting,old\nencoding=win1252\n",
        )
        .unwrap();
        fs::write(
            &ini,
            "[General]\nDisable Audio=1\n[Movies]\nNew Game=intro.bik\n[Archives]\nArchive 0=Tribunal.bsa\n",
        )
        .unwrap();

        let importer = IniImporter::new(ImportOptions::default());
        let result = importer.import_config_paths(&ini, &cfg).unwrap();

        let no_sound: Vec<_> = result
            .config
            .generic_settings_iter()
            .filter(|setting| setting.key() == "no-sound")
            .map(|setting| setting.value().to_owned())
            .collect();
        assert_eq!(no_sound, vec!["1"]);
        assert!(result.config.get_game_setting("Movies_New_Game").is_some());
        assert!(result.config.has_archive_file("Morrowind.bsa"));
        assert!(result.config.has_archive_file("Tribunal.bsa"));

        importer
            .save_config_output(&output, &result.config)
            .unwrap();
        let written = fs::read_to_string(&output).unwrap();
        assert!(written.contains("no-sound=1"));
        assert!(written.contains("fallback=Movies_New_Game,intro.bik"));
        assert!(written.contains("fallback-archive=Morrowind.bsa"));

        fs::remove_dir_all(dir).unwrap();
    }

    fn tes3_bytes(masters: &[&str]) -> Vec<u8> {
        let masters: Vec<_> = masters.iter().map(|master| master.as_bytes()).collect();
        tes3_bytes_from_master_bytes(&masters)
    }

    fn tes3_bytes_from_master_bytes(masters: &[&[u8]]) -> Vec<u8> {
        let mut record = Vec::new();
        subrecord(&mut record, *b"HEDR", &[0; 300]);
        for master in masters {
            let mut name = (*master).to_vec();
            name.push(0);
            subrecord(&mut record, *b"MAST", &name);
            subrecord(&mut record, *b"DATA", &0u64.to_le_bytes());
        }

        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"TES3");
        bytes.extend_from_slice(&u32::try_from(record.len()).unwrap().to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&record);
        bytes
    }

    fn subrecord(output: &mut Vec<u8>, name: [u8; 4], data: &[u8]) {
        output.extend_from_slice(&name);
        output.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
        output.extend_from_slice(data);
    }

    fn unique_test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "rome-ini-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
