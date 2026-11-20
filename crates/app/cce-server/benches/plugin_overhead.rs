//! Plugin load and per-hook overhead with a real Lua plugin loaded from a
//! project-local `.cce/plugins.json`.
//!
//! Run: `cargo bench -p cce-server --features plugins --bench plugin_overhead`
//! Report: `benches/results/plugin_overhead.tsv`.
//!
//! A native plugin can be added by pointing `CCE_BENCH_NATIVE_PLUGIN` at a
//! locally built `.so`; the same registry then loads both branches.

use std::io::Write as _;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use cce_bench_kit::{DEFAULT_LANG_DIST, ProjectSize, SyntheticProject};
use cce_plugin::{CodePlugin, PluginSource};
use cce_plugin_runtime::{FilePluginSource, PluginEntry, PluginRegistryFile, PluginType};
use cce_types::grouper::EntityGroup;

const LUA_SCRIPT: &str = r#"
plugin = {
    id = "bench_hooks",
    name = "Benchmark Hooks",
    version = "1.0.0",
    priority = 1000,
    capabilities = { "text_gen", "symbol_extract", "file_filter", "query_rewrite" }
}

function plugin.generate_bm25(group)
    return "bench bm25 text " .. group.name
end

function plugin.generate_embedding(group)
    return "bench embedding text " .. group.name
end

function plugin.extract_symbols(content, file_path, language)
    return {
        { id = file_path .. ":bench", name = "bench_symbol", kind = "function" }
    }
end

function plugin.filter_file(file_path, is_directory, size)
    if string.find(file_path, "bench_skip", 1, true) then
        return "exclude"
    end
    return nil
end

function plugin.rewrite_query(query)
    if string.find(query, "tf", 1, true) then
        return { rewritten_query = string.gsub(query, "tf", "tensorflow"), expansion_terms = { "tensorflow" } }
    end
    return nil
end
"#;

fn write_registry(root: &std::path::Path, entries: Vec<PluginEntry>) {
    let dir = root.join(".cce").join("plugins");
    std::fs::create_dir_all(&dir).expect("plugin dir");
    std::fs::write(dir.join("hooks.lua"), LUA_SCRIPT).expect("write lua plugin");
    let registry = PluginRegistryFile { plugins: entries };
    std::fs::write(
        root.join(".cce").join("plugins.json"),
        serde_json::to_string_pretty(&registry).expect("serialize registry"),
    )
    .expect("write plugins.json");
}

fn lua_entry() -> PluginEntry {
    PluginEntry {
        id: "bench_hooks".to_string(),
        path: "plugins/hooks.lua".to_string(),
        enabled: true,
        description: None,
        plugin_type: PluginType::Lua,
        file_patterns: None,
        languages: None,
        capabilities: None,
        priority: None,
        capability_priorities: None,
    }
}

fn native_entry(path: &str) -> PluginEntry {
    PluginEntry {
        id: "bench_native".to_string(),
        path: path.to_string(),
        enabled: true,
        description: None,
        plugin_type: PluginType::Native,
        file_patterns: None,
        languages: None,
        capabilities: None,
        priority: None,
        capability_priorities: None,
    }
}

fn main() {
    let root = tempfile::TempDir::new().expect("plugin fixture root");
    let native = std::env::var("CCE_BENCH_NATIVE_PLUGIN").ok();
    let mut entries = vec![lua_entry()];
    if let Some(path) = &native {
        entries.push(native_entry(path));
    }
    write_registry(root.path(), entries);

    let registry_started = Instant::now();
    let bundles = FilePluginSource::from_project(root.path(), None)
        .collect()
        .expect("plugin registry loads");
    let registry_ms = registry_started.elapsed().as_secs_f64() * 1000.0;
    assert!(
        !bundles.is_empty(),
        "benchmark requires at least one plugin"
    );

    let lua_path = root.path().join(".cce/plugins/hooks.lua");
    let lua_started = Instant::now();
    let lua = cce_plugin_runtime::LuaPlugin::from_script(
        &std::fs::read_to_string(&lua_path).expect("read lua script"),
    )
    .expect("lua plugin initializes");
    let lua_init_ms = lua_started.elapsed().as_secs_f64() * 1000.0;

    let native_init_ms = match &native {
        Some(path) => {
            let started = Instant::now();
            let plugin = cce_plugin_runtime::NativePlugin::load(std::path::Path::new(path))
                .expect("native plugin initializes");
            let elapsed = started.elapsed().as_secs_f64() * 1000.0;
            drop(plugin);
            elapsed
        }
        None => 0.0,
    };

    let project = SyntheticProject::generate(ProjectSize::Small, DEFAULT_LANG_DIST);
    let group = EntityGroup::default();
    let files = project.files.len();
    let started = Instant::now();
    let mut filter_ns = 0u128;
    let mut symbols_ns = 0u128;
    let mut bm25_ns = 0u128;
    let mut embedding_ns = 0u128;
    let mut script_ns = 0u64;
    let mut dispatch_ns = 0u64;
    for file in &project.files {
        let mark = Instant::now();
        let _ = lua.filter_file(&file.path.to_string_lossy(), false, 100);
        filter_ns += mark.elapsed().as_nanos();
        let timing = lua.last_hook_timing();
        script_ns += timing.script_us;
        dispatch_ns += timing.dispatch_us;
        let mark = Instant::now();
        let _ = lua.extract_symbols(&file.content, &file.path.to_string_lossy(), file.language);
        symbols_ns += mark.elapsed().as_nanos();
        let timing = lua.last_hook_timing();
        script_ns += timing.script_us;
        dispatch_ns += timing.dispatch_us;
        let mark = Instant::now();
        let _ = lua.generate_bm25(&group);
        bm25_ns += mark.elapsed().as_nanos();
        let timing = lua.last_hook_timing();
        script_ns += timing.script_us;
        dispatch_ns += timing.dispatch_us;
        let mark = Instant::now();
        let _ = lua.generate_embedding(&group);
        embedding_ns += mark.elapsed().as_nanos();
        let timing = lua.last_hook_timing();
        script_ns += timing.script_us;
        dispatch_ns += timing.dispatch_us;
    }
    let hook_wall_ms = started.elapsed().as_secs_f64() * 1000.0;

    let queries = 200;
    let mut query_script_ns = 0u64;
    let mut query_dispatch_ns = 0u64;
    let started = Instant::now();
    for _ in 0..queries {
        let _ = lua.rewrite_query("tf nlp search").expect("rewrite hook");
        let timing = lua.last_hook_timing();
        query_script_ns += timing.script_us;
        query_dispatch_ns += timing.dispatch_us;
    }
    let query_ms = started.elapsed().as_secs_f64() * 1000.0;

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after unix epoch")
        .as_nanos();
    let run_id = format!("{:x}", nanos & 0xffff_ffff_ffff);
    let mut out = cce_bench_kit::open_report_with_header(
        env!("CARGO_MANIFEST_DIR"),
        "plugin_overhead.tsv",
        "# run_id\tplugin_count\tfiles\tregistry_load_ms\tlua_init_ms\tnative_init_ms\tfilter_us_per_file\tsymbol_us_per_file\tbm25_us_per_file\tembedding_us_per_file\thook_wall_ms\tscript_us_per_hook\tdispatch_us_per_hook\tquery_rewrite_us_per_call\tquery_script_us_per_call\tquery_dispatch_us_per_call\tquery_wall_ms",
    );
    let per_file = |total: u128| total as f64 / files as f64 / 1000.0;
    let hooks = (files * 4) as u64;
    let per_hook = |total: u64, count: u64| total as f64 / count as f64;
    let _ = writeln!(
        out,
        "{run_id}\t{}\t{files}\t{registry_ms:.3}\t{lua_init_ms:.3}\t{native_init_ms:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{hook_wall_ms:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{query_ms:.3}",
        bundles.len(),
        per_file(filter_ns),
        per_file(symbols_ns),
        per_file(bm25_ns),
        per_file(embedding_ns),
        per_hook(script_ns, hooks),
        per_hook(dispatch_ns, hooks),
        query_ms * 1000.0 / queries as f64,
        query_script_ns as f64 / queries as f64,
        query_dispatch_ns as f64 / queries as f64,
    );
    println!(
        "plugins={} files={files} registry={registry_ms:.2}ms filter={:.3}us/file",
        bundles.len(),
        per_file(filter_ns)
    );
}
