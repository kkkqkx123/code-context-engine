//! End-to-end test for the native plugin path: compiles a C plugin against
//! `plugin-sdk/include/cce_plugin.h` (i.e. a plugin that does NOT use the
//! Rust SDK), loads it through the host loader, and exercises the full
//! `CodePlugin` surface including batch generation and `error_type`
//! restoration.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use cce_plugin::{CodePlugin, PluginError};
use cce_plugin_runtime::NativePlugin;
use cce_types::grouper::{EntityGroup, GroupType};

/// Compile the C fixture against `cce_plugin.h` into a shared library and
/// return its path. The temp directory is leaked (tests only) so the library
/// stays on disk for the duration of the test process.
fn build_c_plugin() -> &'static PathBuf {
    static ARTIFACT: OnceLock<&'static PathBuf> = OnceLock::new();
    ARTIFACT.get_or_init(|| {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let src = manifest.join("tests/fixtures/c_plugin.c");
        let header_dir = manifest.join("../../plugin-sdk/include");

        let dir = tempfile::Builder::new()
            .prefix("cce-e2e-c-plugin-")
            .tempdir()
            .expect("failed to create temp dir for C plugin")
            .keep();
        let artifact = dir.join(format!(
            "{prefix}{name}{suffix}",
            prefix = std::env::consts::DLL_PREFIX,
            name = "cce_e2e_c_plugin",
            suffix = std::env::consts::DLL_SUFFIX,
        ));

        let compiler = std::env::var_os("CC")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("cc"));
        let status = Command::new(&compiler)
            .arg("-shared")
            .arg("-fPIC")
            .arg("-O2")
            .arg(format!("-I{}", header_dir.display()))
            .arg(&src)
            .arg("-o")
            .arg(&artifact)
            .status()
            .unwrap_or_else(|e| {
                panic!("failed to invoke C compiler {compiler:?}: {e}");
            });
        assert!(
            status.success(),
            "C compiler failed to build the e2e plugin: {status:?}"
        );
        Box::leak(Box::new(artifact))
    })
}

fn plugin_path() -> &'static PathBuf {
    build_c_plugin()
}

fn load_plugin() -> &'static NativePlugin {
    static PLUGIN: OnceLock<NativePlugin> = OnceLock::new();
    PLUGIN.get_or_init(|| {
        NativePlugin::load(plugin_path()).expect("failed to load compiled C plugin")
    })
}

fn make_group(id: &str, name: &str) -> EntityGroup {
    let mut group = EntityGroup::new(id.to_string(), GroupType::Standalone);
    group.name = name.to_string().into();
    group
}

#[test]
fn test_load_and_metadata() {
    let plugin = load_plugin();
    let meta = plugin.metadata();
    assert_eq!(meta.id, "e2e/c-plugin");
    assert_eq!(meta.name, "C E2E Plugin");
    assert_eq!(meta.version, "0.1.0");
    assert_eq!(meta.priority, 7);
}

#[test]
fn test_capabilities() {
    let plugin = load_plugin();
    assert!(plugin.supports_bm25());
    assert!(plugin.supports_embedding());
}

#[test]
fn test_generate_bm25_single() {
    let plugin = load_plugin();
    let group = make_group("g1", "handler");
    let result = plugin.generate_bm25(&group).expect("bm25 call failed");
    assert_eq!(result.as_deref(), Some("C-plugin-bm25"));
}

#[test]
fn test_generate_bm25_single_skip() {
    let plugin = load_plugin();
    // The C plugin returns {"result":"none"} for groups whose name contains
    // "skip" — the host must surface that as Ok(None) (fall back to built-in).
    let group = make_group("g2", "skipgroup");
    let result = plugin.generate_bm25(&group).expect("bm25 call failed");
    assert_eq!(result, None);
}

#[test]
fn test_generate_bm25_batch() {
    let plugin = load_plugin();
    let groups = [
        make_group("g1", "a"),
        make_group("g2", "b"),
        make_group("g3", "c"),
    ];
    let refs: Vec<&EntityGroup> = groups.iter().collect();
    let results = plugin
        .generate_bm25_batch(&refs)
        .expect("bm25 batch call failed");
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].as_deref(), Some("batch-bm25-0"));
    assert_eq!(results[1], None); // C plugin emits null for the second group
    assert_eq!(results[2].as_deref(), Some("batch-bm25-2"));
}

#[test]
fn test_generate_embedding_error_type_restored() {
    let plugin = load_plugin();
    let group = make_group("g1", "handler");
    let result = plugin.generate_embedding(&group);
    // The C plugin replies with error_type "logic"; the host must restore
    // the concrete PluginError::LogicError variant instead of ScriptError.
    match result {
        Err(PluginError::LogicError(msg)) => {
            assert!(msg.contains("embedding unsupported"));
        }
        other => panic!("expected LogicError, got: {other:?}"),
    }
}

#[test]
fn test_generate_embedding_batch_error_type_restored() {
    let plugin = load_plugin();
    let group = make_group("g1", "a");
    let refs = vec![&group];
    let result = plugin.generate_embedding_batch(&refs);
    match result {
        Err(PluginError::LogicError(msg)) => {
            assert!(msg.contains("embedding batch unsupported"));
        }
        other => panic!("expected LogicError, got: {other:?}"),
    }
}
