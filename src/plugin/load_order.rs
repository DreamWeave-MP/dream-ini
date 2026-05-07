pub(crate) fn dependency_sort(mut source: Vec<(String, Vec<String>)>) -> Vec<String> {
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

pub(crate) fn apply_morrowind_expansion_order(files: &mut Vec<String>) {
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

fn contains_ignore_ascii_case(values: &[String], needle: &str) -> bool {
    position_ignore_ascii_case(values, needle).is_some()
}

fn position_ignore_ascii_case(values: &[String], needle: &str) -> Option<usize> {
    values
        .iter()
        .position(|value| value.eq_ignore_ascii_case(needle))
}
