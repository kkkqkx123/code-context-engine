// Comment processing tests grouped by concern.
use super::associator::{gap_is_adjacent, is_go_doc_line, merge_plain_comment_blocks};

use super::classifier::{CommentClass, classify_comment, merge_top_level_line_comments};

use super::*;

use crate::parser::ast_parser::AstParser;

use cce_types::EntityId;

fn make_comment(text: &str, row: usize, col: usize, name: &str) -> Comment {
    Comment {
        text: text.to_string(),
        span: Span::new(0, text.len(), row, col, row, col + text.len()),
        capture_name: name.to_string(),
    }
}

fn make_entity(
    id: u64,
    start_byte: usize,
    end_byte: usize,
    start_row: usize,
    end_row: usize,
    name: &str,
    kind: cce_types::EntityKind,
) -> Entity {
    Entity {
        id: EntityId(id),
        kind,
        name: name.to_string(),
        signature: String::new(),
        parameters: vec![],
        return_type: None,
        span: Span::new(start_byte, end_byte, start_row, 0, end_row, 0),
        depth: 0,
        parent: None,
        children: vec![],
        doc_comment: None,
        modifiers: Vec::new(),
        attributes: std::collections::HashMap::new(),
        metadata: std::collections::HashMap::new(),
        is_stdlib: false,
        subtype: None,
        stdlib_category: None,
    }
}

mod license {
    use super::super::license_detector::{find_license_blocks, is_license_span};
    use super::*;
    use cce_config::{LicenseHeaderConfig, LicenseHeaderRule};

    fn rule(prefix: &str, suffix: Option<&str>) -> LicenseHeaderRule {
        LicenseHeaderRule {
            prefix: prefix.to_string(),
            suffix: suffix.map(|s| s.to_string()),
        }
    }

    fn config_with(rules: Vec<LicenseHeaderRule>) -> LicenseHeaderConfig {
        LicenseHeaderConfig {
            enabled: true,
            max_comments: 30,
            rules,
        }
    }

    /// Build line comments laid out at the given (text, row) pairs with
    /// sequential byte ranges; returns the comments and the next free byte.
    fn header(lines: &[(&str, usize)]) -> (Vec<Comment>, usize) {
        let mut comments = Vec::new();
        let mut byte = 0usize;
        for (text, row) in lines {
            comments.push(Comment {
                text: text.to_string(),
                span: Span::new(byte, byte + text.len(), *row, 0, *row, text.len()),
                capture_name: "comment.line".to_string(),
            });
            byte += text.len() + 1;
        }
        (comments, byte)
    }

    fn trailing_entity(byte: usize) -> Vec<Entity> {
        vec![make_entity(
            1,
            byte,
            byte + 10,
            100,
            100,
            "e",
            cce_types::EntityKind::Function,
        )]
    }

    #[test]
    fn prefix_and_suffix_close_the_block() {
        let (comments, byte) = header(&[
            ("// Copyright 2024 Acme Corp", 0),
            ("// Some intermediate notice", 1),
            ("// All rights reserved.", 2),
            ("// A module note below", 3),
        ]);
        let blocks = find_license_blocks(&comments, &trailing_entity(byte), &rule_pair());
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].start_byte, comments[0].span.start_byte);
        assert_eq!(blocks[0].end_byte, comments[2].span.end_byte);
        assert!(is_license_span(&comments[1].span, &blocks));
        assert!(!is_license_span(&comments[3].span, &blocks));
    }

    fn rule_pair() -> LicenseHeaderConfig {
        config_with(vec![rule("copyright", Some("all rights reserved"))])
    }

    #[test]
    fn unclosed_suffix_keeps_everything() {
        let (comments, byte) = header(&[
            ("// Copyright 2024 Acme Corp", 0),
            ("// but the suffix line never appears here", 1),
        ]);
        let blocks = find_license_blocks(&comments, &trailing_entity(byte), &rule_pair());
        assert!(blocks.is_empty());
    }

    #[test]
    fn block_starts_at_prefix_line_not_run_start() {
        let (comments, byte) = header(&[
            ("#!/usr/bin/env python", 0),
            ("# Copyright 2024 Acme Corp", 1),
            ("# All rights reserved.", 2),
        ]);
        let blocks = find_license_blocks(&comments, &trailing_entity(byte), &rule_pair());
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].start_byte, comments[1].span.start_byte);
    }

    #[test]
    fn matching_is_case_insensitive() {
        let (comments, byte) =
            header(&[("// COPYRIGHT 2024 ACME", 0), ("// ALL RIGHTS RESERVED", 1)]);
        let blocks = find_license_blocks(&comments, &trailing_entity(byte), &rule_pair());
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn multi_line_block_comment_matches_per_line() {
        let text = "/*\n * Copyright 2024 Acme Corp\n * details here\n * All rights reserved.\n */";
        let comments = vec![Comment {
            text: text.to_string(),
            span: Span::new(0, text.len(), 0, 0, 4, 3),
            capture_name: "comment.block".to_string(),
        }];
        let blocks = find_license_blocks(&comments, &trailing_entity(text.len() + 1), &rule_pair());
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].start_byte, 0);
    }

    #[test]
    fn several_blocks_inside_one_run() {
        let (comments, byte) = header(&[
            ("// Copyright A", 0),
            ("// All rights reserved.", 1),
            ("// Copyright B", 2),
            ("// All rights reserved.", 3),
        ]);
        let blocks = find_license_blocks(&comments, &trailing_entity(byte), &rule_pair());
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].end_byte, comments[1].span.end_byte);
        assert_eq!(blocks[1].start_byte, comments[2].span.start_byte);
    }

    #[test]
    fn suffixless_rule_takes_run_remainder() {
        let (comments, byte) = header(&[
            ("// SPDX-License-Identifier: MIT", 0),
            ("// some trailing notice", 1),
        ]);
        let config = config_with(vec![rule("spdx-license-identifier:", None)]);
        let blocks = find_license_blocks(&comments, &trailing_entity(byte), &config);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].end_byte, comments[1].span.end_byte);
    }

    #[test]
    fn blank_line_gap_splits_runs() {
        // One blank line (rows 1 -> 3) keeps one run; two blank lines split it.
        let (comments, byte) = header(&[
            ("// Copyright A", 0),
            ("// just a note", 1),
            ("// Copyright B", 4),
            ("// All rights reserved.", 5),
        ]);
        let blocks = find_license_blocks(&comments, &trailing_entity(byte), &rule_pair());
        // Run 1 has no closing suffix, so only block B is removed.
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].start_byte, comments[2].span.start_byte);
    }

    #[test]
    fn one_blank_line_inside_run_is_contiguous() {
        let (comments, byte) = header(&[
            ("// Copyright 2024 Acme Corp", 0),
            ("", 1),
            ("// All rights reserved.", 2),
        ]);
        let comments: Vec<Comment> = comments
            .into_iter()
            .enumerate()
            .filter_map(|(i, c)| if i == 1 { None } else { Some(c) })
            .collect();
        let blocks = find_license_blocks(&comments, &trailing_entity(byte), &rule_pair());
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn max_comments_bounds_the_header() {
        let (comments, byte) = header(&[
            ("// project note", 0),
            ("// another note", 1),
            ("// Copyright 2024 Acme Corp", 2),
            ("// All rights reserved.", 3),
        ]);
        let config = LicenseHeaderConfig {
            max_comments: 2,
            ..rule_pair()
        };
        assert!(find_license_blocks(&comments, &trailing_entity(byte), &config).is_empty());
        // With the full budget the block is found.
        assert_eq!(
            find_license_blocks(&comments, &trailing_entity(byte), &rule_pair()).len(),
            1
        );
    }

    #[test]
    fn disabled_config_matches_nothing() {
        let (comments, byte) = header(&[
            ("// Copyright 2024 Acme Corp", 0),
            ("// All rights reserved.", 1),
        ]);
        let config = LicenseHeaderConfig {
            enabled: false,
            ..rule_pair()
        };
        assert!(find_license_blocks(&comments, &trailing_entity(byte), &config).is_empty());
    }

    #[test]
    fn custom_rules_replace_builtins() {
        let (comments, byte) = header(&[
            ("// Copyright 2024 Acme Corp", 0),
            ("// All rights reserved.", 1),
            ("// license-ref: internal-eula", 2),
        ]);
        let config = config_with(vec![rule("license-ref:", None)]);
        let blocks = find_license_blocks(&comments, &trailing_entity(byte), &config);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].start_byte, comments[2].span.start_byte);
    }

    #[test]
    fn misleading_words_are_not_license() {
        // The legacy contains-based matcher removed these; literals must not.
        let (comments, byte) = header(&[
            ("// Note: please do not alter this license notice", 0),
            ("// See the LICENSE file for more details", 1),
        ]);
        let blocks = find_license_blocks(
            &comments,
            &trailing_entity(byte),
            &LicenseHeaderConfig::default(),
        );
        assert!(blocks.is_empty());
    }

    #[test]
    fn builtin_covers_apache_gpl_and_mit_endings() {
        let cases: Vec<Vec<&str>> = vec![
            vec![
                "// Copyright 2024 Acme",
                "// limitations under the License.",
            ],
            vec![
                "// Copyright (C) 2024 Acme",
                "// GNU General Public License for more details.",
            ],
            vec![
                "// Copyright 2024 Acme",
                "// IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER",
                "// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING",
                "// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER",
                "// DEALINGS IN THE SOFTWARE.",
            ],
        ];
        for lines in cases {
            let pairs: Vec<(&str, usize)> =
                lines.iter().enumerate().map(|(i, l)| (*l, i)).collect();
            let (comments, byte) = header(&pairs);
            let blocks = find_license_blocks(
                &comments,
                &trailing_entity(byte),
                &LicenseHeaderConfig::default(),
            );
            assert_eq!(blocks.len(), 1, "builtin rules for {lines:?}");
        }
    }

    #[test]
    fn comments_after_first_entity_are_not_scanned() {
        let (comments, byte) = header(&[("// Copyright 2024 Acme Corp", 0)]);
        let mut entities = trailing_entity(byte / 2);
        entities[0].span.start_byte = 1;
        entities[0].span.end_byte = byte + 10;
        let blocks = find_license_blocks(&comments, &entities, &rule_pair());
        assert!(blocks.is_empty());
    }

    #[test]
    fn processor_strips_license_but_keeps_header_note() {
        let processor = CommentProcessor::new().with_license_config(LicenseHeaderConfig::default());
        let mut parser = AstParser::new();
        let code = "// Copyright 2024 Acme Corp\n\
                    // All rights reserved.\n\
                    // Module-level note\n\
                    use std::sync::Once;\n";
        let tree = parser
            .parse_with_tree(code, &Language::Rust)
            .expect("Failed to parse")
            .0;
        let mut entities = vec![make_entity(
            1,
            code.find("use std::sync::Once").unwrap(),
            code.len(),
            3,
            3,
            "X",
            cce_types::EntityKind::Function,
        )];
        let mut behavior = BehaviorStore::default();

        processor
            .process_with_span(&tree, code, &Language::Rust, &mut entities, &mut behavior)
            .expect("Failed to process comments");

        let facts = behavior
            .get(FILE_DOC_SENTINEL_ID)
            .expect("sentinel facts expected");
        assert!(
            facts
                .facts
                .iter()
                .any(|f| f.text.contains("Module-level note")),
            "non-license header note must survive"
        );
        assert!(
            facts
                .facts
                .iter()
                .all(|f| !f.text.contains("Copyright") && !f.text.contains("rights reserved")),
            "license lines must be stripped"
        );
    }
}

mod associate;
mod classify_merge;
mod clean;
mod extract;
mod go;
mod python;
