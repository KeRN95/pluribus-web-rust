// Rebuild when the English catalogue changes, which becomes a sorted table of the Albanian texts
// and their English (src/i18n.rs). The catalogue is the repository's i18n/, two levels up from
// this crate.
fn main() {
    println!("cargo:rerun-if-changed=../../i18n");
    english();
}

/// The catalogue, i18n/en/*.txt: each entry the Albanian text on a line of its own and its English
/// on the next, after "= ", the entries apart by a blank line, a line starting with "#" a comment.
/// An entry given twice with the same English is one; with another English, or without its
/// English, it stops the build. The files whose names start with "js" hold the scripts' texts,
/// which make a table of their own too (EN_JS), that English pages carry for them.
fn english() {
    use std::{collections::BTreeMap, fs, path::Path};

    let mut entries: BTreeMap<String, (String, String)> = BTreeMap::new();
    let mut scripts: BTreeMap<String, String> = BTreeMap::new();
    let mut files: Vec<_> = fs::read_dir("../../i18n/en")
        .map(|dir| dir.filter_map(Result::ok).map(|entry| entry.path()).filter(|path| path.extension().is_some_and(|ext| ext == "txt")).collect())
        .unwrap_or_default();
    files.sort();
    for file in files {
        let text = fs::read_to_string(&file).unwrap_or_else(|error| panic!("{}: {error}", file.display()));
        let name = file.display().to_string();
        let script = file.file_name().is_some_and(|file| file.to_string_lossy().starts_with("js"));
        let mut albanian: Option<(usize, String)> = None;
        for (number, line) in text.lines().enumerate().map(|(number, line)| (number + 1, line.trim_end())) {
            if line.starts_with('#') {
                continue;
            }
            match (line, &albanian) {
                ("", None) => {}
                ("", Some((at, key))) => panic!("{name}:{at}: „{key}“ has no English (\"= ...\" on the next line)"),
                (line, Some(_)) if line.starts_with("= ") => {
                    let (at, key) = albanian.take().expect("matched");
                    let english = line[2..].to_string();
                    if script {
                        scripts.insert(key.clone(), english.clone());
                    }
                    match entries.get(&key) {
                        Some((before, place)) if *before != english => {
                            panic!("{name}:{at}: „{key}“ is „{english}“ here and „{before}“ at {place}")
                        }
                        Some(_) => {}
                        None => {
                            entries.insert(key, (english, format!("{name}:{at}")));
                        }
                    }
                }
                (line, Some((at, key))) => panic!("{name}:{at}: „{key}“ is followed by „{line}“ where its English should be"),
                (line, None) if line.starts_with("= ") => panic!("{name}:{number}: English with no Albanian before it"),
                (line, None) => albanian = Some((number, line.to_string())),
            }
        }
        if let Some((at, key)) = albanian {
            panic!("{name}:{at}: „{key}“ has no English");
        }
    }
    let mut table = String::from("/// The English catalogue (i18n/en/), sorted by the Albanian, for a binary search.\npub static EN: &[(&str, &str)] = &[\n");
    for (key, (english, _)) in &entries {
        table.push_str(&format!("    ({key:?}, {english:?}),\n"));
    }
    table.push_str("];\n");
    table.push_str("/// The scripts' texts (i18n/en/js*.txt), as English pages carry them for i18n.js.\npub static EN_JS: &[(&str, &str)] = &[\n");
    for (key, english) in &scripts {
        table.push_str(&format!("    ({key:?}, {english:?}),\n"));
    }
    table.push_str("];\n");
    let out = std::env::var("OUT_DIR").expect("cargo sets OUT_DIR");
    fs::write(Path::new(&out).join("i18n_en.rs"), table).expect("the catalogue's table is written");
}
