// Scala Standard Library Detector
// Handles detection of Scala standard library entities

pub struct ScalaStdlibDetector;

mod detector;
mod packages;
mod types;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_scala_package() {
        assert!(ScalaStdlibDetector::is_scala_package("scala"));
        assert!(ScalaStdlibDetector::is_scala_package("scala.collection"));
        assert!(ScalaStdlibDetector::is_scala_package("scala.concurrent"));
        assert!(ScalaStdlibDetector::is_scala_package("cats.effect"));
        assert!(ScalaStdlibDetector::is_scala_package("zio"));
        assert!(!ScalaStdlibDetector::is_scala_package("my.package"));
    }

    #[test]
    fn test_is_builtin_type() {
        assert!(ScalaStdlibDetector::is_builtin_type("String"));
        assert!(ScalaStdlibDetector::is_builtin_type("Int"));
        assert!(ScalaStdlibDetector::is_builtin_type("List"));
        assert!(ScalaStdlibDetector::is_builtin_type("Option"));
        assert!(ScalaStdlibDetector::is_builtin_type("Future"));
        assert!(ScalaStdlibDetector::is_builtin_type("IO"));
        assert!(!ScalaStdlibDetector::is_builtin_type("MyClass"));
    }

    #[test]
    fn test_is_builtin_function() {
        assert!(ScalaStdlibDetector::is_builtin_function("println"));
        assert!(ScalaStdlibDetector::is_builtin_function("List"));
        assert!(ScalaStdlibDetector::is_builtin_function("Option"));
        assert!(ScalaStdlibDetector::is_builtin_function("pure"));
        assert!(!ScalaStdlibDetector::is_builtin_function("myFunction"));
    }

    #[test]
    fn test_is_common_method() {
        assert!(ScalaStdlibDetector::is_common_method("map"));
        assert!(ScalaStdlibDetector::is_common_method("flatMap"));
        assert!(ScalaStdlibDetector::is_common_method("filter"));
        assert!(ScalaStdlibDetector::is_common_method("foldLeft"));
        assert!(!ScalaStdlibDetector::is_common_method("myMethod"));
    }

    #[test]
    fn test_is_scala_path() {
        assert!(ScalaStdlibDetector::is_scala_path("scala"));
        assert!(ScalaStdlibDetector::is_scala_path(
            "scala.collection.immutable.List"
        ));
        assert!(ScalaStdlibDetector::is_scala_path("cats.effect.IO"));
        assert!(ScalaStdlibDetector::is_scala_path("zio.ZIO"));
        assert!(ScalaStdlibDetector::is_scala_path("org.http4s.Request"));
        assert!(!ScalaStdlibDetector::is_scala_path("my.package.MyClass"));
    }

    #[test]
    fn test_is_stdlib_call() {
        assert!(ScalaStdlibDetector::is_stdlib_call("String"));
        assert!(ScalaStdlibDetector::is_stdlib_call("Int"));
        assert!(ScalaStdlibDetector::is_stdlib_call("List"));
        assert!(ScalaStdlibDetector::is_stdlib_call("Option"));
        assert!(ScalaStdlibDetector::is_stdlib_call("Future"));
        assert!(ScalaStdlibDetector::is_stdlib_call("IO"));

        assert!(ScalaStdlibDetector::is_stdlib_call("println"));
        assert!(ScalaStdlibDetector::is_stdlib_call("List"));
        assert!(ScalaStdlibDetector::is_stdlib_call("Option"));
        assert!(ScalaStdlibDetector::is_stdlib_call("pure"));

        assert!(ScalaStdlibDetector::is_stdlib_call("map"));
        assert!(ScalaStdlibDetector::is_stdlib_call("flatMap"));
        assert!(ScalaStdlibDetector::is_stdlib_call("filter"));
        assert!(ScalaStdlibDetector::is_stdlib_call("foldLeft"));

        assert!(ScalaStdlibDetector::is_stdlib_call(
            "scala.collection.immutable.List"
        ));
        assert!(ScalaStdlibDetector::is_stdlib_call("cats.effect.IO"));
        assert!(ScalaStdlibDetector::is_stdlib_call("zio.ZIO"));
        assert!(ScalaStdlibDetector::is_stdlib_call("org.http4s.Request"));

        assert!(ScalaStdlibDetector::is_stdlib_call("list.map"));
        assert!(ScalaStdlibDetector::is_stdlib_call("option.getOrElse"));
        assert!(ScalaStdlibDetector::is_stdlib_call("future.flatMap"));

        assert!(!ScalaStdlibDetector::is_stdlib_call("MyClass"));
        assert!(!ScalaStdlibDetector::is_stdlib_call("myFunction"));
        assert!(!ScalaStdlibDetector::is_stdlib_call(
            "my.package.myFunction"
        ));
    }
}
