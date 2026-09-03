use crate::error::PluginError;

use super::ffi_helpers::{ffi_error, parse_ffi_result, symbol_missing_err};
use super::native_plugin::{CURRENT_ABI_VERSION, MINIMUM_ABI_VERSION, NativePlugin};

use std::path::PathBuf;

// ── Unit tests (no actual FFI loading) ──

#[test]
fn test_parse_ffi_result_ok_some() {
    let json = r#"{"result":"ok","value":"hello"}"#;
    let result: Result<Option<String>, PluginError> = parse_ffi_result(json);
    assert_eq!(result.unwrap(), Some("hello".to_string()));
}

#[test]
fn test_parse_ffi_result_ok_none() {
    let json = r#"{"result":"ok","value":null}"#;
    let result: Result<Option<String>, PluginError> = parse_ffi_result(json);
    assert_eq!(result.unwrap(), None);
}

#[test]
fn test_parse_ffi_result_none() {
    let json = r#"{"result":"none"}"#;
    let result: Result<Option<String>, PluginError> = parse_ffi_result(json);
    assert_eq!(result.unwrap(), None);
}

#[test]
fn test_parse_ffi_result_error() {
    let json = r#"{"result":"error","message":"something broke"}"#;
    let result: Result<Option<String>, PluginError> = parse_ffi_result(json);
    match result {
        Err(PluginError::ScriptError(msg)) => {
            assert!(msg.contains("something broke"));
        }
        _ => panic!("Expected ScriptError"),
    }
}

#[test]
fn test_parse_ffi_result_error_no_message() {
    let json = r#"{"result":"error"}"#;
    let result: Result<Option<String>, PluginError> = parse_ffi_result(json);
    match result {
        Err(PluginError::ScriptError(msg)) => {
            assert!(!msg.is_empty());
        }
        _ => panic!("Expected ScriptError"),
    }
}

#[test]
fn test_parse_ffi_result_unknown_result_type() {
    let json = r#"{"result":"bogus"}"#;
    let result: Result<Option<String>, PluginError> = parse_ffi_result(json);
    match result {
        Err(PluginError::InvalidOutput(msg)) => {
            assert!(msg.contains("bogus"));
        }
        _ => panic!("Expected InvalidOutput"),
    }
}

#[test]
fn test_parse_ffi_result_invalid_json() {
    let result: Result<Option<String>, PluginError> = parse_ffi_result("not json");
    match result {
        Err(PluginError::InvalidOutput(_)) => {}
        _ => panic!("Expected InvalidOutput"),
    }
}

#[test]
fn test_load_nonexistent_library() {
    let path = PathBuf::from("nonexistent_plugin.so");
    let result = NativePlugin::load(&path);
    assert!(result.is_err());
    match result {
        Err(PluginError::ResourceError(msg)) => {
            assert!(msg.contains("Failed to load native plugin"));
        }
        _ => panic!("Expected ResourceError"),
    }
}

/// Verify that loading a real system library (without CCE plugin symbols)
/// fails with a ResourceError about missing symbols.
#[test]
fn test_load_system_library_missing_symbols() {
    let system_lib = if cfg!(target_os = "windows") {
        PathBuf::from("C:\\Windows\\System32\\kernel32.dll")
    } else if cfg!(target_os = "linux") {
        PathBuf::from("libc.so.6")
    } else if cfg!(target_os = "macos") {
        PathBuf::from("/usr/lib/libSystem.B.dylib")
    } else {
        return; // Skip on unsupported platforms
    };

    if !system_lib.exists() {
        return; // Skip if library not found at expected path
    }

    let result = NativePlugin::load(&system_lib);
    assert!(result.is_err(), "Expected error, got Ok");
    match result {
        Err(PluginError::ResourceError(msg)) => {
            assert!(
                msg.contains("missing required symbol"),
                "Expected symbol missing error, got: {}",
                msg
            );
        }
        Err(e) => {
            panic!("Expected ResourceError, got: {:?}", e);
        }
        Ok(_) => unreachable!(),
    }
}

#[test]
fn test_abi_version_constants() {
    assert_eq!(MINIMUM_ABI_VERSION, 1);
    assert_eq!(CURRENT_ABI_VERSION, 1);
}

#[test]
fn test_symbol_missing_err_message() {
    let path = PathBuf::from("test_plugin.so");
    let err = symbol_missing_err(&path, "cce_plugin_metadata");
    assert!(err.to_string().contains("test_plugin.so"));
    assert!(err.to_string().contains("cce_plugin_metadata"));
}

#[test]
fn test_ffi_error_defaults_to_script() {
    let err = ffi_error(Some("boom".to_string()), None);
    assert!(matches!(err, PluginError::ScriptError(msg) if msg == "boom"));
}

#[test]
fn test_ffi_error_maps_error_type() {
    assert!(matches!(
        ffi_error(None, Some("timeout")),
        PluginError::Timeout
    ));
    assert!(matches!(
        ffi_error(Some("bad".to_string()), Some("invalid_output")),
        PluginError::InvalidOutput(_)
    ));
    assert!(matches!(
        ffi_error(Some("logic".to_string()), Some("logic")),
        PluginError::LogicError(_)
    ));
    assert!(matches!(
        ffi_error(Some("res".to_string()), Some("resource")),
        PluginError::ResourceError(_)
    ));
    assert!(matches!(
        ffi_error(None, Some("circuit_broken")),
        PluginError::CircuitBroken
    ));
    assert!(matches!(
        ffi_error(Some("nf".to_string()), Some("not_found")),
        PluginError::NotFound(_)
    ));
    assert!(matches!(
        ffi_error(Some("exec".to_string()), Some("execution_failed")),
        PluginError::ExecutionFailed(_)
    ));
}

#[test]
fn test_ffi_error_unknown_type_falls_back_to_script() {
    let err = ffi_error(Some("weird".to_string()), Some("bogus"));
    assert!(matches!(err, PluginError::ScriptError(msg) if msg == "weird"));
}

#[test]
fn test_parse_ffi_result_error_with_error_type() {
    let json = r#"{"result":"error","message":"timeout!","error_type":"timeout"}"#;
    let result: Result<Option<String>, PluginError> = parse_ffi_result(json);
    assert!(matches!(result, Err(PluginError::Timeout)));
}
