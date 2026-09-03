use super::ScalaStdlibDetector;

impl ScalaStdlibDetector {
    pub fn is_scala_package(package: &str) -> bool {
        Self::SCALA_PACKAGES.contains(&package)
    }

    pub fn is_builtin_type(name: &str) -> bool {
        Self::BUILTIN_TYPES.contains(&name)
    }

    pub fn is_builtin_function(name: &str) -> bool {
        Self::BUILTIN_FUNCTIONS.contains(&name)
    }

    pub fn is_common_method(name: &str) -> bool {
        Self::COMMON_METHODS.contains(&name)
    }

    pub fn is_scala_path(path: &str) -> bool {
        for &package in Self::SCALA_PACKAGES {
            if path == package || path.starts_with(&format!("{}.", package)) {
                return true;
            }
        }

        let first_component = path.split('.').next().unwrap_or("");
        if first_component == "scala"
            || first_component == "cats"
            || first_component == "zio"
            || first_component == "akka"
            || first_component == "play"
            || first_component == "org"
        {
            if first_component == "org" {
                let parts: Vec<&str> = path.split('.').collect();
                if parts.len() >= 2 {
                    let second = parts[1];
                    if second == "http4s"
                        || second == "scalatest"
                        || second == "specs2"
                        || second == "scalacheck"
                        || second == "apache"
                    {
                        return true;
                    }
                }
                return false;
            }
            return true;
        }

        false
    }

    pub fn is_stdlib_call(call_name: &str) -> bool {
        if Self::is_builtin_type(call_name) {
            return true;
        }

        if Self::is_builtin_function(call_name) {
            return true;
        }

        if Self::is_common_method(call_name) {
            return true;
        }

        if call_name.contains('.') {
            if Self::is_scala_path(call_name) {
                return true;
            }

            let parts: Vec<&str> = call_name.split('.').collect();
            if parts.len() >= 2 {
                let package = parts[0];
                if Self::is_scala_package(package) {
                    return true;
                }

                let receiver = parts[0];
                let _method = parts[1];

                if Self::is_builtin_type(receiver) {
                    return true;
                }
                if !receiver.is_empty() {
                    let mut capitalized = receiver.chars().collect::<Vec<_>>();
                    capitalized[0] = capitalized[0].to_ascii_uppercase();
                    let capitalized: String = capitalized.into_iter().collect();
                    if Self::is_builtin_type(&capitalized) {
                        return true;
                    }
                }

                if parts.len() == 2 && Self::is_scala_package(receiver) {
                    return true;
                }
            }
        }

        false
    }

    pub fn is_stdlib_by_type(
        call_name: &str,
        relation_type: &cce_types::relation::RelationType,
    ) -> bool {
        use cce_types::relation::RelationType;

        match relation_type {
            RelationType::DirectCall
            | RelationType::InstanceMethodCall
            | RelationType::StaticMethodCall
            | RelationType::ChainedMethodCall
            | RelationType::ConstructorCall
            | RelationType::CallbackCall
            | RelationType::GenericCall => {
                if Self::is_builtin_type(call_name) {
                    return true;
                }

                if Self::is_builtin_function(call_name) {
                    return true;
                }

                if Self::is_common_method(call_name) {
                    return true;
                }

                if call_name.contains('.') {
                    if Self::is_scala_path(call_name) {
                        return true;
                    }

                    let parts: Vec<&str> = call_name.split('.').collect();
                    if parts.len() >= 2 {
                        let package = parts[0];
                        if Self::is_scala_package(package) {
                            return true;
                        }

                        let receiver = parts[0];
                        if Self::is_builtin_type(receiver) {
                            return true;
                        }

                        if parts.len() == 2 && Self::is_scala_package(receiver) {
                            return true;
                        }
                    }
                }

                false
            }

            _ => false,
        }
    }
}
