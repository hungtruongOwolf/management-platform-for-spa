//! sqlx reads `sqlx.toml` from the root of the crate it compiles, so the date and time type
//! overrides (ADR-0026) are copied into every crate that runs `query!`. A copy that drifts would
//! give one crate other Rust types for the same columns: every copy must equal kernel's.

use std::path::Path;

#[test]
fn every_crate_with_sqlx_macros_has_the_same_sqlx_toml() -> Result<(), Box<dyn std::error::Error>> {
    let kernel = Path::new(env!("CARGO_MANIFEST_DIR"));
    let expected = std::fs::read_to_string(kernel.join("sqlx.toml"))?;
    let crates = kernel.parent().ok_or("crates dir")?;
    let mut checked = 0;
    for entry in std::fs::read_dir(crates)? {
        let dir = entry?.path();
        let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap_or_default();
        if !manifest.contains("sqlx") {
            continue;
        }
        let copy = std::fs::read_to_string(dir.join("sqlx.toml"))
            .map_err(|e| format!("{}: sqlx.toml missing ({e})", dir.display()))?;
        assert_eq!(
            copy.replace("\r\n", "\n"),
            expected.replace("\r\n", "\n"),
            "{}/sqlx.toml differs from kernel's",
            dir.display()
        );
        checked += 1;
    }
    assert!(checked > 1, "found the workspace crates");
    Ok(())
}
