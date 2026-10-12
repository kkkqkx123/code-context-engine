//! Admission context and host scoping helpers.
//!
//! The context travels in request extensions after successful verification.
//! Handlers and the middleware share it to decide whether a project id falls
//! inside the token binding.

/// Verified identity attached to admitted requests.
#[derive(Debug, Clone)]
pub struct AdmissionContext {
    /// Log-safe token fingerprint, never the token itself.
    pub fingerprint: String,
    /// Explicitly authorized project ids. Empty authorizes no project scope.
    pub projects: Vec<i64>,
    /// Stored byte quota for pushed content. None means unlimited.
    pub quota_bytes: Option<u64>,
}

impl AdmissionContext {
    /// Create a context from a verified stored token.
    pub fn new(fingerprint: String, projects: Vec<i64>, quota_bytes: Option<u64>) -> Self {
        Self {
            fingerprint,
            projects,
            quota_bytes,
        }
    }

    /// Whether the token binding covers the given project id.
    pub fn allows_project(&self, project_id: i64) -> bool {
        self.projects.contains(&project_id)
    }
}

/// Whether the bind host is a loopback address.
pub fn is_loopback_host(host: &str) -> bool {
    let normalized = host.trim().to_lowercase();
    normalized == "127.0.0.1"
        || normalized == "::1"
        || normalized == "[::1]"
        || normalized == "localhost"
}

/// Whether the request path bypasses authentication.
pub fn is_public_path(path: &str, prefixes: &[String]) -> bool {
    prefixes
        .iter()
        .any(|prefix| path == prefix || path.starts_with(&format!("{prefix}/")))
}

/// Whether serving on the host requires the admission layer.
pub fn requires_admission(host: &str) -> bool {
    !is_loopback_host(host)
}

/// Extract the project id from project-scoped route paths.
///
/// Routes spell the id as `/api/project/{id}` or
/// `/api/project/{project_id}`; collection routes without an id yield none.
pub fn project_id_from_path(path: &str) -> Option<i64> {
    let clean = path.split('?').next().unwrap_or(path);
    let mut segments = clean.split('/').filter(|s| !s.is_empty());
    if segments.next()? != "api" {
        return None;
    }
    if segments.next()? != "project" {
        return None;
    }
    segments.next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_detection_covers_common_forms() {
        assert!(is_loopback_host("127.0.0.1"));
        assert!(is_loopback_host("localhost"));
        assert!(is_loopback_host("::1"));
        assert!(!is_loopback_host("0.0.0.0"));
        assert!(!is_loopback_host("192.168.1.10"));
    }

    #[test]
    fn path_project_extraction_handles_known_routes() {
        assert_eq!(project_id_from_path("/api/project/12/index"), Some(12));
        assert_eq!(project_id_from_path("/api/project/7/graph/ego"), Some(7));
        assert_eq!(project_id_from_path("/api/project"), None);
        assert_eq!(project_id_from_path("/api/search"), None);
    }

    #[test]
    fn public_path_matching_supports_prefixes() {
        let prefixes = vec!["/api/health".to_string()];
        assert!(is_public_path("/api/health", &prefixes));
        assert!(is_public_path("/api/health/qdrant", &prefixes));
        assert!(!is_public_path("/api/search", &prefixes));
    }

    #[test]
    fn path_extraction_strips_query_and_rejects_non_numeric() {
        assert_eq!(
            project_id_from_path("/api/project/12/index?verbose=true"),
            Some(12)
        );
        assert_eq!(project_id_from_path("/api/project/abc/index"), None);
        assert_eq!(project_id_from_path("/api/project/7/ingest/batch"), Some(7));
        assert_eq!(project_id_from_path("/api/admission/stats"), None);
    }

    #[test]
    fn scope_binding_covers_only_listed_projects() {
        let context = AdmissionContext::new("fp".to_string(), vec![3, 7], None);
        assert!(context.allows_project(3));
        assert!(!context.allows_project(4));
        let empty = AdmissionContext::new("fp".to_string(), Vec::new(), None);
        assert!(!empty.allows_project(3));
    }

    #[test]
    fn admission_required_off_loopback() {
        assert!(!requires_admission("127.0.0.1"));
        assert!(!requires_admission("localhost"));
        assert!(requires_admission("0.0.0.0"));
        assert!(requires_admission("example.com"));
    }

    #[test]
    fn path_extraction_handles_trailing_slash_and_collection_routes() {
        assert_eq!(project_id_from_path("/api/project/12/"), Some(12));
        assert_eq!(project_id_from_path("/api/project/12"), Some(12));
        assert_eq!(project_id_from_path("/api/project/"), None);
        assert_eq!(project_id_from_path("/api/project"), None);
        assert_eq!(project_id_from_path("/other/project/12/index"), None);
        assert_eq!(project_id_from_path(""), None);
    }

    #[test]
    fn public_path_rejects_partial_prefix_segments() {
        let prefixes = vec!["/api/health".to_string()];
        assert!(!is_public_path("/api/healthz", &prefixes));
        assert!(!is_public_path("/api/search", &prefixes));
        assert!(is_public_path("/api/health", &prefixes));
    }
}
