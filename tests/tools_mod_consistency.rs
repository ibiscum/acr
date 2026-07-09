use std::collections::BTreeSet;
use std::fs;

fn declared_modules_from_mod_rs() -> BTreeSet<String> {
    let content = fs::read_to_string("src/tools/mod.rs").expect("failed to read src/tools/mod.rs");
    content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            trimmed
                .strip_prefix("pub mod ")
                .and_then(|rest| rest.strip_suffix(';'))
                .map(|name| name.to_string())
        })
        .collect()
}

fn modules_from_tools_directory() -> BTreeSet<String> {
    fs::read_dir("src/tools")
        .expect("failed to read src/tools directory")
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let file_name = path.file_name()?.to_str()?;
            if !file_name.ends_with(".rs") || file_name == "mod.rs" {
                return None;
            }
            let stem = path.file_stem()?.to_str()?;
            if !stem.starts_with("acr_") {
                return None;
            }
            Some(stem.to_string())
        })
        .collect()
}

#[test]
fn regression_tools_mod_exports_all_tool_modules() {
    let declared = declared_modules_from_mod_rs();
    let expected = modules_from_tools_directory();

    assert_eq!(declared, expected, "src/tools/mod.rs exports drifted from src/tools/*.rs");
}
