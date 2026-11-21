use super::*;

#[test]
fn test_chunk_override_above_plugin_wins_over_builtin() {
    fn plugin_chunks(
        conversions: Vec<GroupConversions>,
        _file_path: &str,
    ) -> Result<Option<Vec<ChunkedResult>>, PluginError> {
        let group = &conversions[0].group;
        Ok(Some(vec![plugin_chunk_from_text(
            group,
            "PLUGIN-CHUNK-MARKER",
            ChunkPath::Bm25,
        )]))
    }
    let chunker = chunker_with(vec![(
        ChunkMockPlugin::with_id("chunker", 100).chunk_fn(plugin_chunks),
        None,
    )]);

    let chunks = chunker
        .chunk_groups(&chunk_conversions(), "app.py")
        .expect("chunk")
        .chunks;
    assert_eq!(
        chunks.len(),
        1,
        "plugin chunk list replaces built-in output"
    );
    assert_eq!(chunks[0].text, "PLUGIN-CHUNK-MARKER");
}

#[test]
fn test_chunk_override_first_non_none_plugin_wins() {
    fn decline(
        _conversions: Vec<GroupConversions>,
        _file_path: &str,
    ) -> Result<Option<Vec<ChunkedResult>>, PluginError> {
        Ok(None)
    }
    fn plugin_chunks(
        conversions: Vec<GroupConversions>,
        _file_path: &str,
    ) -> Result<Option<Vec<ChunkedResult>>, PluginError> {
        let group = &conversions[0].group;
        Ok(Some(vec![plugin_chunk_from_text(
            group,
            "SECOND-PLUGIN",
            ChunkPath::Bm25,
        )]))
    }
    let chunker = chunker_with(vec![
        (
            ChunkMockPlugin::with_id("decline", 100).chunk_fn(decline),
            None,
        ),
        (
            ChunkMockPlugin::with_id("second", 10).chunk_fn(plugin_chunks),
            None,
        ),
    ]);

    let chunks = chunker
        .chunk_groups(&chunk_conversions(), "app.py")
        .expect("chunk")
        .chunks;
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].text, "SECOND-PLUGIN");
}

#[test]
fn test_chunk_decline_falls_back_to_builtin() {
    fn decline(
        _conversions: Vec<GroupConversions>,
        _file_path: &str,
    ) -> Result<Option<Vec<ChunkedResult>>, PluginError> {
        Ok(None)
    }
    let chunker = chunker_with(vec![(
        ChunkMockPlugin::with_id("decline", 100).chunk_fn(decline),
        None,
    )]);

    let chunks = chunker
        .chunk_groups(&chunk_conversions(), "app.py")
        .expect("chunk")
        .chunks;
    assert!(!chunks.is_empty(), "built-in chunker must produce chunks");
    assert!(
        chunks.iter().all(|c| !c.text.contains("PLUGIN")),
        "declined plugin must not influence chunk content"
    );
}

#[test]
fn test_chunk_error_falls_back_to_builtin() {
    fn fail(
        _conversions: Vec<GroupConversions>,
        _file_path: &str,
    ) -> Result<Option<Vec<ChunkedResult>>, PluginError> {
        Err(PluginError::ExecutionFailed("broken".to_string()))
    }
    let chunker = chunker_with(vec![(
        ChunkMockPlugin::with_id("broken", 100).chunk_fn(fail),
        None,
    )]);

    let chunks = chunker
        .chunk_groups(&chunk_conversions(), "app.py")
        .expect("chunk")
        .chunks;
    assert!(
        !chunks.is_empty(),
        "built-in chunker must take over on error"
    );
}

#[test]
fn test_chunk_file_pattern_mismatch_uses_builtin() {
    fn plugin_chunks(
        conversions: Vec<GroupConversions>,
        _file_path: &str,
    ) -> Result<Option<Vec<ChunkedResult>>, PluginError> {
        let group = &conversions[0].group;
        Ok(Some(vec![plugin_chunk_from_text(
            group,
            "PLUGIN-CHUNK-MARKER",
            ChunkPath::Bm25,
        )]))
    }
    let chunker = chunker_with(vec![(
        ChunkMockPlugin::with_id("py-only", 100).chunk_fn(plugin_chunks),
        Some(vec!["*.py"]),
    )]);

    // `.rs` file does not match the plugin's pattern → built-in chunker.
    let chunks = chunker
        .chunk_groups(&chunk_conversions(), "lib.rs")
        .expect("chunk")
        .chunks;
    assert!(!chunks.is_empty());
    assert!(
        chunks
            .iter()
            .all(|c| !c.text.contains("PLUGIN-CHUNK-MARKER"))
    );
}

#[test]
fn test_chunk_below_plugin_used_only_when_builtin_empty() {
    fn below_chunks(
        conversions: Vec<GroupConversions>,
        _file_path: &str,
    ) -> Result<Option<Vec<ChunkedResult>>, PluginError> {
        let group = &conversions[0].group;
        Ok(Some(vec![plugin_chunk_from_text(
            group,
            "BELOW-FALLBACK",
            ChunkPath::Bm25,
        )]))
    }
    let chunker = chunker_with(vec![(
        ChunkMockPlugin::with_id("below", -1).chunk_fn(below_chunks),
        None,
    )]);

    // Content present → built-in produces chunks → below tier stays silent.
    let chunks = chunker
        .chunk_groups(&chunk_conversions(), "app.py")
        .expect("chunk")
        .chunks;
    assert!(!chunks.is_empty());
    assert!(
        chunks.iter().all(|c| !c.text.contains("BELOW-FALLBACK")),
        "below-tier plugin must stay silent when the built-in produced chunks"
    );

    // Empty conversions → built-in produces nothing → below tier runs.
    let empty: Vec<GroupConversions> = vec![GroupConversions {
        group: create_merge_test_group("g1", "", 0, 0),
        header_conversion: Some(create_merge_test_conversion("")),
        member_conversions: vec![],
    }];
    let chunks = chunker
        .chunk_groups(&empty, "app.py")
        .expect("chunk")
        .chunks;
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].text, "BELOW-FALLBACK");
}
