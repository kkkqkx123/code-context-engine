use super::*;
use crate::modules::ServiceType;
use crate::project::ProjectAppConfig;

#[test]
fn test_default_config() {
    let config = AppConfig::default();
    assert_eq!(config.server.host, "0.0.0.0");
    assert_eq!(config.server.port, 9000);
    assert_eq!(config.database.sqlite.path, "metadata.db");
    assert_eq!(config.logger.level, LogLevel::Info);
    assert!(!config.rerank.enabled);
    assert_eq!(config.rerank.model, "gpt-4o-mini");
    assert_eq!(config.rerank.max_candidates, 50);
}

#[test]
fn test_config_serialization() {
    let config = AppConfig::default();
    let toml_str = toml::to_string_pretty(&config).expect("Failed to serialize");
    let deserialized: AppConfig = toml::from_str(&toml_str).expect("Failed to deserialize");
    assert_eq!(config.server.host, deserialized.server.host);
    assert_eq!(config.rerank.enabled, deserialized.rerank.enabled);
    assert_eq!(config.rerank.model, deserialized.rerank.model);
    assert_eq!(
        config.rerank.max_candidates,
        deserialized.rerank.max_candidates
    );
}

#[test]
fn test_validate_allows_remote_provider_without_api_key() {
    let mut config = AppConfig::default();
    config.embedder.default_model = "dummy-model".to_string();
    config.llm.providers.insert(
        "openai".to_string(),
        ProviderConfig {
            id: "openai".to_string(),
            name: "OpenAI".to_string(),
            enabled: true,
            provider_type: cce_types::llm::ProviderType::Remote,
            api_keys: Vec::new(),
            base_url: "https://api.openai.com/v1".to_string(),
            endpoints: std::collections::HashMap::new(),
            timeout_secs: 30,
            max_retries: 3,
            retry_delay_ms: 1000,
            retry_jitter: 0.2,
            rate_limit_max_retries: 5,
            rate_limit_max_delay_ms: 60000,
            rate_limit: 60,
            circuit_breaker: crate::modules::CircuitBreakerConfig::default(),
            proxy_url: None,
            extra_headers: std::collections::HashMap::new(),
            api_key_file: None,
        },
    );

    assert!(config.validate_structured().is_ok());
}

#[test]
fn test_validate_rejects_provider_with_empty_base_url() {
    let mut config = AppConfig::default();
    config.embedder.default_model = "dummy-model".to_string();
    config.llm.providers.insert(
        "openai".to_string(),
        ProviderConfig {
            id: "openai".to_string(),
            name: "OpenAI".to_string(),
            enabled: true,
            provider_type: cce_types::llm::ProviderType::Remote,
            api_keys: vec!["key".to_string()],
            base_url: String::new(),
            endpoints: std::collections::HashMap::new(),
            timeout_secs: 30,
            max_retries: 3,
            retry_delay_ms: 1000,
            retry_jitter: 0.2,
            rate_limit_max_retries: 5,
            rate_limit_max_delay_ms: 60000,
            rate_limit: 60,
            circuit_breaker: crate::modules::CircuitBreakerConfig::default(),
            proxy_url: None,
            extra_headers: std::collections::HashMap::new(),
            api_key_file: None,
        },
    );

    let err = config
        .validate_structured()
        .expect_err("should reject empty base_url");
    assert!(err.to_string().contains("empty base_url"));
}

#[test]
fn test_validate_dependencies_no_warnings() {
    let mut config = AppConfig::default();
    config.database.bm25.enabled = true;

    let warnings = config.validate_dependencies();
    assert!(warnings.is_empty());
}

#[test]
fn test_validate_dependencies_summary_warning() {
    let mut config = AppConfig::default();
    config.database.bm25.enabled = true;
    config.export.include_summary = true;
    config.orchestrator.indexer.store_summaries = false;

    let warnings = config.validate_dependencies();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].field, "export.include_summary");
}

#[test]
fn test_validate_dependencies_relation_warnings() {
    let mut config = AppConfig::default();
    config.database.bm25.enabled = true;
    config.export.enable_relation_enhancement = true;
    config.relation.index.enabled = false;
    config.orchestrator.indexer.build_relations = false;

    let warnings = config.validate_dependencies();
    assert_eq!(warnings.len(), 2);
}

#[test]
fn test_resolve_dependencies_auto_enable() {
    let mut config = AppConfig::default();
    config.database.bm25.enabled = true;
    config.export.include_summary = true;
    config.export.enable_relation_enhancement = true;
    config.orchestrator.indexer.store_summaries = false;
    config.orchestrator.indexer.build_relations = false;
    config.relation.index.enabled = false;

    let infos = config.resolve_dependencies();

    assert_eq!(infos.len(), 3);
    assert!(config.orchestrator.indexer.store_summaries);
    assert!(config.orchestrator.indexer.build_relations);
    assert!(config.relation.index.enabled);
}

#[test]
fn test_validate_and_resolve_dependencies() {
    let mut config = AppConfig::default();
    config.database.bm25.enabled = true;
    config.export.include_summary = true;
    config.export.enable_relation_enhancement = true;
    config.orchestrator.indexer.store_summaries = false;
    config.orchestrator.indexer.build_relations = false;
    config.relation.index.enabled = false;

    let messages = config.validate_and_resolve_dependencies();

    assert_eq!(messages.len(), 6);
    assert!(config.orchestrator.indexer.store_summaries);
    assert!(config.orchestrator.indexer.build_relations);
    assert!(config.relation.index.enabled);
}

#[test]
fn test_merge_with_project_empty() {
    let global = AppConfig::default();
    let project = ProjectAppConfig::default();
    let merged = global.merge_with_project(&project);

    assert_eq!(merged.server.host, global.server.host);
    assert_eq!(
        merged.scanner.follow_symlinks,
        global.scanner.follow_symlinks
    );
}

#[test]
fn test_merge_with_project_checkpoint_ttl() {
    let global = AppConfig::default();
    assert_eq!(global.orchestrator.checkpoint_ttl_seconds, 86400);

    let project = ProjectAppConfig {
        orchestrator: Some(crate::ProjectOrchestratorConfig {
            checkpoint_ttl_seconds: Some(7200),
            ..Default::default()
        }),
        ..Default::default()
    };

    let merged = global.merge_with_project(&project);
    assert_eq!(merged.orchestrator.checkpoint_ttl_seconds, 7200);

    let untouched = global.merge_with_project(&ProjectAppConfig::default());
    assert_eq!(untouched.orchestrator.checkpoint_ttl_seconds, 86400);
}

#[test]
fn test_merge_with_project_scanner() {
    let global = AppConfig::default();
    let project = ProjectAppConfig {
        scanner: Some(ScannerConfig {
            respect_gitignore: false,
            ..Default::default()
        }),
        ..Default::default()
    };

    let merged = global.merge_with_project(&project);

    assert!(!merged.scanner.respect_gitignore);
    assert_eq!(merged.server.host, global.server.host);
    assert_eq!(merged.database.sqlite.path, global.database.sqlite.path);
}

#[test]
fn test_merge_with_project_preserves_sensitive() {
    let mut global = AppConfig::default();
    use std::collections::HashMap;
    let mut providers = HashMap::new();
    providers.insert(
        "openai".to_string(),
        crate::modules::ProviderConfig {
            id: "openai".to_string(),
            name: "OpenAI".to_string(),
            base_url: "https://api.example.com".to_string(),
            api_keys: vec!["secret-key".to_string()],
            ..Default::default()
        },
    );
    global.llm.providers = providers;

    let project = ProjectAppConfig {
        scanner: Some(ScannerConfig {
            ..Default::default()
        }),
        ..Default::default()
    };

    let merged = global.merge_with_project(&project);

    let provider = merged.llm.providers.get("openai").unwrap();
    assert_eq!(provider.base_url, "https://api.example.com");
    assert_eq!(provider.api_keys, vec!["secret-key".to_string()]);
}

#[test]
fn test_merge_with_project_embedder_model() {
    use crate::ProjectEmbedderConfig;
    use std::collections::HashMap;

    let mut global = AppConfig::default();
    global.embedder.default_model = "all-MiniLM-L6-v2".to_string();

    let mut providers = HashMap::new();
    providers.insert(
        "openai".to_string(),
        crate::modules::ProviderConfig {
            id: "openai".to_string(),
            name: "OpenAI".to_string(),
            base_url: "https://api.example.com".to_string(),
            api_keys: vec![],
            ..Default::default()
        },
    );
    global.llm.providers = providers;

    let mut models = HashMap::new();
    models.insert(
        "all-MiniLM-L6-v2".to_string(),
        crate::modules::EmbeddingModelConfig {
            provider_id: "openai".to_string(),
            model: "all-MiniLM-L6-v2".to_string(),
            vector_dimension: 384,
            ..Default::default()
        },
    );
    global.llm.embedding_models = models;

    let project = ProjectAppConfig {
        embedder: Some(ProjectEmbedderConfig {
            model: Some("bge-m3".to_string()),
            ..Default::default()
        }),
        ..Default::default()
    };

    let merged = global.merge_with_project(&project);

    assert_eq!(merged.embedder.default_model, "bge-m3");
    let provider = merged.llm.providers.get("openai").unwrap();
    assert_eq!(provider.base_url, "https://api.example.com");
}

#[test]
fn test_merge_with_project_llm_models() {
    use crate::ProjectLlmConfig;

    let mut global = AppConfig::default();
    global.llm.defaults.chat = Some("gpt-3.5-turbo".to_string());
    global.llm.defaults.rerank = Some("cross-encoder-base".to_string());
    global.rerank.enabled = false;
    global.rerank.max_candidates = 50;

    let project = ProjectAppConfig {
        llm: Some(ProjectLlmConfig {
            chat_model: Some("gpt-4o".to_string()),
            rerank_model: Some("gpt-4o-mini-rerank".to_string()),
            enable_rerank: Some(true),
            rerank_max_candidates: Some(30),
        }),
        ..Default::default()
    };

    let merged = global.merge_with_project(&project);

    assert_eq!(merged.llm.defaults.chat, Some("gpt-4o".to_string()));
    assert_eq!(
        merged.llm.defaults.rerank,
        Some("gpt-4o-mini-rerank".to_string())
    );
    assert!(merged.rerank.enabled);
    assert_eq!(merged.rerank.max_candidates, 30);
}

#[test]
fn test_merge_with_project_storage_qdrant_preset() {
    use crate::modules::storage::CollectionPreset;
    use crate::project::{ProjectQdrantConfig, ProjectStorageConfig};

    let mut global = AppConfig::default();
    global.database.qdrant.preset = CollectionPreset::Medium;

    let project = ProjectAppConfig {
        storage: Some(ProjectStorageConfig {
            qdrant: Some(ProjectQdrantConfig {
                preset: Some(CollectionPreset::Large),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    };

    let merged = global.merge_with_project(&project);

    assert_eq!(merged.database.qdrant.preset, CollectionPreset::Large);
}

#[test]
fn test_merge_with_project_storage_bm25() {
    use crate::project::{ProjectBm25Config, ProjectStorageConfig};

    let mut global = AppConfig::default();
    global.database.bm25.enabled = false;

    let project = ProjectAppConfig {
        storage: Some(ProjectStorageConfig {
            bm25: Some(ProjectBm25Config {
                enabled: Some(true),
                index_path: Some("./.cce/bm25".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    };

    let merged = global.merge_with_project(&project);

    assert!(merged.database.bm25.enabled);
    assert_eq!(
        merged.database.bm25.index_path,
        Some("./.cce/bm25".to_string())
    );
}

#[test]
fn test_merge_with_project_storage_full_config() {
    use crate::modules::storage::{Bm25AlgorithmConfig, CollectionPreset};
    use crate::project::*;

    let mut global = AppConfig::default();
    global.database.qdrant.preset = CollectionPreset::Medium;
    global.database.bm25.enabled = false;

    let project = ProjectAppConfig {
        storage: Some(ProjectStorageConfig {
            qdrant: Some(ProjectQdrantConfig {
                preset: Some(CollectionPreset::Large),
                hnsw: Some(HnswConfigOverride {
                    m: Some(64),
                    ef_construct: Some(512),
                    on_disk: Some(true),
                    payload_m: Some(64),
                    inline_storage: Some(true),
                }),
                quantization: Some(QuantizationConfig::Scalar(
                    crate::modules::storage::ScalarQuantizationConfig {
                        quant_type: "int8".to_string(),
                        quantile: 0.99,
                        always_ram: false,
                    },
                )),
                ..Default::default()
            }),
            bm25: Some(ProjectBm25Config {
                enabled: Some(true),
                algorithm: Some(Bm25AlgorithmConfig { k1: 2.0, b: 0.3 }),
                ..Default::default()
            }),
            index_manager: None,
        }),
        ..Default::default()
    };

    let merged = global.merge_with_project(&project);

    assert_eq!(merged.database.qdrant.preset, CollectionPreset::Large);

    let hnsw = merged.database.qdrant.hnsw.expect("hnsw resolved");
    assert_eq!(hnsw.m, 64);
    assert_eq!(hnsw.ef_construct, 512);
    assert_eq!(hnsw.inline_storage, Some(true));
    let quant = merged
        .database
        .qdrant
        .quantization
        .expect("quantization resolved");
    assert!(matches!(
        quant,
        crate::modules::storage::QuantizationConfig::Scalar(_)
    ));

    assert!(merged.database.bm25.enabled);
    assert_eq!(merged.database.bm25.algorithm.k1, 2.0);
}

#[test]
fn test_merge_qdrant_partial_patch_preserves_base() {
    use crate::modules::storage::{CollectionPreset, HnswConfig, QuantizationConfig};
    use crate::project::{HnswConfigOverride, ProjectQdrantConfig, ProjectStorageConfig};

    let mut global = AppConfig::default();
    global.database.qdrant.preset = CollectionPreset::Medium;
    global.database.qdrant.hnsw = Some(HnswConfig {
        m: 32,
        ef_construct: 256,
        on_disk: true,
        payload_m: Some(32),
        inline_storage: Some(true),
    });

    let project = ProjectAppConfig {
        storage: Some(ProjectStorageConfig {
            qdrant: Some(ProjectQdrantConfig {
                hnsw: Some(HnswConfigOverride {
                    m: Some(48),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    };

    let merged = global.merge_with_project(&project);

    let hnsw = merged.database.qdrant.hnsw.expect("hnsw resolved");
    assert_eq!(hnsw.m, 48, "patch field overridden");
    assert_eq!(hnsw.ef_construct, 256, "unpatched field preserved");
    assert_eq!(hnsw.inline_storage, Some(true), "unpatched field preserved");

    global.database.qdrant.quantization = Some(QuantizationConfig::Scalar(Default::default()));
    let merged = global.merge_with_project(&project);
    assert!(matches!(
        merged.database.qdrant.quantization,
        Some(QuantizationConfig::Scalar(_))
    ));
}

fn config_with_provider_and_models() -> AppConfig {
    let mut config = AppConfig::default();
    config.llm.providers.insert(
        "mock-provider".to_string(),
        ProviderConfig {
            id: "mock-provider".to_string(),
            name: "Mock".to_string(),
            base_url: "https://api.mock.example.com/v1".to_string(),
            api_keys: vec!["test-key".to_string()],
            endpoints: std::collections::HashMap::from([
                (ServiceType::Chat, "custom/chat".to_string()),
                (ServiceType::Rerank, "custom/rerank".to_string()),
            ]),
            ..ProviderConfig::default()
        },
    );

    config.llm.embedding_models.insert(
        "emb-model".to_string(),
        EmbeddingModelConfig {
            provider_id: "mock-provider".to_string(),
            model: "bge-m3".to_string(),
            vector_dimension: 1024,
            ..EmbeddingModelConfig::default()
        },
    );

    config.llm.chat_models.insert(
        "chat-model".to_string(),
        ChatModelConfig {
            provider_id: "mock-provider".to_string(),
            model: "Qwen/Qwen3.5-4B".to_string(),
            temperature: 0.2,
            max_tokens: 4096,
            max_input_tokens: 8192,
            extra_params: std::collections::HashMap::from([(
                "extra".to_string(),
                serde_json::json!("value"),
            )]),
            ..ChatModelConfig::default()
        },
    );

    config.llm.rerank_models.insert(
        "rerank-model".to_string(),
        RerankModelConfig {
            provider_id: "mock-provider".to_string(),
            model: "BAAI/bge-reranker-v2-m3".to_string(),
            mode: crate::modules::RerankMode::CrossEncoder,
        },
    );
    config
}

#[test]
fn test_resolve_llm_connection_honors_endpoint_overrides() {
    let config = config_with_provider_and_models();

    let chat = config
        .resolve_llm_connection("chat-model", ServiceType::Chat)
        .expect("chat resolution must succeed");
    assert_eq!(chat.provider_id, "mock-provider");
    assert_eq!(chat.base_url, "https://api.mock.example.com/v1");
    assert_eq!(chat.endpoint_path, "custom/chat");
    assert_eq!(chat.api_keys, vec!["test-key".to_string()]);
    assert_eq!(
        chat.extra_params.get("extra"),
        Some(&serde_json::json!("value")),
        "chat model extra_params must flow into the connection"
    );

    let rerank = config
        .resolve_llm_connection("rerank-model", ServiceType::Rerank)
        .expect("rerank resolution must succeed");
    assert_eq!(rerank.endpoint_path, "custom/rerank");
    assert!(rerank.extra_params.is_empty());

    let embedding = config
        .resolve_llm_connection("emb-model", ServiceType::Embedding)
        .expect("embedding resolution must succeed");
    assert_eq!(
        embedding.endpoint_path, "embeddings",
        "unoverridden services keep their default path"
    );
}

#[test]
fn test_resolve_llm_connection_missing_model() {
    let config = config_with_provider_and_models();
    assert!(
        config
            .resolve_llm_connection("nope", ServiceType::Chat)
            .is_err()
    );
    assert!(
        config
            .resolve_llm_connection("chat-model", ServiceType::Completion)
            .is_err(),
        "Completion has no registry"
    );
}

#[test]
fn test_resolve_chat_config_merges_model_and_provider() {
    let config = config_with_provider_and_models();
    let resolved = config
        .resolve_chat_config("chat-model")
        .expect("chat resolution must succeed");

    assert_eq!(resolved.model, "Qwen/Qwen3.5-4B");
    assert_eq!(resolved.temperature, 0.2);
    assert_eq!(resolved.max_tokens, 4096);
    assert_eq!(resolved.max_input_tokens, 8192);
    assert_eq!(resolved.endpoint_path, "custom/chat");
    assert_eq!(resolved.base_url, "https://api.mock.example.com/v1");
    assert_eq!(
        resolved.extra_params.get("extra"),
        Some(&serde_json::json!("value"))
    );
}

#[test]
fn test_resolve_embedding_config_includes_endpoint_path() {
    let config = config_with_provider_and_models();
    let resolved = config
        .resolve_embedding_config("emb-model")
        .expect("embedding resolution must succeed");
    assert_eq!(resolved.endpoint_path, "embeddings");
    assert_eq!(resolved.model, "bge-m3");
    assert_eq!(resolved.vector_dimension, 1024);
}
