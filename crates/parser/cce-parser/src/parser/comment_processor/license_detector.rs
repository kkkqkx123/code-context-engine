use super::Comment;
use cce_types::Entity;

pub(crate) fn find_license_block_end(comments: &[Comment], entities: &[Entity]) -> usize {
    let first_entity_start = match entities.first() {
        Some(e) => e.span.start_byte,
        None => return 0,
    };

    let mut prev_end: Option<usize> = None;

    for comment in comments {
        if comment.span.start_byte >= first_entity_start {
            break;
        }

        let is_contiguous = match prev_end {
            Some(end) => comment.span.start_byte <= end + 2,
            None => true,
        };

        if !is_contiguous {
            break;
        }

        if is_license_text(&comment.text) {
            prev_end = Some(comment.span.end_byte);
        } else if prev_end.is_some() {
            break;
        } else {
            return 0;
        }
    }

    prev_end.unwrap_or(0)
}

fn is_license_text(text: &str) -> bool {
    let normalized = text.to_lowercase();
    let license_markers = [
        "copyright",
        "licensed under",
        "spdx-license-identifier",
        "mit license",
        "apache license",
        "gnu general public license",
        "gpl-",
        "lgpl-",
        "bsd-",
        "mozilla public license",
        "unlicense",
        "all rights reserved",
        "see license",
        "see the license",
        "free software foundation",
        "redistribution and use",
        "do not alter",
    ];

    let text_to_check: String = normalized
        .lines()
        .map(|line| {
            line.trim_start_matches('/')
                .trim_start_matches('#')
                .trim_start_matches('*')
                .trim()
        })
        .collect::<Vec<_>>()
        .join(" ");

    license_markers
        .iter()
        .any(|marker| text_to_check.contains(marker))
}
