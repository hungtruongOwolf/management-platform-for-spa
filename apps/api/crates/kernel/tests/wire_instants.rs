//! Instants leave the API as chrono wrote them (ADR-0026): every `Timestamp` field of a type that
//! derives `Serialize` goes through `kernel::time::rfc3339`. jiff's own serializer would still
//! compile and still be RFC 3339, only with other text (`.12Z` for `.120Z`), so nothing else
//! catches a forgotten attribute. This reads the sources, like `authz_matrix.rs` reads the router.

use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            rust_files(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// A field line whose type is an instant (`at: Timestamp,`, `Option<jiff::Timestamp>`, …).
fn is_instant_field(line: &str) -> bool {
    let Some((name, ty)) = line.trim().trim_start_matches("pub ").split_once(": ") else {
        return false;
    };
    let ty = ty.trim_end_matches(',').trim();
    !name.contains(' ')
        && matches!(
            ty,
            "Timestamp" | "jiff::Timestamp" | "Option<Timestamp>" | "Option<jiff::Timestamp>"
        )
}

/// `file:line` of every instant field of a `Serialize` struct or enum without the attribute.
fn unformatted_instants(file: &Path) -> std::io::Result<Vec<String>> {
    let text = std::fs::read_to_string(file)?;
    let lines: Vec<&str> = text.lines().collect();
    let mut found = Vec::new();
    // Inside the body of an item whose derive list has `Serialize`: the brace depth it opened at.
    let mut serialized_from: Option<usize> = None;
    let mut pending_serialize = false;
    let mut depth = 0usize;
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("#[derive(") && trimmed.contains("Serialize") {
            pending_serialize = true;
        }
        let opens_item = (trimmed.contains("struct ") || trimmed.contains("enum "))
            && !trimmed.starts_with("//");
        if pending_serialize && opens_item && line.contains('{') {
            serialized_from = Some(depth);
            pending_serialize = false;
        } else if opens_item {
            pending_serialize = false;
        }
        if serialized_from.is_some() && is_instant_field(line) {
            // The field's own attributes and doc comment: the lines right above it, up to the
            // previous field.
            let own = lines[..i].iter().rev().take_while(|l| {
                let l = l.trim_start();
                l.starts_with("#[") || l.starts_with("///") || l.starts_with("//")
            });
            if !own.into_iter().any(|l| l.contains("rfc3339")) {
                found.push(format!("{}:{}: {}", file.display(), i + 1, line.trim()));
            }
        }
        depth += line.matches('{').count();
        depth = depth.saturating_sub(line.matches('}').count());
        if serialized_from.is_some_and(|d| depth <= d) {
            serialized_from = None;
        }
    }
    Ok(found)
}

#[test]
fn every_serialized_instant_keeps_the_api_text() -> Result<(), Box<dyn std::error::Error>> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("crates dir")?;
    let mut files = Vec::new();
    for entry in std::fs::read_dir(crates)? {
        let src = entry?.path().join("src");
        if src.is_dir() {
            rust_files(&src, &mut files)?;
        }
    }
    assert!(files.len() > 50, "found the workspace sources");
    let mut missing = Vec::new();
    for file in &files {
        missing.extend(unformatted_instants(file)?);
    }
    assert!(
        missing.is_empty(),
        "add #[serde(with = \"kernel::time::rfc3339\")] (or serialize_with = \
         \"kernel::time::rfc3339::serialize_option\" for an Option) to:\n{}",
        missing.join("\n")
    );
    Ok(())
}

#[test]
fn the_scan_sees_a_forgotten_attribute() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!("wire-instants-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let file = dir.join("dto.rs");
    std::fs::write(
        &file,
        "#[derive(Debug, Serialize)]\npub struct Dto {\n    \
         #[serde(with = \"kernel::time::rfc3339\")]\n    pub ok: Timestamp,\n    \
         pub missed: Option<Timestamp>,\n}\n\npub struct Row {\n    pub at: Timestamp,\n}\n",
    )?;
    let found = unformatted_instants(&file)?;
    std::fs::remove_dir_all(&dir)?;
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].ends_with("pub missed: Option<Timestamp>,"),
        "{found:?}"
    );
    Ok(())
}
