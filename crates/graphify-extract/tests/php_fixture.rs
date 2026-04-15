use graphify_core::types::{EdgeKind, Language, NodeKind};
use graphify_extract::{
    discover_files_with_psr4, resolver::ModuleResolver, ExtractionResult, LanguageExtractor,
    PhpExtractor,
};
use std::path::PathBuf;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap() // crates/
        .parent()
        .unwrap() // workspace root
        .join("tests/fixtures/php_project")
}

fn psr4_mappings() -> Vec<(String, String)> {
    let mut resolver = ModuleResolver::new(&fixture_root());
    resolver.load_composer_json(&fixture_root().join("composer.json"));
    resolver.psr4_mappings().to_vec()
}

#[test]
fn discover_php_fixture_finds_four_source_files() {
    let mappings = psr4_mappings();
    let files = discover_files_with_psr4(&fixture_root(), &[Language::Php], "", &[], &mappings);
    let names: Vec<&str> = files.iter().map(|f| f.module_name.as_str()).collect();
    assert_eq!(
        files.len(),
        4,
        "expected 4 PHP source files (LlmTest.php excluded), got {:?}",
        names
    );
}

#[test]
fn discover_php_fixture_applies_psr4_to_module_names() {
    let mappings = psr4_mappings();
    let files = discover_files_with_psr4(&fixture_root(), &[Language::Php], "", &[], &mappings);
    let names: Vec<&str> = files.iter().map(|f| f.module_name.as_str()).collect();
    assert!(
        names.contains(&"App.Main"),
        "expected App.Main; got {:?}",
        names
    );
    assert!(
        names.contains(&"App.Services.Llm"),
        "expected App.Services.Llm; got {:?}",
        names
    );
    assert!(
        names.contains(&"App.Models.User"),
        "expected App.Models.User; got {:?}",
        names
    );
    assert!(
        names.contains(&"App.Controllers.HomeController"),
        "expected App.Controllers.HomeController; got {:?}",
        names
    );
    assert!(
        !names.iter().any(|n| n.contains("LlmTest")),
        "LlmTest.php must be excluded"
    );
}

#[test]
fn home_controller_imports_resolve_to_local_modules() {
    let mappings = psr4_mappings();
    let files = discover_files_with_psr4(&fixture_root(), &[Language::Php], "", &[], &mappings);

    let mut resolver = ModuleResolver::new(&fixture_root());
    for f in &files {
        resolver.register_module(&f.module_name);
    }

    let ctrl = files
        .iter()
        .find(|f| f.module_name == "App.Controllers.HomeController")
        .expect("HomeController discovered");

    let source = std::fs::read(&ctrl.path).expect("read fixture");
    let extractor = PhpExtractor::new();
    let result: ExtractionResult = extractor.extract_file(&ctrl.path, &source, &ctrl.module_name);

    let calls_targets: Vec<&str> = result
        .edges
        .iter()
        .filter(|e| e.2.kind == EdgeKind::Calls)
        .map(|e| e.1.as_str())
        .collect();
    assert!(
        calls_targets.contains(&"App.Services.Llm"),
        "use App\\Services\\Llm should Calls-target App.Services.Llm; got {:?}",
        calls_targets
    );
    assert!(
        calls_targets.contains(&"App.Models.User"),
        "use App\\Models\\User should Calls-target App.Models.User; got {:?}",
        calls_targets
    );

    for raw_target in ["App.Services.Llm", "App.Models.User"] {
        let (resolved, is_local, _conf) = resolver.resolve(raw_target, &ctrl.module_name, false);
        assert_eq!(
            resolved, raw_target,
            "resolver must be identity for dot-form"
        );
        assert!(is_local, "{} must resolve to local", raw_target);
    }
}

#[test]
fn home_controller_extracts_class_and_method_nodes() {
    let mappings = psr4_mappings();
    let files = discover_files_with_psr4(&fixture_root(), &[Language::Php], "", &[], &mappings);

    let ctrl = files
        .iter()
        .find(|f| f.module_name == "App.Controllers.HomeController")
        .expect("HomeController");

    let source = std::fs::read(&ctrl.path).expect("read fixture");
    let extractor = PhpExtractor::new();
    let result = extractor.extract_file(&ctrl.path, &source, &ctrl.module_name);

    let class = result
        .nodes
        .iter()
        .find(|n| {
            n.kind == NodeKind::Class && n.id == "App.Controllers.HomeController.HomeController"
        })
        .expect("class node");
    assert_eq!(class.language, Language::Php);

    let method = result
        .nodes
        .iter()
        .find(|n| {
            n.kind == NodeKind::Method
                && n.id == "App.Controllers.HomeController.HomeController.handle"
        })
        .expect("handle method node");
    assert_eq!(method.language, Language::Php);
}
