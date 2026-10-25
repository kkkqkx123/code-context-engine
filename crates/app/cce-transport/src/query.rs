//! Lightweight query-string builder for remote graph requests.
//!
//! Centralizes URL percent-encoding and parameter ordering so the remote
//! service only declares its parameters instead of hand-formatting paths.

/// Percent-encode a raw string using the historical `urlencoding_sanitize`
/// rules. Kept byte-for-byte compatible with the previous implementation.
pub fn urlencoding_sanitize(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for byte in raw.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// A single structured query parameter value.
#[derive(Debug, Clone)]
pub enum QueryParam {
    /// Text value, percent-encoded.
    Text(String),
    /// Unsigned integer rendered verbatim.
    Number(usize),
    /// Boolean rendered as `true`/`false`.
    Bool(bool),
    /// List joined with commas then percent-encoded.
    List(Vec<String>),
}

impl QueryParam {
    fn render(&self) -> String {
        match self {
            QueryParam::Text(value) => urlencoding_sanitize(value),
            QueryParam::Number(value) => value.to_string(),
            QueryParam::Bool(value) => value.to_string(),
            QueryParam::List(values) => urlencoding_sanitize(&values.join(",")),
        }
    }
}

/// Builder accumulating an ordered set of query parameters onto a path.
#[derive(Debug, Clone)]
pub struct GraphQuery {
    path: String,
    params: Vec<(&'static str, QueryParam)>,
}

impl GraphQuery {
    /// Start from a path that already contains the project id substitution.
    pub fn new(path: String) -> Self {
        Self {
            path,
            params: Vec::new(),
        }
    }

    /// Append one parameter in declaration order.
    pub fn param(mut self, key: &'static str, value: QueryParam) -> Self {
        self.params.push((key, value));
        self
    }

    /// Render the final path including the query string.
    pub fn build(self) -> String {
        if self.params.is_empty() {
            return self.path;
        }
        let mut out = self.path;
        out.push('?');
        let mut first = true;
        for (key, value) in self.params {
            if !first {
                out.push('&');
            }
            first = false;
            out.push_str(key);
            out.push('=');
            out.push_str(&value.render());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_entity_id() {
        assert_eq!(urlencoding_sanitize("sym::foo/bar"), "sym%3A%3Afoo%2Fbar");
    }

    #[test]
    fn builds_ordered_query() {
        let path = GraphQuery::new("/p/1/graph/ego".to_string())
            .param("entity_id", QueryParam::Text("a/b".to_string()))
            .param("depth", QueryParam::Number(2))
            .param("include_external", QueryParam::Bool(true))
            .build();
        assert_eq!(
            path,
            "/p/1/graph/ego?entity_id=a%2Fb&depth=2&include_external=true"
        );
    }

    #[test]
    fn list_joins_with_comma() {
        let path = GraphQuery::new("/x".to_string())
            .param(
                "ids",
                QueryParam::List(vec!["a".to_string(), "b/c".to_string()]),
            )
            .build();
        assert_eq!(path, "/x?ids=a%2Cb%2Fc");
    }
}
