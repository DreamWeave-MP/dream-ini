// SPDX-License-Identifier: MIT OR Apache-2.0

//! The synthetic import fixture shared by the benchmarks: a large `Morrowind.ini`, a large
//! `openmw.cfg`, and the archives the INI names, in a unique temporary directory.

#![allow(dead_code)]

use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The fixture on disk; removed on drop.
pub struct Fixture {
    pub dir: PathBuf,
    pub ini: PathBuf,
    pub cfg: PathBuf,
}

impl Fixture {
    pub fn create(name: &str) -> Fixture {
        let dir = unique_bench_dir(name);
        fs::create_dir_all(&dir).expect("create benchmark directory");
        create_archives(&dir);
        let ini = dir.join("Morrowind.ini");
        let cfg = dir.join("openmw.cfg");
        fs::write(&ini, large_morrowind_ini()).expect("write benchmark ini");
        fs::write(&cfg, large_openmw_cfg()).expect("write benchmark cfg");
        Fixture { dir, ini, cfg }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

pub fn large_morrowind_ini() -> String {
    let mut ini = String::new();
    ini.push_str("[General]\nDisable Audio=0\n");
    ini.push_str("[Archives]\n");
    for index in 0..128 {
        writeln!(ini, "Archive {index}=Archive{index}.bsa").expect("write archive entry");
    }

    for index in 0..512 {
        write!(
            ini,
            "[Movies]\nNew Game=intro{index}.bik\nCompany Logo=logo{index}.bik\n"
        )
        .expect("write movies section");
        write!(
            ini,
            "[Weather]\nSunrise Time={}\nSunset Time={}\nSun Glare Fader Max=0.75\n",
            5 + index % 3,
            18 + index % 4
        )
        .expect("write weather section");
        write!(
            ini,
            "[Weather Clear]\nSky Day Color={},{},{}\nCloud Texture=cloud{index}.dds\n",
            index % 255,
            (index * 2) % 255,
            (index * 3) % 255
        )
        .expect("write weather clear section");
        write!(
            ini,
            "[Noise Section {index}]\nIgnored Key=value{index}\n; comment {index}\n"
        )
        .expect("write noise section");
    }

    ini
}

pub fn large_openmw_cfg() -> String {
    let mut cfg = String::from("encoding=win1252\nresources=resources\n");
    for index in 0..256 {
        writeln!(cfg, "data=/opt/morrowind/Data Files {index}").expect("write data entry");
        writeln!(cfg, "fallback=Old_Setting_{index},old").expect("write fallback entry");
    }
    cfg
}

fn unique_bench_dir(name: &str) -> PathBuf {
    let temp_dir = std::env::temp_dir();
    let temp_dir = temp_dir.canonicalize().unwrap_or(temp_dir);
    temp_dir.join(format!(
        "dream-ini-bench-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after Unix epoch")
            .as_nanos()
    ))
}

fn create_archives(dir: &Path) {
    let data_dir = dir.join("Data Files");
    fs::create_dir_all(&data_dir).expect("create benchmark data directory");
    fs::write(data_dir.join("Morrowind.bsa"), []).expect("write base archive");
    for index in 0..128 {
        fs::write(data_dir.join(format!("Archive{index}.bsa")), []).expect("write archive");
    }
}
