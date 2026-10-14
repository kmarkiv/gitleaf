//! File classification and bounded main-document detection, shared by imports and the explorer.
use std::{io::Read, path::Path};

pub fn generated(path: &str) -> bool {
    let (top, _) = path.split_once('/').unwrap_or(("", ""));
    if matches!(
        top,
        "output" | "build" | "tmp" | "_minted" | ".cache" | "out" | "auxil" | "_to_delete"
    ) {
        return true;
    }
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "aux"
            | "log"
            | "out"
            | "bbl"
            | "blg"
            | "fls"
            | "fdb_latexmk"
            | "synctex"
            | "gz"
            | "toc"
            | "lof"
            | "lot"
            | "nav"
            | "snm"
            | "vrb"
            | "xdv"
            | "bcf"
            | "idx"
            | "ind"
            | "ilg"
    ) || path.ends_with(".run.xml")
}

fn documentclass(source: &str) -> bool {
    fn escaped(line: &str, offset: usize) -> bool {
        line.as_bytes()[..offset]
            .iter()
            .rev()
            .take_while(|b| **b == b'\\')
            .count()
            % 2
            == 1
    }
    source.lines().any(|line| {
        let end = line
            .match_indices('%')
            .find(|(i, _)| !escaped(line, *i))
            .map(|(i, _)| i)
            .unwrap_or(line.len());
        let line = &line[..end];
        line.match_indices("\\documentclass").any(|(i, command)| {
            !escaped(line, i)
                && !line
                    .as_bytes()
                    .get(i + command.len())
                    .is_some_and(u8::is_ascii_alphabetic)
        })
    })
}

pub fn main_file<'a>(root: &Path, paths: impl Iterator<Item = &'a str>) -> Option<String> {
    let mut tex: Vec<_> = paths
        .filter(|p| {
            !generated(p)
                && p.rsplit('.')
                    .next()
                    .is_some_and(|e| e.eq_ignore_ascii_case("tex"))
        })
        .collect();
    tex.sort_by_key(|p| (p.bytes().filter(|b| *b == b'/').count(), *p));
    if tex.contains(&"main.tex") {
        return Some("main.tex".into());
    }
    for path in &tex {
        let mut bytes = Vec::new();
        if let Ok(file) = std::fs::File::open(root.join(path)) {
            if file.take(64 * 1024).read_to_end(&mut bytes).is_ok()
                && documentclass(&String::from_utf8_lossy(&bytes))
            {
                return Some((*path).into());
            }
        }
    }
    tex.first().map(|p| (*p).into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recognizes_document_classes_without_being_misled_by_comments() {
        assert!(documentclass("\\documentclass[12pt]{article}"));
        assert!(!documentclass("% \\documentclass{article}\nBody"));
        assert!(!documentclass("\\documentclasses{wrong}"));
        assert!(!documentclass("\\\\documentclass{escaped}"));
        assert!(documentclass("\\% \\documentclass{article}"));
        assert!(generated("build/main.tex"));
        assert!(generated("main.AUX"));
        assert!(generated("main.run.xml"));
        assert!(!generated("chapters/main.tex"));
    }
}
