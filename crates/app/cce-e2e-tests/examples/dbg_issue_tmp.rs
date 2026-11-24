use cce_e2e_tests::TestFixture;
use cce_e2e_tests::fixture::FixtureSpec;
use cce_e2e_tests::init_minimal_logging;

fn dump(spec: FixtureSpec, label: &str) {
    let fixture = TestFixture::load(spec).expect("load");
    use cce_parser::parser::ParseCoordinator;
    let mut files = Vec::new();
    let mut stack = vec![fixture.root_path().to_path_buf()];
    let mut coordinator = ParseCoordinator::new();
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).expect("readable");
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(content) = std::fs::read_to_string(&path) {
                let rel = path
                    .strip_prefix(fixture.root_path())
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                if rel.starts_with(".cce") {
                    continue;
                }
                if let Ok(parsed) = coordinator.parse(&rel, &content) {
                    files.push(parsed);
                }
            }
        }
    }
    eprintln!("########## {label} ##########");
    for file in &files {
        eprintln!("FILE {} lang={:?}", file.path, file.language);
        for entity in &file.entities {
            eprintln!(
                "  ENTITY kind={:?} name={} ret={:?} params={:?} meta={:?}",
                entity.kind, entity.name, entity.return_type, entity.parameters, entity.metadata,
            );
        }
    }
    let bindings = cce_e2e_tests::type_inference_assert::collect_type_bindings(&files);
    for b in &bindings {
        eprintln!("  BINDING {b:?}");
    }
}

fn main() {
    init_minimal_logging();
    dump(
        FixtureSpec::typescript_type_inference_visibility(),
        "ts_visibility",
    );
    dump(FixtureSpec::php_type_inference_overloads(), "php_overloads");
    dump(FixtureSpec::bash_type_inference_variables(), "bash_vars");
    dump(FixtureSpec::lua_type_inference_variables(), "lua_vars");
    dump(
        FixtureSpec::kotlin_type_inference_scope_functions(),
        "kotlin_scope",
    );
}
