use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use encoding_rs::{Encoding, WINDOWS_1250, WINDOWS_1251, WINDOWS_1252};
use openmw_config::EncodingSetting;

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
}

#[derive(Debug, Clone)]
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
    pub fn new(options: ImportOptions) -> Self {
        Self { options }
    }

    pub fn import_paths(&self, ini_path: &Path, cfg_path: &Path) -> Result<ImportResult, ImportError> {
        let cfg_text = read_to_string(cfg_path)?;
        let mut cfg = parse_cfg_str(&cfg_text);

        let encoding = self.effective_encoding(&cfg)?;
        set_single_value(&mut cfg, "encoding", encoding.as_label().to_owned());

        let ini_bytes = read_bytes(ini_path)?;
        let ini = parse_ini_bytes(&ini_bytes, encoding);
        self.import_maps(&mut cfg, &ini, ini_path)
    }

    pub fn write_output(&self, output_path: &Path, cfg: &MultiMap) -> Result<(), ImportError> {
        write_cfg_via_openmw_config(output_path, cfg)
    }

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
        data_paths.push(ini_path.parent().unwrap_or_else(|| Path::new("")).join("Data Files"));

        let mut content_files = Vec::new();
        for file in sequential_ini_values(ini, "Game Files:GameFile") {
            if !ends_with_ignore_ascii_case(file, ".esm") && !ends_with_ignore_ascii_case(file, ".esp") {
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

        content_files.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));

        let format = self.options.game.plugin_format();
        let mut dependencies = Vec::new();
        for (_, path) in content_files {
            let header = read_plugin_header(&path, format)?;
            dependencies.push((header.name, header.masters));
        }

        let mut sorted = dependency_sort(dependencies);
        apply_morrowind_expansion_order(&mut sorted);
        cfg.insert("content".to_owned(), sorted);

        Ok(())
    }
}

pub fn parse_ini_bytes(bytes: &[u8], encoding: TextEncoding) -> MultiMap {
    let (decoded, _, _) = encoding.encoding_rs().decode(bytes);
    parse_ini_str(&decoded)
}

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
            section = line[1..end].to_owned();
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
                insert_multimap(cfg, "fallback".to_owned(), format!("{fallback_key},{value}"));
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
    while let Some((element, _)) = source.first().cloned() {
        dependency_sort_step(element, &mut source, &mut result);
    }
    result
}

fn dependency_sort_step(element: String, source: &mut Vec<(String, Vec<String>)>, result: &mut Vec<String>) {
    let Some(index) = source.iter().position(|(name, _)| *name == element) else {
        return;
    };
    let (name, dependencies) = source.remove(index);
    for dependency in dependencies {
        dependency_sort_step(dependency, source, result);
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
        let bloodmoon_index = position_ignore_ascii_case(files, "Bloodmoon.esm").expect("Bloodmoon.esm remains present");
        files.insert(bloodmoon_index, tribunal);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginHeader {
    pub name: String,
    pub masters: Vec<String>,
}

pub fn read_plugin_header(path: &Path, format: PluginFormat) -> Result<PluginHeader, ImportError> {
    match format {
        PluginFormat::Tes3 => read_tes3_header(path),
    }
}

fn read_tes3_header(path: &Path) -> Result<PluginHeader, ImportError> {
    let bytes = read_bytes(path)?;
    if bytes.len() < 16 || &bytes[0..4] != b"TES3" {
        return Err(ImportError::InvalidPluginHeader {
            path: path.to_owned(),
            message: "missing TES3 record".to_owned(),
        });
    }

    let record_size = read_u32_le(&bytes, 4, path)? as usize;
    let record_start = 16usize;
    let record_end = record_start.checked_add(record_size).ok_or_else(|| ImportError::InvalidPluginHeader {
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
            masters.push(read_c_string(&bytes[offset..offset + size]));
        }

        offset += size;
    }

    Ok(PluginHeader {
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string()),
        masters,
    })
}

fn write_cfg_via_openmw_config(output_path: &Path, cfg: &MultiMap) -> Result<(), ImportError> {
    let mut config = openmw_config::OpenMWConfiguration::new(Some(output_path.to_owned()))
        .map_err(|error| ImportError::OpenMwConfig(error.to_string()))?;

    if let Some(values) = cfg.get("content") {
        config.set_content_files(Some(values.clone()));
    }
    if let Some(values) = cfg.get("fallback-archive") {
        config.set_fallback_archives(Some(values.clone()));
    }
    if let Some(values) = cfg.get("data") {
        config.set_data_directories(Some(values.iter().map(PathBuf::from).collect()));
    }
    if let Some(value) = cfg.get("encoding").and_then(|values| values.last()) {
        let mut empty = String::new();
        let setting = EncodingSetting::try_from((value.clone(), output_path, &mut empty))
            .map_err(|error| ImportError::OpenMwConfig(error.to_string()))?;
        config.set_encoding(Some(setting));
    }

    if let Some(values) = cfg.get("fallback") {
        config
            .set_game_settings(Some(values.clone()))
            .map_err(|error| ImportError::OpenMwConfig(error.to_string()))?;
    }

    fs::write(output_path, config.to_string()).map_err(|source| ImportError::Io {
        path: output_path.to_owned(),
        source,
    })
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
    values.iter().position(|value| value.eq_ignore_ascii_case(needle))
}

fn system_time_key(time: SystemTime) -> u128 {
    time.duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos()
}

fn read_u32_le(bytes: &[u8], offset: usize, path: &Path) -> Result<u32, ImportError> {
    let bytes = bytes.get(offset..offset + 4).ok_or_else(|| ImportError::InvalidPluginHeader {
        path: path.to_owned(),
        message: "unexpected end of file".to_owned(),
    })?;
    Ok(u32::from_le_bytes(bytes.try_into().expect("slice length checked")))
}

fn read_c_string(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

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
    "Fonts:Font 0",
    "Fonts:Font 1",
    "Fonts:Font 2",
    "Movies:Company Logo",
    "Movies:Morrowind Logo",
    "Movies:New Game",
    "Movies:Loading",
    "Movies:Options Menu",
    "General:Werewolf FOV",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn values<'a>(map: &'a MultiMap, key: &str) -> &'a [String] {
        map.get(key).map(Vec::as_slice).unwrap_or(&[])
    }

    #[test]
    fn parses_ini_sections_comments_duplicates_and_equals() {
        let parsed = parse_ini_str("[General]\nDisable Audio=1 ; comment\nName=a=b\nName=c\n=ignored\nEmpty=\n[bad\nignored\n");

        assert_eq!(values(&parsed, "General:Disable Audio"), &["1 ".to_owned()]);
        assert_eq!(values(&parsed, "General:Name"), &["a=b".to_owned(), "c".to_owned()]);
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
        assert_eq!(values(&parsed, "key"), &["value # not comment".to_owned(), "second".to_owned()]);
    }

    #[test]
    fn imports_merge_fallback_and_archives() {
        let importer = IniImporter::new(ImportOptions::default());
        let mut cfg = parse_cfg_str("no-sound=0\nfallback=old\n");
        let ini = parse_ini_str(
            "[General]\nDisable Audio=1\n[Fonts]\nFont 0=magic\n[Archives]\nArchive 0=Tribunal.bsa\nArchive 1=Bloodmoon.bsa\n[Movies]\nNew Game=intro.bik\n",
        );

        let result = importer.import_maps(&mut cfg, &ini, Path::new("Morrowind.ini")).unwrap();

        assert_eq!(values(&result.cfg, "no-sound"), &["1".to_owned()]);
        assert_eq!(
            values(&result.cfg, "fallback-archive"),
            &["Morrowind.bsa".to_owned(), "Tribunal.bsa".to_owned(), "Bloodmoon.bsa".to_owned()]
        );
        assert_eq!(values(&result.cfg, "fallback"), &["Movies_New_Game,intro.bik".to_owned()]);
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
        let mut files = vec!["Morrowind.esm".to_owned(), "Bloodmoon.esm".to_owned(), "Tribunal.esm".to_owned()];
        apply_morrowind_expansion_order(&mut files);
        assert_eq!(files, vec!["Morrowind.esm", "Tribunal.esm", "Bloodmoon.esm"]);
    }

    #[test]
    fn reads_tes3_header_masters() {
        let dir = unique_test_dir("tes3-header");
        fs::create_dir_all(&dir).unwrap();
        let plugin = dir.join("Patch.esp");
        fs::write(&plugin, tes3_bytes(&["Morrowind.esm", "Tribunal.esm"])).unwrap();

        let header = read_plugin_header(&plugin, PluginFormat::Tes3).unwrap();

        assert_eq!(header.name, "Patch.esp");
        assert_eq!(header.masters, vec!["Morrowind.esm", "Tribunal.esm"]);
        fs::remove_dir_all(dir).unwrap();
    }

    fn tes3_bytes(masters: &[&str]) -> Vec<u8> {
        let mut record = Vec::new();
        subrecord(&mut record, b"HEDR", &[0; 300]);
        for master in masters {
            let mut name = master.as_bytes().to_vec();
            name.push(0);
            subrecord(&mut record, b"MAST", &name);
            subrecord(&mut record, b"DATA", &0u64.to_le_bytes());
        }

        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"TES3");
        bytes.extend_from_slice(&(record.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&record);
        bytes
    }

    fn subrecord(output: &mut Vec<u8>, name: &[u8; 4], data: &[u8]) {
        output.extend_from_slice(name);
        output.extend_from_slice(&(data.len() as u32).to_le_bytes());
        output.extend_from_slice(data);
    }

    fn unique_test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "rome-ini-{name}-{}",
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ))
    }
}
