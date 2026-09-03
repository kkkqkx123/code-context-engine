use cce_types::entity::Entity;

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
    let start = sig.find('(')?;
    let end = sig[start..].find(')')? + start;
    let inside = sig[start + 1..end].trim();
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

pub fn extract_receiver_for_entities(entities: &mut [Entity]) {
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
}
