//! Mixed tokenizer / text cleaner benchmark.
//!
//! Covers the mixed-tokenizer benchmark need: Chinese dict segmentation versus
//! identifier splitting on index and retrieval paths. Small inputs,
//!
//! Run with: `cargo run --bench mixed_tokenizer`
//!
//! Results are printed to stdout and appended to
//! `benches/results/mixed_tokenizer.tsv`.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use cce_text::{Bm25TextCleaner, MixedTokenizer};

fn chinese_dense() -> String {
    "数据库连接池初始化失败重试机制缓存穿透保护限流降级熔断器配置中心注册发现心跳检测负载均衡会话保持分布式锁事务补偿幂等性保障"
        .repeat(20)
}

fn identifier_dense() -> String {
    "calculateTotalAmountWithDiscountAndTaxForUserSessionManagerFactoryProviderImpl ".repeat(200)
}

fn mixed() -> String {
    "用户User登录login会话session令牌token过期expire刷新refresh重试retry机制mechanism ".repeat(100)
}

fn long_title() -> String {
    "calculator::calculate_total_amount_with_discount_and_tax_for_user_session_manager_factory "
        .repeat(10)
}

fn chinese_long() -> String {
    "数据库连接池初始化失败重试机制缓存穿透保护限流降级熔断器配置中心注册发现心跳检测负载均衡会话保持分布式锁事务补偿幂等性保障"
        .repeat(600)
}

fn identifier_long() -> String {
    "calculateTotalAmountWithDiscountAndTaxForUserSessionManagerFactoryProviderImpl ".repeat(2000)
}

fn bench_ms(iters: usize, mut f: impl FnMut()) -> f64 {
    f();
    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    start.elapsed().as_secs_f64() * 1000.0 / iters as f64
}

fn main() {
    println!("mixed_tokenizer benchmark (debug, small inputs)");
    println!(
        "{:<12} {:>12} {:>12} {:>12}",
        "sample", "tokenize", "offsets", "clean"
    );

    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/results/mixed_tokenizer.tsv"))
        .ok();
    if let Some(f) = out.as_mut() {
        let _ = writeln!(f, "# sample\ttokenize_ms\toffsets_ms\tclean_ms\ttokens");
    }

    let tokenizer = MixedTokenizer::new();
    let cleaner = Bm25TextCleaner::new();
    let samples = [
        ("chinese", chinese_dense()),
        ("ident", identifier_dense()),
        ("mixed", mixed()),
        ("title", long_title()),
        ("chinese_long", chinese_long()),
        ("ident_long", identifier_long()),
    ];
    for (label, text) in &samples {
        let tokens = tokenizer.tokenize(text).len();
        let t1 = bench_ms(5, || {
            let _ = tokenizer.tokenize(text);
        });
        let t2 = bench_ms(5, || {
            let _ = tokenizer.tokenize_offsets(text);
        });
        let t3 = bench_ms(5, || {
            let _ = cleaner.clean(text);
        });
        println!("{label:<12} {t1:>12.2} {t2:>12.2} {t3:>12.2}");
        if let Some(f) = out.as_mut() {
            let _ = writeln!(f, "{label}\t{t1:.2}\t{t2:.2}\t{t3:.2}\t{tokens}");
        }
    }
    println!("truncated_inputs: {}", cce_text::truncated_input_count());
}
