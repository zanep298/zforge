//! Project-type detection for `zforge init`. Inspects the project root for
//! marker files (Cargo.toml, go.mod, pubspec.yaml, etc.) and resolves the
//! project's language, default test command, and best-effort display name.

use std::path::Path;

pub(crate) struct DetectedProject {
    pub(crate) language: String,
    pub(crate) test_command: String,
    pub(crate) project_name: String,
}

pub(crate) fn detect_project(root: &Path) -> DetectedProject {
    if root.join("Cargo.toml").exists() {
        let name = read_cargo_name(root).unwrap_or_default();
        return DetectedProject {
            language: "rust".into(),
            test_command: "cargo test".into(),
            project_name: name,
        };
    }
    if root.join("go.mod").exists() {
        let name = read_go_module_name(root).unwrap_or_default();
        return DetectedProject {
            language: "go".into(),
            test_command: "go test ./...".into(),
            project_name: name,
        };
    }
    if root.join("pubspec.yaml").exists() {
        let name = read_pubspec_name(root).unwrap_or_default();
        return DetectedProject {
            language: "flutter".into(),
            test_command: "flutter test".into(),
            project_name: name,
        };
    }
    if root.join("package.json").exists() {
        let name = read_package_json_name(root).unwrap_or_default();
        return DetectedProject {
            language: "typescript".into(),
            test_command: "npm test".into(),
            project_name: name,
        };
    }
    if root.join("pyproject.toml").exists() || root.join("setup.py").exists() {
        return DetectedProject {
            language: "python".into(),
            test_command: "pytest".into(),
            project_name: String::new(),
        };
    }
    if let Some(d) = detect_android_project(root) {
        return d;
    }
    if let Some(d) = detect_ios_project(root) {
        return d;
    }
    DetectedProject {
        language: "rust".into(),
        test_command: "cargo test".into(),
        project_name: String::new(),
    }
}

fn detect_android_project(root: &Path) -> Option<DetectedProject> {
    // Android project: has settings.gradle.kts (or .gradle) and an app/ subdirectory
    let has_settings =
        root.join("settings.gradle.kts").exists() || root.join("settings.gradle").exists();
    let has_app_dir = root.join("app").is_dir();
    if !(has_settings && has_app_dir) {
        return None;
    }
    let name = read_android_app_name(root).unwrap_or_default();
    Some(DetectedProject {
        language: "android".into(),
        test_command: "./gradlew test".into(),
        project_name: name,
    })
}

fn read_android_app_name(root: &Path) -> Option<String> {
    let settings = root.join("settings.gradle.kts");
    let content = std::fs::read_to_string(&settings)
        .or_else(|_| std::fs::read_to_string(root.join("settings.gradle")))
        .ok()?;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("rootProject.name") {
            if let Some((_, val)) = line.split_once('=') {
                return Some(
                    val.trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .trim_matches(',')
                        .to_string(),
                );
            }
        }
    }
    None
}

fn detect_ios_project(root: &Path) -> Option<DetectedProject> {
    // Swift Package Manager project
    if root.join("Package.swift").exists() {
        let name = read_swift_package_name(root).unwrap_or_default();
        return Some(DetectedProject {
            language: "ios".into(),
            test_command: "swift test".into(),
            project_name: name,
        });
    }
    // Xcode project / workspace
    if let Ok(entries) = std::fs::read_dir(root) {
        let mut xcodeproj: Option<String> = None;
        let mut xcworkspace: Option<String> = None;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".xcworkspace") && !name.contains(".xcodeproj") {
                xcworkspace = Some(name.trim_end_matches(".xcworkspace").to_string());
            } else if name.ends_with(".xcodeproj") {
                xcodeproj = Some(name.trim_end_matches(".xcodeproj").to_string());
            }
        }
        let scheme = xcworkspace.or(xcodeproj);
        if let Some(scheme_name) = scheme {
            let test_command = format!(
                "xcodebuild test -scheme {scheme_name} -destination 'platform=iOS Simulator,name=iPhone 16'"
            );
            return Some(DetectedProject {
                language: "ios".into(),
                test_command,
                project_name: scheme_name,
            });
        }
    }
    None
}

fn read_swift_package_name(root: &Path) -> Option<String> {
    let content = std::fs::read_to_string(root.join("Package.swift")).ok()?;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("name:") {
            return Some(
                line.trim_start_matches("name:")
                    .trim()
                    .trim_matches('"')
                    .trim_matches(',')
                    .to_string(),
            );
        }
    }
    None
}

fn read_cargo_name(root: &Path) -> Option<String> {
    let content = std::fs::read_to_string(root.join("Cargo.toml")).ok()?;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("name") {
            if let Some((_, val)) = line.split_once('=') {
                return Some(val.trim().trim_matches('"').to_string());
            }
        }
    }
    None
}

fn read_go_module_name(root: &Path) -> Option<String> {
    let content = std::fs::read_to_string(root.join("go.mod")).ok()?;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("module ") {
            let module = line.trim_start_matches("module ").trim();
            // take last path segment as the name
            return Some(module.rsplit('/').next().unwrap_or(module).to_string());
        }
    }
    None
}

fn read_package_json_name(root: &Path) -> Option<String> {
    let content = std::fs::read_to_string(root.join("package.json")).ok()?;
    // simple extract — avoid pulling in serde_json just for this
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("\"name\"") {
            if let Some((_, val)) = line.split_once(':') {
                return Some(
                    val.trim()
                        .trim_matches(',')
                        .trim()
                        .trim_matches('"')
                        .to_string(),
                );
            }
        }
    }
    None
}

fn read_pubspec_name(root: &Path) -> Option<String> {
    let content = std::fs::read_to_string(root.join("pubspec.yaml")).ok()?;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("name:") {
            return Some(
                line.trim_start_matches("name:")
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string(),
            );
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn cargo_toml_detects_rust() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\n",
        )
        .unwrap();
        let d = detect_project(tmp.path());
        assert_eq!(d.language, "rust");
        assert_eq!(d.test_command, "cargo test");
        assert_eq!(d.project_name, "demo");
    }

    #[test]
    fn go_mod_detects_go() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("go.mod"),
            "module github.com/example/demo-go\n\ngo 1.22\n",
        )
        .unwrap();
        let d = detect_project(tmp.path());
        assert_eq!(d.language, "go");
        assert_eq!(d.project_name, "demo-go");
    }

    #[test]
    fn package_json_detects_typescript() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("package.json"),
            "{\n  \"name\": \"demo-ts\",\n  \"version\": \"1.0.0\"\n}\n",
        )
        .unwrap();
        let d = detect_project(tmp.path());
        assert_eq!(d.language, "typescript");
        assert_eq!(d.project_name, "demo-ts");
    }

    #[test]
    fn pubspec_detects_flutter() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("pubspec.yaml"), "name: demo_app\n").unwrap();
        let d = detect_project(tmp.path());
        assert_eq!(d.language, "flutter");
    }

    #[test]
    fn pyproject_detects_python() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("pyproject.toml"), "[project]\nname=\"x\"\n").unwrap();
        let d = detect_project(tmp.path());
        assert_eq!(d.language, "python");
        assert_eq!(d.test_command, "pytest");
    }

    #[test]
    fn android_settings_plus_app_dir_detects_android() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("settings.gradle.kts"),
            "rootProject.name = \"demo-android\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(tmp.path().join("app")).unwrap();
        let d = detect_project(tmp.path());
        assert_eq!(d.language, "android");
        assert_eq!(d.project_name, "demo-android");
    }

    #[test]
    fn package_swift_detects_ios() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("Package.swift"),
            "// swift-tools-version:5.9\nname: \"DemoLib\",\n",
        )
        .unwrap();
        let d = detect_project(tmp.path());
        assert_eq!(d.language, "ios");
        assert_eq!(d.test_command, "swift test");
    }

    #[test]
    fn empty_dir_falls_back_to_rust() {
        let tmp = TempDir::new().unwrap();
        let d = detect_project(tmp.path());
        assert_eq!(d.language, "rust");
        assert_eq!(d.project_name, "");
    }
}
