//! Contract guard: every route constant in `cce-api::paths` is either called
//! by the client or explicitly declared out of scope.
//!
//! The check reads sources instead of a hand-kept table, so adding a route
//! constant without a client method fails here rather than waiting for a
//! consumer to notice.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Route constants deliberately absent from the Rust client.
///
/// The log-file endpoints exist for the web console only: no Rust consumer
/// calls them, and their payloads are not modeled in `cce-api`, so exposing
/// them here would force contract models without a consumer.
const OUT_OF_SCOPE: &[&str] = &[
    "LOGS",
    "LOGS_FILES",
    "LOGS_FILE_TEMPLATE",
    "LOGS_FILE_DOWNLOAD_TEMPLATE",
    "LOGS_ROTATE",
    "LOGS_CONFIG",
    "LOGS_WS",
];

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn paths_source() -> String {
    let path = manifest_dir().join("../cce-api/src/paths.rs");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Collect `pub const NAME: &str = "/..."` route constants, including
/// declarations whose value sits on the following line.
fn route_constants(source: &str) -> BTreeSet<String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut names = BTreeSet::new();
    for (index, line) in lines.iter().enumerate() {
        let Some(rest) = line.trim().strip_prefix("pub const ") else {
            continue;
        };
        let Some((name, tail)) = rest.split_once(':') else {
            continue;
        };
        let continuation = lines.get(index + 1).copied().unwrap_or_default();
        let declared = format!("{tail}{continuation}");
        if declared.contains("= \"/") {
            names.insert(name.trim().to_string());
        }
    }
    names
}

fn client_sources() -> String {
    fn walk(dir: &Path, out: &mut String) {
        for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display())) {
            let entry = entry.expect("directory entry");
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                out.push_str(&fs::read_to_string(&path).expect("read client source"));
                out.push('\n');
            }
        }
    }

    let mut out = String::new();
    walk(&manifest_dir().join("src"), &mut out);
    out
}

#[test]
fn every_route_constant_has_a_client_call_site() {
    let constants = route_constants(&paths_source());
    assert!(
        constants.len() > 60,
        "route constant parsing broke: {} found",
        constants.len()
    );
    let sources = client_sources();
    let missing: Vec<&str> = constants
        .iter()
        .map(String::as_str)
        .filter(|name| !OUT_OF_SCOPE.contains(name))
        .filter(|name| !sources.contains(&format!("paths::{name}")))
        .collect();
    assert!(
        missing.is_empty(),
        "route constants without a client method: {missing:?}"
    );
}

#[test]
fn out_of_scope_entries_are_live_constants() {
    let constants = route_constants(&paths_source());
    let stale: Vec<&str> = OUT_OF_SCOPE
        .iter()
        .copied()
        .filter(|name| !constants.contains(*name))
        .collect();
    assert!(stale.is_empty(), "stale out-of-scope entries: {stale:?}");
}
