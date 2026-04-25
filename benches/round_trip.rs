use std::fmt::Write;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use rome_ini::{ImportOptions, IniImporter, serialize_cfg};

fn round_trip(c: &mut Criterion) {
    let dir = unique_bench_dir();
    fs::create_dir_all(&dir).expect("create benchmark directory");
    let ini = dir.join("Morrowind.ini");
    let cfg = dir.join("openmw.cfg");

    fs::write(&ini, large_morrowind_ini()).expect("write benchmark ini");
    fs::write(&cfg, large_openmw_cfg()).expect("write benchmark cfg");

    let importer = IniImporter::new(ImportOptions::default());
    c.bench_function("large_ini_round_trip", |b| {
        b.iter(|| {
            let result = importer
                .import_paths(black_box(&ini), black_box(&cfg))
                .expect("import benchmark files");
            black_box(serialize_cfg(&result.cfg));
        });
    });

    let _ = fs::remove_dir_all(dir);
}

fn large_morrowind_ini() -> String {
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

fn large_openmw_cfg() -> String {
    let mut cfg = String::from("encoding=win1252\nresources=resources\n");
    for index in 0..256 {
        writeln!(cfg, "data=/opt/morrowind/Data Files {index}").expect("write data entry");
        writeln!(cfg, "fallback=Old_Setting_{index},old").expect("write fallback entry");
    }
    cfg
}

fn unique_bench_dir() -> PathBuf {
    std::env::temp_dir().join(format!(
        "rome-ini-bench-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after Unix epoch")
            .as_nanos()
    ))
}

criterion_group!(benches, round_trip);
criterion_main!(benches);
