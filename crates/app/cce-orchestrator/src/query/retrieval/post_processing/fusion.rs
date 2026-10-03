//! Hybrid retrieval fusion module
//!
//! Provides weighted normalized score fusion for combining results from
//! two independent recall paths (vector + BM25). This module performs
//! min-max normalization within each path's results, then computes a
//! weighted linear combination.
//!
//! This approach preserves score magnitude information and allows
//! per-query-intent weight customization.

mod aligner;
mod merger;
mod normalizer;

pub use aligner::{
    alignment_key, compute_alignment_coverage, expand_multi_entity_results, FusionAlignmentStats,
};
pub use merger::{fuse_hybrid_results, fuse_hybrid_results_with_stats};
pub use normalizer::minmax_normalize;

/// Configuration for hybrid vector + BM25 fusion
#[derive(Debug, Clone)]
pub struct HybridFusionConfig {
    /// Weight assigned to normalized vector scores [0.0, 1.0]
    pub vector_weight: f32,
    /// Weight assigned to normalized BM25 scores [0.0, 1.0]
    pub bm25_weight: f32,
    /// Whether to include items that only appear in one path
    pub include_single_path: bool,
    /// Minimum fused score threshold
    pub min_score: f32,
    /// Whether to keep at most one result per physical chunk after fusion.
    ///
    /// Entity-level alignment can surface the same chunk once per contained
    /// entity; enabling this collapses them to the best-scoring entry per
    /// `id` (chunk id). Defaults to `true` so multi-entity chunks do not
    /// inflate the result list with identical content.
    pub dedup_by_chunk: bool,
}

impl Default for HybridFusionConfig {
    fn default() -> Self {
        Self {
            vector_weight: 0.5,
            bm25_weight: 0.5,
            include_single_path: true,
            min_score: 0.0,
            dedup_by_chunk: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::types::SearchResult;
    use cce_types::EntityId;

    fn make_vector_result(id: &str, entity_id: u64, score: f32) -> SearchResult {
        SearchResult {
            id: id.to_string(),
            entity_ids: vec![EntityId(entity_id)],
            score,
            original_score: score,
            vector_score: score,
            bm25_score: None,
            sources: vec!["vector".to_string()],
            ..Default::default()
        }
    }

    fn make_bm25_result(id: &str, entity_id: u64, score: f32) -> SearchResult {
        SearchResult {
            id: id.to_string(),
            entity_ids: vec![EntityId(entity_id)],
            score,
            original_score: score,
            vector_score: 0.0,
            bm25_score: Some(score),
            sources: vec!["bm25".to_string()],
            ..Default::default()
        }
    }

    fn make_vector_result_with_segment(id: &str, segment_id: &str, score: f32) -> SearchResult {
        SearchResult {
            id: id.to_string(),
            entity_ids: Vec::new(),
            segment_id: Some(segment_id.to_string()),
            score,
            original_score: score,
            vector_score: score,
            bm25_score: None,
            sources: vec!["vector".to_string()],
            ..Default::default()
        }
    }

    fn make_bm25_result_with_segment(id: &str, segment_id: &str, score: f32) -> SearchResult {
        SearchResult {
            id: id.to_string(),
            entity_ids: Vec::new(),
            segment_id: Some(segment_id.to_string()),
            score,
            original_score: score,
            vector_score: 0.0,
            bm25_score: Some(score),
            sources: vec!["bm25".to_string()],
            ..Default::default()
        }
    }

    #[test]
    fn test_minmax_normalize() {
        let scores = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let normalized = normalizer::minmax_normalize(&scores);
        assert!((normalized[0] - 0.0).abs() < 0.001);
        assert!((normalized[4] - 1.0).abs() < 0.001);
        assert!((normalized[2] - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_normalize_all_equal() {
        let scores = vec![0.5, 0.5, 0.5];
        let normalized = normalizer::minmax_normalize(&scores);
        assert!(normalized.iter().all(|&s| (s - 1.0).abs() < 0.001));
    }

    #[test]
    fn test_fuse_hybrid_results_both_paths() {
        let vector = vec![
            make_vector_result("emb_a", 1, 0.9),
            make_vector_result("emb_b", 2, 0.7),
            make_vector_result("emb_c", 3, 0.5),
        ];
        let bm25 = vec![
            make_bm25_result("bm25_b", 2, 0.8),
            make_bm25_result("bm25_a", 1, 0.6),
            make_bm25_result("bm25_d", 4, 0.9),
        ];

        let config = HybridFusionConfig {
            vector_weight: 0.5,
            bm25_weight: 0.5,
            include_single_path: true,
            min_score: 0.0,
            dedup_by_chunk: false,
        };

        let fused = fuse_hybrid_results(vector, bm25, &config);

        let entity_ids: Vec<i64> = fused
            .iter()
            .filter_map(|r| r.entity_ids.first())
            .map(|e| e.0 as i64)
            .collect();
        assert!(entity_ids.contains(&1));
        assert!(entity_ids.contains(&2));
        assert!(entity_ids.contains(&3));
        assert!(entity_ids.contains(&4));

        let e2_result = fused
            .iter()
            .find(|r| r.entity_ids.first() == Some(&EntityId(2)))
            .unwrap();
        assert!(e2_result.score > 0.5);

        let e3_result = fused
            .iter()
            .find(|r| r.entity_ids.first() == Some(&EntityId(3)))
            .unwrap();
        assert!(e3_result.score < 0.5);
    }

    #[test]
    fn test_fuse_hybrid_results_exclude_single_path() {
        let vector = vec![
            make_vector_result("emb_a", 1, 0.9),
            make_vector_result("emb_b", 2, 0.7),
        ];
        let bm25 = vec![
            make_bm25_result("bm25_b", 2, 0.8),
            make_bm25_result("bm25_c", 3, 0.9),
        ];

        let config = HybridFusionConfig {
            vector_weight: 0.5,
            bm25_weight: 0.5,
            include_single_path: false,
            min_score: 0.0,
            dedup_by_chunk: false,
        };

        let fused = fuse_hybrid_results(vector, bm25, &config);

        let entity_ids: Vec<i64> = fused
            .iter()
            .filter_map(|r| r.entity_ids.first())
            .map(|e| e.0 as i64)
            .collect();
        assert!(entity_ids.contains(&2));
        assert!(!entity_ids.contains(&1));
        assert!(!entity_ids.contains(&3));
    }

    #[test]
    fn test_fuse_empty_vectors() {
        let fused = fuse_hybrid_results(
            vec![],
            vec![make_bm25_result("a", 1, 0.9)],
            &HybridFusionConfig::default(),
        );
        assert_eq!(fused.len(), 1);
        assert_eq!(fused[0].entity_ids.first(), Some(&EntityId(1)));
    }

    #[test]
    fn test_fuse_single_path_applies_weight_and_min_score() {
        let config = HybridFusionConfig {
            vector_weight: 0.5,
            bm25_weight: 0.5,
            include_single_path: true,
            min_score: 0.0,
            dedup_by_chunk: false,
        };
        let fused = fuse_hybrid_results(vec![], vec![make_bm25_result("a", 1, 0.9)], &config);
        assert_eq!(fused.len(), 1);
        assert_eq!(fused[0].entity_ids.first(), Some(&EntityId(1)));
        assert!((fused[0].score - 0.5).abs() < 0.001);

        let filtered = fuse_hybrid_results(
            vec![],
            vec![make_bm25_result("a", 1, 0.9)],
            &HybridFusionConfig {
                min_score: 0.6,
                ..config
            },
        );
        assert!(filtered.is_empty());

        let fused = fuse_hybrid_results(vec![make_vector_result("emb_a", 1, 0.9)], vec![], &config);
        assert_eq!(fused.len(), 1);
        assert!((fused[0].score - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_fuse_empty_both() {
        let fused = fuse_hybrid_results(vec![], vec![], &HybridFusionConfig::default());
        assert!(fused.is_empty());
    }

    #[test]
    fn test_fuse_best_score_per_entity() {
        let vector = vec![
            make_vector_result("emb_a1", 1, 0.5),
            make_vector_result("emb_a2", 1, 0.9),
            make_vector_result("emb_b", 2, 0.7),
        ];
        let bm25 = vec![
            make_bm25_result("bm25_a", 1, 0.8),
            make_bm25_result("bm25_b", 2, 0.6),
        ];

        let config = HybridFusionConfig {
            vector_weight: 0.5,
            bm25_weight: 0.5,
            include_single_path: true,
            min_score: 0.0,
            dedup_by_chunk: false,
        };

        let fused = fuse_hybrid_results(vector, bm25, &config);

        let e1 = fused
            .iter()
            .find(|r| r.entity_ids.first() == Some(&EntityId(1)))
            .unwrap();
        assert!((e1.score - 1.0).abs() < 0.01 || e1.score > 0.8);
    }

    #[test]
    fn test_fuse_document_chunks_by_segment_id() {
        let vector = vec![
            make_vector_result_with_segment("emb_sec1", "section_1", 0.9),
            make_vector_result_with_segment("emb_sec2", "section_2", 0.7),
            make_vector_result_with_segment("emb_sec3", "section_3", 0.5),
        ];
        let bm25 = vec![
            make_bm25_result_with_segment("bm25_sec2", "section_2", 0.8),
            make_bm25_result_with_segment("bm25_sec1", "section_1", 0.6),
            make_bm25_result_with_segment("bm25_sec4", "section_4", 0.9),
        ];

        let config = HybridFusionConfig {
            vector_weight: 0.5,
            bm25_weight: 0.5,
            include_single_path: true,
            min_score: 0.0,
            dedup_by_chunk: false,
        };

        let fused = fuse_hybrid_results(vector, bm25, &config);

        let segment_ids: Vec<&str> = fused
            .iter()
            .map(|r| r.segment_id.as_deref().unwrap_or(""))
            .collect();
        assert!(segment_ids.contains(&"section_1"));
        assert!(segment_ids.contains(&"section_2"));
        assert!(segment_ids.contains(&"section_3"));
        assert!(segment_ids.contains(&"section_4"));

        let sec2 = fused
            .iter()
            .find(|r| r.segment_id.as_deref() == Some("section_2"))
            .unwrap();
        assert!(sec2.score > 0.5);

        let sec3 = fused
            .iter()
            .find(|r| r.segment_id.as_deref() == Some("section_3"))
            .unwrap();
        assert!(sec3.score < 0.5);
    }

    #[test]
    fn test_fuse_mixed_entity_and_segment() {
        let vector = vec![
            make_vector_result("emb_func1", 100, 0.95),
            make_vector_result_with_segment("emb_doc1", "doc_section_1", 0.8),
        ];
        let bm25 = vec![
            make_bm25_result("bm25_func1", 100, 0.7),
            make_bm25_result_with_segment("bm25_doc1", "doc_section_1", 0.6),
        ];

        let config = HybridFusionConfig {
            vector_weight: 0.6,
            bm25_weight: 0.4,
            include_single_path: true,
            min_score: 0.0,
            dedup_by_chunk: false,
        };

        let fused = fuse_hybrid_results(vector, bm25, &config);

        assert_eq!(fused.len(), 2);

        let func = fused
            .iter()
            .find(|r| r.entity_ids.first() == Some(&EntityId(100)))
            .unwrap();
        assert!(func.score > 0.5);

        let doc = fused
            .iter()
            .find(|r| r.segment_id.as_deref() == Some("doc_section_1"))
            .unwrap();
        assert!(doc.score >= 0.0);

        assert!(func.score > doc.score);
    }

    #[test]
    fn test_fuse_same_segment_different_entity_no_match() {
        let vector = vec![make_vector_result("emb_a", 100, 0.9)];
        let bm25 = vec![make_bm25_result("bm25_b", 200, 0.8)];

        let config = HybridFusionConfig {
            vector_weight: 0.5,
            bm25_weight: 0.5,
            include_single_path: true,
            min_score: 0.0,
            dedup_by_chunk: false,
        };

        let fused = fuse_hybrid_results(vector, bm25, &config);

        assert_eq!(fused.len(), 2);

        for r in &fused {
            assert!(
                r.score <= 0.5,
                "single-path results should have reduced score <= 0.5"
            );
        }
    }

    #[test]
    fn test_fuse_entity_id_priority_over_segment_id() {
        let mut vector = make_vector_result("emb_a", 100, 0.9);
        vector.segment_id = Some("shared_segment".to_string());

        let mut bm25 = make_bm25_result("bm25_a", 200, 0.8);
        bm25.segment_id = Some("shared_segment".to_string());

        let config = HybridFusionConfig {
            vector_weight: 0.5,
            bm25_weight: 0.5,
            include_single_path: true,
            min_score: 0.0,
            dedup_by_chunk: false,
        };

        let fused = fuse_hybrid_results(vec![vector], vec![bm25], &config);

        assert_eq!(fused.len(), 2);

        for r in &fused {
            assert!(
                r.score <= 0.5,
                "single-path results should have reduced score <= 0.5"
            );
        }
    }

    #[test]
    fn test_fuse_multiple_chunks_same_segment_picks_best() {
        let vector = vec![
            make_vector_result_with_segment("emb_s1_a", "sec1", 0.5),
            make_vector_result_with_segment("emb_s1_b", "sec1", 0.9),
        ];
        let bm25 = vec![
            make_bm25_result_with_segment("bm25_s1_a", "sec1", 0.4),
            make_bm25_result_with_segment("bm25_s1_b", "sec1", 0.8),
        ];

        let config = HybridFusionConfig {
            vector_weight: 0.5,
            bm25_weight: 0.5,
            include_single_path: true,
            min_score: 0.0,
            dedup_by_chunk: false,
        };

        let fused = fuse_hybrid_results(vector, bm25, &config);

        assert_eq!(fused.len(), 1);
        let sec1 = fused
            .iter()
            .find(|r| r.segment_id.as_deref() == Some("sec1"))
            .unwrap();
        assert!(sec1.score > 0.8);
    }

    #[test]
    fn test_fuse_exclude_single_path_segment_chunks() {
        let vector = vec![
            make_vector_result_with_segment("emb_s1", "sec1", 0.9),
            make_vector_result_with_segment("emb_s2", "sec2", 0.7),
        ];
        let bm25 = vec![make_bm25_result_with_segment("bm25_s2", "sec2", 0.8)];

        let config = HybridFusionConfig {
            vector_weight: 0.5,
            bm25_weight: 0.5,
            include_single_path: false,
            min_score: 0.0,
            dedup_by_chunk: false,
        };

        let fused = fuse_hybrid_results(vector, bm25, &config);

        let segment_ids: Vec<&str> = fused
            .iter()
            .map(|r| r.segment_id.as_deref().unwrap_or(""))
            .collect();
        assert!(segment_ids.contains(&"sec2"));
        assert!(!segment_ids.contains(&"sec1"));
    }

    fn make_unkeyed_result(id: &str, score: f32) -> SearchResult {
        SearchResult {
            id: id.to_string(),
            entity_ids: Vec::new(),
            segment_id: None,
            score,
            original_score: score,
            vector_score: score,
            bm25_score: None,
            sources: vec!["vector".to_string()],
            ..Default::default()
        }
    }

    #[test]
    fn test_fuse_unkeyed_results_do_not_collapse() {
        let vector = vec![
            make_unkeyed_result("emb_a", 0.9),
            make_unkeyed_result("emb_b", 0.8),
            make_unkeyed_result("emb_c", 0.7),
        ];
        let bm25 = vec![
            make_unkeyed_result("bm25_b", 0.9),
            make_unkeyed_result("bm25_c", 0.85),
        ];

        let config = HybridFusionConfig {
            vector_weight: 0.5,
            bm25_weight: 0.5,
            include_single_path: true,
            min_score: 0.0,
            dedup_by_chunk: false,
        };

        let fused = fuse_hybrid_results(vector, bm25, &config);

        assert_eq!(fused.len(), 5, "unkeyed chunks must not collapse");
        let ids: Vec<&str> = fused.iter().map(|r| r.id.as_str()).collect();
        for id in ["emb_a", "emb_b", "emb_c", "bm25_b", "bm25_c"] {
            assert_eq!(
                ids.iter().filter(|i| **i == id).count(),
                1,
                "chunk {id} should appear exactly once"
            );
        }
    }

    #[test]
    fn test_fuse_unkeyed_path_ids_do_not_fuse_across_paths() {
        let vector = vec![
            make_unkeyed_result("g_emb_0", 0.9),
            make_unkeyed_result("h_emb_1", 0.8),
        ];
        let bm25 = vec![
            make_unkeyed_result("g_bm25_0", 0.9),
            make_unkeyed_result("h_bm25_2", 0.85),
        ];

        let config = HybridFusionConfig {
            vector_weight: 0.5,
            bm25_weight: 0.5,
            include_single_path: true,
            min_score: 0.0,
            dedup_by_chunk: false,
        };

        let fused = fuse_hybrid_results(vector, bm25, &config);

        assert_eq!(
            fused.len(),
            4,
            "unkeyed path-id chunks must not fuse across paths"
        );
        let ids: Vec<&str> = fused.iter().map(|r| r.id.as_str()).collect();
        for id in ["g_emb_0", "g_bm25_0", "h_emb_1", "h_bm25_2"] {
            assert_eq!(
                ids.iter().filter(|i| **i == id).count(),
                1,
                "chunk {id} should appear exactly once"
            );
        }
    }

    #[test]
    fn test_fuse_document_chunks_align_by_segment_id() {
        let vector = vec![
            make_vector_result_with_segment("emb_sec1", "doc_group_1", 0.9),
            make_vector_result_with_segment("emb_sec2", "doc_group_2", 0.7),
        ];
        let bm25 = vec![
            make_bm25_result_with_segment("bm25_sec1", "doc_group_1", 0.8),
            make_bm25_result_with_segment("bm25_sec2", "doc_group_2", 0.6),
        ];

        let config = HybridFusionConfig {
            vector_weight: 0.5,
            bm25_weight: 0.5,
            include_single_path: true,
            min_score: 0.0,
            dedup_by_chunk: false,
        };

        let fused = fuse_hybrid_results(vector, bm25, &config);

        assert_eq!(fused.len(), 2);
        for r in &fused {
            assert!(
                r.bm25_score.is_some(),
                "matched document chunk should carry the BM25 score"
            );
        }
        let sec1 = fused
            .iter()
            .find(|r| r.segment_id.as_deref() == Some("doc_group_1"))
            .unwrap();
        assert!((sec1.score - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_fuse_deterministic_tie_break() {
        let vector = vec![
            make_vector_result("emb_a", 1, 0.9),
            make_vector_result("emb_b", 2, 0.7),
        ];
        let bm25 = vec![
            make_bm25_result("bm25_a", 1, 0.8),
            make_bm25_result("bm25_b", 2, 0.6),
        ];
        let config = HybridFusionConfig {
            vector_weight: 0.5,
            bm25_weight: 0.5,
            include_single_path: true,
            min_score: 0.0,
            dedup_by_chunk: false,
        };

        let fused = fuse_hybrid_results(vector, bm25, &config);
        let ids: Vec<String> = fused.iter().map(|r| r.id.clone()).collect();
        for _ in 0..20 {
            let again = fuse_hybrid_results(
                vec![
                    make_vector_result("emb_a", 1, 0.9),
                    make_vector_result("emb_b", 2, 0.7),
                ],
                vec![
                    make_bm25_result("bm25_a", 1, 0.8),
                    make_bm25_result("bm25_b", 2, 0.6),
                ],
                &config,
            );
            let ids_again: Vec<String> = again.iter().map(|r| r.id.clone()).collect();
            assert_eq!(ids, ids_again, "tie order must be deterministic");
        }
    }

    #[test]
    fn test_fuse_dedup_by_chunk_keeps_best_per_chunk() {
        let mut v1 = make_vector_result("chunk_x", 100, 0.9);
        v1.segment_id = Some("seg".to_string());
        let mut v2 = make_vector_result("chunk_x", 200, 0.95);
        v2.segment_id = Some("seg".to_string());
        let mut b1 = make_bm25_result("chunk_x", 100, 0.8);
        b1.segment_id = Some("seg".to_string());

        let config = HybridFusionConfig {
            vector_weight: 0.7,
            bm25_weight: 0.3,
            include_single_path: true,
            min_score: 0.0,
            dedup_by_chunk: true,
        };
        let fused = fuse_hybrid_results(vec![v1, v2], vec![b1], &config);

        assert_eq!(fused.len(), 1, "dedup_by_chunk must collapse to one entry");
        assert_eq!(fused[0].entity_ids.first(), Some(&EntityId(200)));
        assert!((fused[0].score - 0.7).abs() < 0.001);

        let config_no_dedup = HybridFusionConfig {
            dedup_by_chunk: false,
            ..config
        };
        let fused_no_dedup = fuse_hybrid_results(
            vec![
                {
                    let mut v = make_vector_result("chunk_x", 100, 0.9);
                    v.segment_id = Some("seg".to_string());
                    v
                },
                {
                    let mut v = make_vector_result("chunk_x", 200, 0.95);
                    v.segment_id = Some("seg".to_string());
                    v
                },
            ],
            vec![{
                let mut b = make_bm25_result("chunk_x", 100, 0.8);
                b.segment_id = Some("seg".to_string());
                b
            }],
            &config_no_dedup,
        );
        assert_eq!(fused_no_dedup.len(), 2);
    }

    #[test]
    fn test_compute_alignment_coverage_counts_matched_keys() {
        let vector = vec![
            make_vector_result("emb_a", 1, 0.9),
            make_vector_result_with_segment("emb_d1", "doc_1", 0.8),
            make_unkeyed_result("emb_u", 0.7),
        ];
        let bm25 = vec![
            make_bm25_result("bm25_a", 1, 0.8),
            make_bm25_result_with_segment("bm25_d1", "doc_1", 0.6),
            make_bm25_result_with_segment("bm25_d2", "doc_2", 0.7),
        ];

        let stats = compute_alignment_coverage(&vector, &bm25);
        assert_eq!(stats.vector_keys, 3);
        assert_eq!(stats.bm25_keys, 3);
        assert_eq!(stats.matched_keys, 2);
    }
}
