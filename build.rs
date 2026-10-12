//! Embed the reproducible browser bundle, including its fonts and PDF worker.
use std::{fs, path::Path};
fn walk(directory: &Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(directory)
        .expect("Run npm ci && npm run build before building GitLeaf")
        .map(Result::unwrap)
    {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, files);
        } else {
            files.push(path);
        }
    }
}
fn main() {
    println!("cargo:rerun-if-changed=static/assets");
    let base = std::env::current_dir().unwrap().join("static/assets");
    assert!(
        base.join("app.js").is_file(),
        "Run npm ci && npm run build to create static/assets/app.js"
    );
    let mut files = Vec::new();
    walk(&base, &mut files);
    files.sort();
    let mut code =
        String::from("pub fn find(path: &str) -> Option<&'static [u8]> { match path {\n");
    for file in files {
        let relative = file
            .strip_prefix(&base)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        code.push_str(&format!(
            "{relative:?} => Some(include_bytes!({:?})),\n",
            file.to_str().unwrap()
        ));
    }
    code.push_str("_ => None } }\n");
    fs::write(
        Path::new(&std::env::var("OUT_DIR").unwrap()).join("assets.rs"),
        code,
    )
    .unwrap();
}
