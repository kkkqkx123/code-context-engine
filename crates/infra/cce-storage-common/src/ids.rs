//! Structured point/document ID helpers.
//!
//! Chunk, vector-point, and BM25 document IDs follow the pattern
//! `{prefix}::{epoch}::{suffix}`. Compaction rewrites the epoch field when
//! rows move between generations.

/// Replace the epoch field in a structured ID without substring collisions.
///
/// A naive `replace(&format!("::{old_epoch}::"), ...)` can mis-fire when
/// epochs share a digit prefix (e.g. old=1, new=11 turns `::11::` into
/// `::111::`). Splitting on `::` and replacing only the second field avoids
/// this.
pub fn replace_epoch_in_id(id: &str, old_epoch: i64, new_epoch: i64) -> String {
    let mut parts: Vec<&str> = id.split("::").collect();
    if parts.len() >= 2 && parts[1] == old_epoch.to_string() {
        let new_epoch_str = new_epoch.to_string();
        parts[1] = &new_epoch_str;
        return parts.join("::");
    }
    id.to_string()
}

/// Replace epochs inside a JSON array of ID strings.
pub fn replace_epoch_in_id_list(list: &str, old_epoch: i64, new_epoch: i64) -> String {
    let Ok(mut ids) = serde_json::from_str::<Vec<String>>(list) else {
        return list.to_string();
    };
    for id in &mut ids {
        *id = replace_epoch_in_id(id, old_epoch, new_epoch);
    }
    serde_json::to_string(&ids).unwrap_or_else(|_| list.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_epoch_in_id_handles_digit_overlap() {
        assert_eq!(
            replace_epoch_in_id("grp::1::chunk-x", 1, 11),
            "grp::11::chunk-x"
        );
        assert_eq!(
            replace_epoch_in_id("grp::11::chunk-x", 1, 11),
            "grp::11::chunk-x"
        );
        assert_eq!(
            replace_epoch_in_id("grp::1::chunk-y", 1, 2),
            "grp::2::chunk-y"
        );
    }

    #[test]
    fn replace_epoch_in_id_list_replaces_all_entries() {
        let input = serde_json::to_string(&vec![
            "grp::1::chunk-a".to_string(),
            "grp::1::chunk-b".to_string(),
        ])
        .unwrap();
        let expected = serde_json::to_string(&vec![
            "grp::3::chunk-a".to_string(),
            "grp::3::chunk-b".to_string(),
        ])
        .unwrap();
        assert_eq!(replace_epoch_in_id_list(&input, 1, 3), expected);
    }
}
