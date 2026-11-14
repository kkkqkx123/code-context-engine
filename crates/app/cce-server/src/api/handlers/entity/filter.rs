//! Translation of HTTP relation filters into relation query options.
//!
//! Both the callee/caller handlers and the call-chain handlers accept the same
//! filter surface, so the comma-separated wire form is decoded here once.

use cce_api::models::RelationFilterParams;
use cce_orchestrator::query::RelationQueryOptions;

/// Apply the HTTP filter parameters onto query options.
pub(super) fn apply_filters(
    filter: RelationFilterParams,
    options: RelationQueryOptions,
) -> RelationQueryOptions {
    let mut options = options.with_exclude_tests(filter.exclude_tests.unwrap_or(false));
    if let Some(prefix) = filter.directory_prefix {
        options = options.with_directory_prefix(prefix);
    }
    if let Some(files) = filter.excluded_files {
        options = options.with_excluded_files(parse_list(&files));
    }
    if let Some(domains) = filter.domains {
        options = options.with_relation_domains(
            parse_list(&domains)
                .into_iter()
                .map(|domain| domain.to_lowercase())
                .collect(),
        );
    }
    options.with_include_external(filter.include_external)
}

/// Split a comma-separated parameter value into trimmed non-empty items.
fn parse_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter() -> RelationFilterParams {
        RelationFilterParams {
            exclude_tests: Some(true),
            directory_prefix: Some("src/flask".to_string()),
            excluded_files: Some("src/flask/app.py, src/flask/cli.py".to_string()),
            domains: Some("Call, Dependency".to_string()),
            include_external: false,
        }
    }

    #[test]
    fn comma_separated_filters_reach_query_options() {
        let opts = apply_filters(filter(), RelationQueryOptions::new());
        assert!(
            opts.exclude_content_types
                .contains(&cce_orchestrator::query::ExcludableContentType::Test)
        );
        assert_eq!(opts.directory_prefix.as_deref(), Some("src/flask"));
        assert_eq!(
            opts.excluded_files,
            vec![
                "src/flask/app.py".to_string(),
                "src/flask/cli.py".to_string()
            ]
        );
        assert_eq!(
            opts.relation_domains,
            vec!["call".to_string(), "dependency".to_string()]
        );
        assert!(!opts.include_external);
    }

    #[test]
    fn absent_filters_leave_defaults() {
        let opts = apply_filters(RelationFilterParams::default(), RelationQueryOptions::new());
        assert!(opts.excluded_files.is_empty());
        assert!(opts.relation_domains.is_empty());
    }
}
