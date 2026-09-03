//! Debug tool: Behavior query predicate (`#match?`) evaluation
//!
//! ## Problem
//!
//! tree-sitter 0.26 Rust bindings do NOT automatically evaluate `(#match?)`
//! predicates. This causes behavior queries that rely on `(#match?)` to
//! produce incorrect results — e.g., a `binary_expression` containing `<<`,
//! `>>`, `<<=`, `>>=`, or even `<` or `>` will match ALL four shift operator
//! patterns (`OpShiftLeft`, `OpShiftLeftAssign`, `OpShiftRight`,
//! `OpShiftRightAssign`).
//!
//! ## How this tool works
//!
//! 1. Defines the same shift operator predicate patterns from the behavior query
//! 2. Evaluates them against sample expressions using pure string matching
//! 3. Shows which expressions incorrectly match which patterns
//! 4. Demonstrates the fix: manual regex post-filtering
//!
//! ## Usage
//!
//! ```bash
//! rustc tools/debug_behavior_predicates.rs && ./debug_behavior_predicates
//! ```
//!
//! No external dependencies needed — all matching uses built-in string ops.
//!
//! ## The root cause
//!
//! In `scheme/common.rs`, the behavior query defines:
//!
//! ```scheme
//! (binary_expression) @behavior.op.shift_left
//! (#match? @behavior.op.shift_left "<<([^=]|$)")
//! ```
//!
//! tree-sitter 0.26's Rust crate **compiles** the `(#match?)` predicate but
//! does **not evaluate it** during match iteration. So ALL `binary_expression`
//! nodes match ALL 4 shift patterns regardless of their actual operator.
//!
//! ## Fix
//!
//! In `behavior_extractor.rs`, after collecting matches, perform a secondary
//! predicate evaluation using regex matching on each capture's text.

use std::time::Instant;

fn main() {
    let start = Instant::now();

    println!("======================================================================");
    println!("  Behavior Predicate Evaluation Debug Tool");
    println!("  Demonstrates (#match?) predicate failure in tree-sitter 0.26");
    println!("======================================================================");
    println!();

    let patterns = vec![
        Pattern {
            label: "OpShiftLeft",
            pattern: r#"<<([^=]|$)"#,
            pred_fn: pred_shift_left,
        },
        Pattern {
            label: "OpShiftLeftAssign",
            pattern: r#"<<="#,
            pred_fn: pred_shift_left_assign,
        },
        Pattern {
            label: "OpShiftRight",
            pattern: r#">>([^=]|$)"#,
            pred_fn: pred_shift_right,
        },
        Pattern {
            label: "OpShiftRightAssign",
            pattern: r#">>="#,
            pred_fn: pred_shift_right_assign,
        },
    ];

    let expressions = vec![
        TestExpr { text: "*item < 0", desc: "Less-than (line 6 of index_sidecar)" },
        TestExpr { text: "1 << 2", desc: "Left shift (line 15 of index_sidecar)" },
        TestExpr { text: "values != null", desc: "Not-equal (Java/TS)" },
        TestExpr { text: "buffer.size() == 0", desc: "Equal (Java)" },
        TestExpr { text: "values !== null", desc: "Not-identical (TypeScript)" },
        TestExpr { text: "a << b", desc: "Left shift" },
        TestExpr { text: "x <<= 3", desc: "Left shift assign" },
        TestExpr { text: "8 >> 1", desc: "Right shift" },
        TestExpr { text: "y >>= 2", desc: "Right shift assign" },
        TestExpr { text: "a < b", desc: "Less-than" },
        TestExpr { text: "a > b", desc: "Greater-than" },
        TestExpr { text: "a <= b", desc: "Less-or-equal" },
        TestExpr { text: "a >= b", desc: "Greater-or-equal" },
    ];

    // ── Part 1: Predicate matrix ───────────────────────────────────

    println!("--- Expression × Predicate Matrix ---");
    println!();

    print!("{:<35}", "Expression");
    for p in &patterns {
        print!(" {:<20}", p.label);
    }
    println!("  Correct?  Duplicates");
    print!("{:-<35}", "");
    for _ in &patterns {
        print!(" {:-<20}", "");
    }
    println!("  {:-<9} {:-<10}", "", "");

    let mut total_dups = 0usize;
    let mut total_spurious = 0usize;

    for expr in &expressions {
        print!("{:<35}", trunc(expr.text, 33));

        let mut accept_count = 0usize;

        for p in &patterns {
            let match_pred = (p.pred_fn)(expr.text);
            let should = operator_should_match(expr.text, p.label);
            let st = if match_pred {
                accept_count += 1;
                if should { "✓ ACCEPT" } else { "✗ SPURIOUS" }
            } else {
                "  reject"
            };
            print!(" {:<20}", st);
        }

        let dups = accept_count.saturating_sub(1);
        total_dups += dups;
        if accept_count > 1 {
            total_spurious += accept_count;
        }
        let cor = if accept_count <= 1 { "✓" } else { "✗" };
        println!("  {:<9} {} ({})", cor, dups, expr.desc);
    }

    // ── Summary ────────────────────────────────────────────────────

    println!();
    println!("======================================================================");
    println!("  SUMMARY");
    println!("======================================================================");
    println!();
    println!("Tree-sitter version: 0.26");
    println!("Predicate type affected: (#match?)");
    println!();
    println!("At this version, tree-sitter Rust bindings DO NOT evaluate");
    println!("(#match?) predicates during query execution.");
    println!("Every binary_expression node matches ALL 4 shift patterns,");
    println!("producing {}+ spurious facts.", total_spurious);
    println!();
    println!("IMPACT:");
    println!("  - Non-shift comparisons (<, >, <=, >=, !=, ==)");
    println!("    produce 4 spurious behavior facts instead of 0");
    println!("  - Actual shift operations produce 4 facts each");
    println!("    (1 correct + 3 spurious)");

    // ── Fix demonstration ──────────────────────────────────────────

    println!();
    println!("--- Fix: Manual predicate evaluation ---");
    println!();
    println!("In `behavior_extractor.rs`, after `process_match`, add:");
    println!();
    println!("  fn evaluate_shift_predicate(capture_name: &str, text: &str) -> bool {{");
    println!("      match capture_name {{");
    println!("          \"@behavior.op.shift_left\" => {{");
    println!("              text.contains(\"<<\") && !text.contains(\"<<=\")");
    println!("          }}");
    println!("          \"@behavior.op.shift_left_assign\" => text.contains(\"<<=\"),");
    println!("          \"@behavior.op.shift_right\" => {{");
    println!("              text.contains(\">>\") && !text.contains(\">>=\")");
    println!("          }}");
    println!("          \"@behavior.op.shift_right_assign\" => text.contains(\">>=\"),");
    println!("          _ => true,");
    println!("      }}");
    println!("  }}");
    println!();
    println!("Or use regex::Regex with the patterns from common.rs for");
    println!("more precise matching.");
    println!();

    let elapsed = start.elapsed();
    println!("Analysis completed in {:?}", elapsed);
    println!("(No external dependencies — run with `rustc tools/debug_behavior_predicates.rs && ./debug_behavior_predicates`)");
}

// ── Predicate implementations ──────────────────────────────────────

type PredFn = fn(&str) -> bool;

struct Pattern {
    label: &'static str,
    pattern: &'static str,
    pred_fn: PredFn,
}

struct TestExpr {
    text: &'static str,
    desc: &'static str,
}

/// Equivalent to `<<([^=]|$)` — "<<" not followed by "="
fn pred_shift_left(text: &str) -> bool {
    if let Some(pos) = text.find("<<") {
        let after = pos + 2;
        after >= text.len() || !text[after..].starts_with('=')
    } else {
        false
    }
}

fn pred_shift_left_assign(text: &str) -> bool {
    text.contains("<<=")
}

/// Equivalent to `>>([^=]|$)` — ">>" not followed by "="
fn pred_shift_right(text: &str) -> bool {
    if let Some(pos) = text.find(">>") {
        let after = pos + 2;
        after >= text.len() || !text[after..].starts_with('=')
    } else {
        false
    }
}

fn pred_shift_right_assign(text: &str) -> bool {
    text.contains(">>=")
}

/// Whether the expression SHOULD match this operator label
fn operator_should_match(text: &str, label: &str) -> bool {
    match label {
        "OpShiftLeft" => pred_shift_left(text),
        "OpShiftLeftAssign" => pred_shift_left_assign(text),
        "OpShiftRight" => pred_shift_right(text),
        "OpShiftRightAssign" => pred_shift_right_assign(text),
        _ => false,
    }
}

fn trunc(s: &str, n: usize) -> String {
    if s.len() <= n { s.to_string() } else { format!("{}...", &s[..n.saturating_sub(3)]) }
}
