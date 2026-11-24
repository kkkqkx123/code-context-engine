use cce_e2e_tests::TestFixture;
use cce_e2e_tests::init_minimal_logging;

fn main() {
    init_minimal_logging();
    let fixture = TestFixture::python_type_inference_discriminated_union().expect("load");
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
    for f in &files {
        for e in &f.entities {
            if matches!(
                e.kind,
                cce_types::entity::EntityKind::Field | cce_types::entity::EntityKind::Property
            ) {
                eprintln!(
                    "FIELD {} params-of-enclosing=? meta={:?} span={:?} src={:?}",
                    e.name,
                    e.metadata,
                    e.span,
                    f.source
                        .get(e.span.start_byte..e.span.end_byte)
                        .unwrap_or("?")
                );
            }
            if e.name == "__init__" {
                eprintln!("INIT params={:?} ret={:?}", e.parameters, e.return_type);
            }
        }
    }
}
