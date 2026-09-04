use cce_types::entity::Entity;
use cce_types::language::Language;

fn normalize_receiver(s: &str) -> String {
    let t = s.trim().trim_start_matches('*').trim();
    let simple = t.rsplit('.').next().unwrap_or(t);
    let g = if let Some(pos) = simple.find('[') {
        &simple[..pos]
    } else {
        simple
    };
    let g = if let Some(pos) = g.find('<') {
        &g[..pos]
    } else {
        g
    };
    g.trim().to_string()
}

fn extract_from_signature(sig: &str) -> Option<String> {
    // Go method shape only: `func (recv Type) Name(...)`. Plain functions
    // (`func Name(...)`) and other languages' signatures must not reach
    // here, otherwise the first parameter is misread as a receiver.
    let rest = sig.trim_start().strip_prefix("func")?.trim_start();
    if !rest.starts_with('(') {
        return None;
    }
    let start = 0;
    let end = rest.find(')')?;
    let inside = rest[start + 1..end].trim();
    if inside.is_empty() {
        return None;
    }
    let parts: Vec<&str> = inside.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }
    let type_part = if parts.len() == 1 { parts[0] } else { parts[1] };
    Some(normalize_receiver(type_part))
}

pub fn extract_receiver_type(entity: &mut Entity) {
    if entity.metadata.contains_key("receiver_type") {
        return;
    }
    if let Some(rt) = extract_from_signature(&entity.signature) {
        if !rt.is_empty() {
            entity.set_metadata("receiver_type", rt);
        }
    }
}

pub fn extract_receiver_for_entities(entities: &mut [Entity], language: &Language) {
    // The signature heuristic only understands Go method receivers.
    // Running it elsewhere misrecords the first parameter as a receiver.
    if *language != Language::Go {
        return;
    }
    for entity in entities.iter_mut() {
        if entity.kind.is_function_like() {
            extract_receiver_type(entity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::entity::{Entity, EntityKind};

    #[test]
    fn extract_simple() {
        let mut e = Entity {
            signature: "func (s MyStruct) Method()".to_string(),
            kind: EntityKind::Method,
            ..Default::default()
        };
        extract_receiver_type(&mut e);
        assert_eq!(e.metadata.get("receiver_type").unwrap(), "MyStruct");
    }

    #[test]
    fn extract_pointer() {
        let mut e = Entity {
            signature: "func (s *MyStruct) Method()".to_string(),
            kind: EntityKind::Method,
            ..Default::default()
        };
        extract_receiver_type(&mut e);
        assert_eq!(e.metadata.get("receiver_type").unwrap(), "MyStruct");
    }

    #[test]
    fn extract_pkg_qualified() {
        let mut e = Entity {
            signature: "func (s pkg.Foo) Method()".to_string(),
            kind: EntityKind::Method,
            ..Default::default()
        };
        extract_receiver_type(&mut e);
        assert_eq!(e.metadata.get("receiver_type").unwrap(), "Foo");
    }

    #[test]
    fn preserve_existing() {
        let mut e = Entity {
            signature: "func (s Other) Method()".to_string(),
            kind: EntityKind::Method,
            metadata: {
                let mut m = std::collections::HashMap::new();
                m.insert("receiver_type".to_string(), "Existing".to_string());
                m
            },
            ..Default::default()
        };
        extract_receiver_type(&mut e);
        assert_eq!(e.metadata.get("receiver_type").unwrap(), "Existing");
    }

    #[test]
    fn skip_plain_function_signature() {
        let mut e = Entity {
            signature: "func foo(x int)".to_string(),
            kind: EntityKind::Function,
            ..Default::default()
        };
        extract_receiver_type(&mut e);
        assert!(!e.metadata.contains_key("receiver_type"));
    }

    #[test]
    fn skip_c_style_signature() {
        // Regression: the first parameter must not be misread as a receiver.
        let mut e = Entity {
            signature: "int add(int a, int b)".to_string(),
            kind: EntityKind::Method,
            ..Default::default()
        };
        extract_receiver_type(&mut e);
        assert!(!e.metadata.contains_key("receiver_type"));
    }

    #[test]
    fn for_entities_only_runs_for_go() {
        let mut e = Entity {
            signature: "int add(int a, int b)".to_string(),
            kind: EntityKind::Method,
            ..Default::default()
        };
        extract_receiver_for_entities(std::slice::from_mut(&mut e), &Language::Cpp);
        assert!(!e.metadata.contains_key("receiver_type"));

        let mut g = Entity {
            signature: "func (s MyStruct) Method()".to_string(),
            kind: EntityKind::Method,
            ..Default::default()
        };
        extract_receiver_for_entities(std::slice::from_mut(&mut g), &Language::Go);
        assert_eq!(g.metadata.get("receiver_type").unwrap(), "MyStruct");
    }
}
