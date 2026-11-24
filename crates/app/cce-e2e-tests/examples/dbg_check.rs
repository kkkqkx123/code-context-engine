use cce_e2e_tests::TestFixture;
use cce_e2e_tests::init_minimal_logging;

fn main() {
    init_minimal_logging();
    let which = std::env::args()
        .nth(1)
        .unwrap_or("scala_overloads".to_string());
    let fixture = match which.as_str() {
        "csharp_lambda" => TestFixture::csharp_type_inference_lambda().expect("load"),
        "python_disc" => TestFixture::python_type_inference_discriminated_union().expect("load"),
        "kotlin_cross" | "kotlin_meta" => {
            TestFixture::kotlin_type_inference_cross_file().expect("load")
        }
        "java_var" => TestFixture::java_type_inference_var_inference().expect("load"),
        "scala_for" | "scala_meta" => {
            TestFixture::scala_type_inference_for_comprehension().expect("load")
        }
        _ => TestFixture::scala_type_inference_overloads().expect("load"),
    };
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
                if let Ok(parsed) = coordinator.parse(&rel, &content) {
                    files.push(parsed);
                }
            }
        }
    }
    let bindings = cce_e2e_tests::type_inference_assert::collect_type_bindings(&files);
    if which == "kotlin_meta" || which == "scala_meta" {
        for file in &files {
            for entity in &file.entities {
                eprintln!(
                    "ENTITY file={} kind={:?} name={} meta={:?} span={:?} src={:?}",
                    file.path,
                    entity.kind,
                    entity.name,
                    entity.metadata,
                    entity.span,
                    file.source
                        .get(entity.span.start_byte..entity.span.end_byte)
                        .unwrap_or("?"),
                );
            }
        }
    }
    for b in &bindings {
        eprintln!("BINDING {b:?}");
    }
}
