//! App compatibility and dependency resolution.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use dnd_plugin::semver::Version;
use dnd_plugin::*;
use serde_json::json;

fn plugin(id: &str, kind: &str, version: &str, app: &str, deps: &[(&str, &str)]) -> Manifest {
    let deps: serde_json::Map<_, _> = deps
        .iter()
        .map(|(id, range)| ((*id).to_owned(), json!(range)))
        .collect();
    serde_json::from_value(json!({
        "format": 1, "id": id, "name": id, "version": version, "kind": kind,
        "authors": ["t"], "license": "MIT", "app_compat": app, "dependencies": deps
    }))
    .unwrap()
}

fn v(s: &str) -> Version {
    Version::parse(s).unwrap()
}

#[test]
fn dependencies_can_be_checked_against_authoritative_dto_versions() {
    let candidate = plugin("addon", "content", "1.0.0", "*", &[("base", "^1.0")]);
    assert_eq!(
        check_dependency_versions(&candidate, [])
            .unwrap_err()
            .code(),
        "missing_dependency"
    );
    for version in ["2.0.0", "broken"] {
        assert_eq!(
            check_dependency_versions(&candidate, [("base", version)])
                .unwrap_err()
                .code(),
            "dependency_version_mismatch"
        );
    }
    assert!(check_dependency_versions(&candidate, [("base", "1.8.0")]).is_ok());
    let self_dependent = plugin("base", "content", "1.0.0", "*", &[("base", "*")]);
    assert!(check_dependency_versions(&self_dependent, [("base", "1.0.0")]).is_err());
}

#[test]
fn app_compat_ranges() {
    let caret = plugin("p", "content", "1.0.0", "^0.1", &[]);
    assert!(check_app_compat(&caret, &v("0.1.0")).is_ok());
    assert!(check_app_compat(&caret, &v("0.1.9")).is_ok());
    assert_eq!(
        check_app_compat(&caret, &v("0.2.0")).unwrap_err().code(),
        "incompatible_app"
    );
    assert!(check_app_compat(&caret, &v("0.0.9")).is_err());
    // A beta build counts as its release.
    assert!(check_app_compat(&caret, &v("0.1.0-beta.2")).is_ok());

    let range = plugin("p", "content", "1.0.0", ">=0.1.0, <2", &[]);
    assert!(check_app_compat(&range, &v("1.5.0")).is_ok());
    assert!(check_app_compat(&range, &v("2.0.0")).is_err());

    let any = plugin("p", "content", "1.0.0", "*", &[]);
    assert!(check_app_compat(&any, &v("9.9.9")).is_ok());
}

#[test]
fn install_checks() {
    let app = v("0.1.0");
    let system = plugin("srd5", "system", "1.2.0", "^0.1", &[]);
    let content = plugin("monsters", "content", "1.0.0", "^0.1", &[("srd5", "^1.1")]);

    assert!(check_install(&system, &[], &app).is_ok());
    assert!(check_install(&content, &[&system], &app).is_ok());
    assert_eq!(
        check_install(&content, &[], &app).unwrap_err().code(),
        "missing_dependency"
    );
    let old = plugin("srd5", "system", "1.0.0", "^0.1", &[]);
    assert_eq!(
        check_install(&content, &[&old], &app).unwrap_err().code(),
        "dependency_version_mismatch"
    );

    // A second rule system is refused; updating the same one is fine.
    let d6 = plugin("generic-d6", "system", "1.0.0", "^0.1", &[]);
    let err = check_install(&d6, &[&system], &app).unwrap_err();
    assert_eq!(err.code(), "multiple_systems");
    let update = plugin("srd5", "system", "2.0.0", "^0.1", &[]);
    assert!(check_install(&update, &[&system, &content], &app).is_ok());

    let future = plugin("x", "ui", "1.0.0", "^0.5", &[]);
    assert_eq!(
        check_install(&future, &[], &app).unwrap_err().code(),
        "incompatible_app"
    );
}

#[test]
fn resolution_deactivates_and_cascades() {
    let app = v("0.1.0");
    let system = plugin("srd5", "system", "2.0.0", "^0.1", &[]);
    // Wants srd5 ^1 but 2.0.0 is installed (e.g. after an update).
    let monsters = plugin("monsters", "content", "1.0.0", "^0.1", &[("srd5", "^1")]);
    // Depends on monsters → deactivated in cascade.
    let lairs = plugin("lairs", "content", "1.0.0", "^0.1", &[("monsters", "^1")]);
    let theme = plugin("dark", "theme", "1.0.0", "^0.1", &[]);
    let future = plugin("future", "ui", "1.0.0", "^1", &[]);
    let orphan = plugin("orphan", "content", "1.0.0", "^0.1", &[("gone", "*")]);

    let r = resolve([&system, &monsters, &lairs, &theme, &future, &orphan], &app);
    assert_eq!(r.active, vec!["srd5", "dark"]);
    let reasons: Vec<(&str, &str)> = r
        .deactivated
        .iter()
        .map(|d| (d.id.as_str(), d.reason.code()))
        .collect();
    assert_eq!(
        reasons,
        vec![
            ("monsters", "dependency_version_mismatch"),
            ("lairs", "dependency_inactive"),
            ("future", "incompatible_app"),
            ("orphan", "missing_dependency"),
        ]
    );
}

#[test]
fn resolution_handles_order_cycles_and_duplicates() {
    let app = v("0.1.0");
    // Dependency listed after its dependent.
    let a = plugin("a", "content", "1.0.0", "^0.1", &[("b", "^1")]);
    let b = plugin("b", "content", "1.4.0", "^0.1", &[]);
    let r = resolve([&a, &b], &app);
    assert_eq!(r.active, vec!["a", "b"]);
    assert!(r.deactivated.is_empty());

    // A cycle of valid plugins stays; a cycle with one bad member goes entirely.
    let x = plugin("x", "content", "1.0.0", "^0.1", &[("y", "^1")]);
    let y = plugin("y", "content", "1.0.0", "^0.1", &[("x", "^1")]);
    assert_eq!(resolve([&x, &y], &app).active, vec!["x", "y"]);
    let y_future = plugin("y", "content", "1.0.0", "^9", &[("x", "^1")]);
    let r = resolve([&x, &y_future], &app);
    assert!(r.active.is_empty());
    assert_eq!(r.deactivated[0].reason.code(), "dependency_inactive");
    assert_eq!(r.deactivated[1].reason.code(), "incompatible_app");

    // Pre-release dependencies match only ranges that ask for them.
    let pre = plugin("pre", "content", "2.0.0-rc.1", "^0.1", &[]);
    let wants = plugin("wants", "content", "1.0.0", "^0.1", &[("pre", "^2")]);
    let r = resolve([&pre, &wants], &app);
    assert_eq!(r.active, vec!["pre"]);

    let dup = plugin("b", "content", "1.0.0", "^0.1", &[]);
    let r = resolve([&b, &dup], &app);
    assert_eq!(r.active, vec!["b"]);
    assert_eq!(r.deactivated[0].reason.code(), "duplicate_plugin");
}
