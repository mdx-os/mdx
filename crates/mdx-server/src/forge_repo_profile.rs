// Deterministic repo intelligence for Forge's external-repo path.
//
// This is intentionally small and boring: no model call, no shelling out, no
// authority opened. It gives the harness enough repo-native context to avoid
// obviously wrong defaults and to keep build artifacts from drowning review.
use mdx_core::json_string_literal;
use std::path::Path;

const MARKER_SCAN_MAX_ENTRIES: usize = 5_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ForgeRepoProfile {
    pub primary_language: &'static str,
    pub language_pack_id: &'static str,
    pub detected_language_packs: Vec<&'static str>,
    pub detected_files: Vec<&'static str>,
    pub quality_signals: Vec<&'static str>,
    pub standards_sources: Vec<&'static str>,
    pub standards_source_fingerprints: Vec<String>,
    pub standards_source_summaries: Vec<String>,
    pub review_axes: Vec<&'static str>,
    pub principal_review_gates: Vec<&'static str>,
    pub language_pack_guidance: Vec<&'static str>,
    pub semantic_intelligence: Vec<&'static str>,
    pub semantic_tool_readiness: Vec<String>,
    pub toolchain_readiness: Vec<String>,
    pub proof_plan_status: &'static str,
    pub proof_plan_next_action: String,
    pub proof_plan_summary: String,
    pub suggested_checks: Vec<String>,
    pub artifact_patterns: Vec<&'static str>,
}

pub(crate) fn profile_repo(root: &Path) -> ForgeRepoProfile {
    let mut detected = Vec::new();

    if has_any_extension(root, "xcworkspace", &mut detected)
        || has_any_extension(root, "xcodeproj", &mut detected)
    {
        return ForgeRepoProfile {
            primary_language: "swift-ios",
            language_pack_id: "ios-xcode",
            detected_language_packs: detected_language_packs(root),
            detected_files: detected,
            quality_signals: quality_signals(root, "ios-xcode"),
            standards_sources: standards_sources(root),
            standards_source_fingerprints: standards_source_fingerprints(root),
            standards_source_summaries: standards_source_summaries(root),
            review_axes: review_axes("ios-xcode"),
            principal_review_gates: principal_review_gates("ios-xcode"),
            language_pack_guidance: language_pack_guidance("ios-xcode"),
            semantic_intelligence: semantic_intelligence("ios-xcode"),
            semantic_tool_readiness: semantic_tool_readiness("ios-xcode"),
            toolchain_readiness: toolchain_readiness(root, "ios-xcode"),
            proof_plan_status: proof_plan_status(root, "ios-xcode", &["xcodebuild test"]),
            proof_plan_next_action: proof_plan_next_action(root, "ios-xcode", &["xcodebuild test"]),
            proof_plan_summary: proof_plan_summary("ios-xcode", &["xcodebuild test"]),
            suggested_checks: command_list(&["xcodebuild test"]),
            artifact_patterns: vec!["DerivedData/**", ".build/**"],
        };
    }
    if has_marker(root, "Package.swift", &mut detected) {
        let suggested_checks = swift_suggested_checks(root);
        return ForgeRepoProfile {
            primary_language: "swift",
            language_pack_id: "swift-spm",
            detected_language_packs: detected_language_packs(root),
            detected_files: detected,
            quality_signals: quality_signals(root, "swift-spm"),
            standards_sources: standards_sources(root),
            standards_source_fingerprints: standards_source_fingerprints(root),
            standards_source_summaries: standards_source_summaries(root),
            review_axes: review_axes("swift-spm"),
            principal_review_gates: principal_review_gates("swift-spm"),
            language_pack_guidance: language_pack_guidance("swift-spm"),
            semantic_intelligence: semantic_intelligence("swift-spm"),
            semantic_tool_readiness: semantic_tool_readiness("swift-spm"),
            toolchain_readiness: toolchain_readiness(root, "swift-spm"),
            proof_plan_status: proof_plan_status(root, "swift-spm", &suggested_checks),
            proof_plan_next_action: proof_plan_next_action(root, "swift-spm", &suggested_checks),
            proof_plan_summary: proof_plan_summary("swift-spm", &suggested_checks),
            suggested_checks,
            artifact_patterns: vec![".build/**", "DerivedData/**"],
        };
    }
    if has_marker(root, "pom.xml", &mut detected) {
        return ForgeRepoProfile {
            primary_language: "java",
            language_pack_id: "java-maven",
            detected_language_packs: detected_language_packs(root),
            detected_files: detected,
            quality_signals: quality_signals(root, "java-maven"),
            standards_sources: standards_sources(root),
            standards_source_fingerprints: standards_source_fingerprints(root),
            standards_source_summaries: standards_source_summaries(root),
            review_axes: review_axes("java-maven"),
            principal_review_gates: principal_review_gates("java-maven"),
            language_pack_guidance: language_pack_guidance("java-maven"),
            semantic_intelligence: semantic_intelligence("java-maven"),
            semantic_tool_readiness: semantic_tool_readiness("java-maven"),
            toolchain_readiness: toolchain_readiness(root, "java-maven"),
            proof_plan_status: proof_plan_status(
                root,
                "java-maven",
                &["mvn -q -DskipTests dependency:go-offline", "mvn test"],
            ),
            proof_plan_next_action: proof_plan_next_action(
                root,
                "java-maven",
                &["mvn -q -DskipTests dependency:go-offline", "mvn test"],
            ),
            proof_plan_summary: proof_plan_summary(
                "java-maven",
                &["mvn -q -DskipTests dependency:go-offline", "mvn test"],
            ),
            suggested_checks: command_list(&[
                "mvn -q -DskipTests dependency:go-offline",
                "mvn test",
            ]),
            artifact_patterns: vec!["target/**"],
        };
    }
    let has_gradle_marker = has_marker(root, "settings.gradle", &mut detected)
        || has_marker(root, "settings.gradle.kts", &mut detected)
        || has_marker(root, "build.gradle", &mut detected)
        || has_marker(root, "build.gradle.kts", &mut detected);
    if has_gradle_marker && is_android_gradle_repo(root) {
        return ForgeRepoProfile {
            primary_language: "android-kotlin-java",
            language_pack_id: "android-gradle",
            detected_language_packs: detected_language_packs(root),
            detected_files: detected,
            quality_signals: quality_signals(root, "android-gradle"),
            standards_sources: standards_sources(root),
            standards_source_fingerprints: standards_source_fingerprints(root),
            standards_source_summaries: standards_source_summaries(root),
            review_axes: review_axes("android-gradle"),
            principal_review_gates: principal_review_gates("android-gradle"),
            language_pack_guidance: language_pack_guidance("android-gradle"),
            semantic_intelligence: semantic_intelligence("android-gradle"),
            semantic_tool_readiness: semantic_tool_readiness("android-gradle"),
            toolchain_readiness: toolchain_readiness(root, "android-gradle"),
            proof_plan_status: proof_plan_status(
                root,
                "android-gradle",
                &["./gradlew dependencies", "./gradlew testDebugUnitTest"],
            ),
            proof_plan_next_action: proof_plan_next_action(
                root,
                "android-gradle",
                &["./gradlew dependencies", "./gradlew testDebugUnitTest"],
            ),
            proof_plan_summary: proof_plan_summary(
                "android-gradle",
                &["./gradlew dependencies", "./gradlew testDebugUnitTest"],
            ),
            suggested_checks: command_list(&[
                "./gradlew dependencies",
                "./gradlew testDebugUnitTest",
            ]),
            artifact_patterns: vec!["build/**", ".gradle/**"],
        };
    }
    if has_gradle_marker {
        return ForgeRepoProfile {
            primary_language: "kotlin-java",
            language_pack_id: "gradle-jvm",
            detected_language_packs: detected_language_packs(root),
            detected_files: detected,
            quality_signals: quality_signals(root, "gradle-jvm"),
            standards_sources: standards_sources(root),
            standards_source_fingerprints: standards_source_fingerprints(root),
            standards_source_summaries: standards_source_summaries(root),
            review_axes: review_axes("gradle-jvm"),
            principal_review_gates: principal_review_gates("gradle-jvm"),
            language_pack_guidance: language_pack_guidance("gradle-jvm"),
            semantic_intelligence: semantic_intelligence("gradle-jvm"),
            semantic_tool_readiness: semantic_tool_readiness("gradle-jvm"),
            toolchain_readiness: toolchain_readiness(root, "gradle-jvm"),
            proof_plan_status: proof_plan_status(
                root,
                "gradle-jvm",
                &["./gradlew dependencies", "./gradlew test"],
            ),
            proof_plan_next_action: proof_plan_next_action(
                root,
                "gradle-jvm",
                &["./gradlew dependencies", "./gradlew test"],
            ),
            proof_plan_summary: proof_plan_summary(
                "gradle-jvm",
                &["./gradlew dependencies", "./gradlew test"],
            ),
            suggested_checks: command_list(&["./gradlew dependencies", "./gradlew test"]),
            artifact_patterns: vec!["build/**", ".gradle/**"],
        };
    }
    if has_marker(root, "global.json", &mut detected)
        || has_any_extension(root, "csproj", &mut detected)
        || has_any_extension(root, "sln", &mut detected)
    {
        return ForgeRepoProfile {
            primary_language: "dotnet",
            language_pack_id: "dotnet",
            detected_language_packs: detected_language_packs(root),
            detected_files: detected,
            quality_signals: quality_signals(root, "dotnet"),
            standards_sources: standards_sources(root),
            standards_source_fingerprints: standards_source_fingerprints(root),
            standards_source_summaries: standards_source_summaries(root),
            review_axes: review_axes("dotnet"),
            principal_review_gates: principal_review_gates("dotnet"),
            language_pack_guidance: language_pack_guidance("dotnet"),
            semantic_intelligence: semantic_intelligence("dotnet"),
            semantic_tool_readiness: semantic_tool_readiness("dotnet"),
            toolchain_readiness: toolchain_readiness(root, "dotnet"),
            proof_plan_status: proof_plan_status(
                root,
                "dotnet",
                &["dotnet restore", "dotnet test"],
            ),
            proof_plan_next_action: proof_plan_next_action(
                root,
                "dotnet",
                &["dotnet restore", "dotnet test"],
            ),
            proof_plan_summary: proof_plan_summary("dotnet", &["dotnet restore", "dotnet test"]),
            suggested_checks: command_list(&["dotnet restore", "dotnet test"]),
            artifact_patterns: vec!["bin/**", "obj/**", "TestResults/**"],
        };
    }
    if has_marker(root, "pyproject.toml", &mut detected)
        || has_marker(root, "requirements.txt", &mut detected)
        || has_marker(root, "setup.py", &mut detected)
    {
        let suggested_checks = python_suggested_checks(root);
        return ForgeRepoProfile {
            primary_language: "python",
            language_pack_id: "python",
            detected_language_packs: detected_language_packs(root),
            detected_files: detected,
            quality_signals: quality_signals(root, "python"),
            standards_sources: standards_sources(root),
            standards_source_fingerprints: standards_source_fingerprints(root),
            standards_source_summaries: standards_source_summaries(root),
            review_axes: review_axes("python"),
            principal_review_gates: principal_review_gates("python"),
            language_pack_guidance: language_pack_guidance("python"),
            semantic_intelligence: semantic_intelligence("python"),
            semantic_tool_readiness: semantic_tool_readiness("python"),
            toolchain_readiness: toolchain_readiness(root, "python"),
            proof_plan_status: proof_plan_status(root, "python", &suggested_checks),
            proof_plan_next_action: proof_plan_next_action(root, "python", &suggested_checks),
            proof_plan_summary: proof_plan_summary("python", &suggested_checks),
            suggested_checks,
            artifact_patterns: vec![".pytest_cache/**", "__pycache__/**", "dist/**", "build/**"],
        };
    }
    if has_marker(root, "package.json", &mut detected) {
        let package_text = std::fs::read_to_string(root.join("package.json")).unwrap_or_default();
        let package_manager_test =
            if package_text.contains("\"pnpm\"") || root.join("pnpm-lock.yaml").exists() {
                "pnpm test"
            } else if root.join("yarn.lock").exists() {
                "yarn test"
            } else {
                "npm test"
            };
        let test_command = node_test_command(root, &package_text, package_manager_test);
        let suggested_checks = node_suggested_checks(root, &package_text, &test_command);
        return ForgeRepoProfile {
            primary_language: "javascript-typescript",
            language_pack_id: "node",
            detected_language_packs: detected_language_packs(root),
            detected_files: detected,
            quality_signals: quality_signals(root, "node"),
            standards_sources: standards_sources(root),
            standards_source_fingerprints: standards_source_fingerprints(root),
            standards_source_summaries: standards_source_summaries(root),
            review_axes: review_axes("node"),
            principal_review_gates: principal_review_gates("node"),
            language_pack_guidance: language_pack_guidance("node"),
            semantic_intelligence: semantic_intelligence("node"),
            semantic_tool_readiness: semantic_tool_readiness("node"),
            toolchain_readiness: toolchain_readiness(root, "node"),
            proof_plan_status: proof_plan_status(root, "node", &suggested_checks),
            proof_plan_next_action: proof_plan_next_action(root, "node", &suggested_checks),
            proof_plan_summary: proof_plan_summary("node", &suggested_checks),
            suggested_checks,
            artifact_patterns: vec![
                "node_modules/**",
                "dist/**",
                "build/**",
                ".next/**",
                "coverage/**",
            ],
        };
    }
    if has_marker(root, "Cargo.toml", &mut detected) {
        return rust_cargo_profile(root, detected, &["cargo test"]);
    }
    if has_marker(root, "go.mod", &mut detected) {
        return ForgeRepoProfile {
            primary_language: "go",
            language_pack_id: "go",
            detected_language_packs: detected_language_packs(root),
            detected_files: detected,
            quality_signals: quality_signals(root, "go"),
            standards_sources: standards_sources(root),
            standards_source_fingerprints: standards_source_fingerprints(root),
            standards_source_summaries: standards_source_summaries(root),
            review_axes: review_axes("go"),
            principal_review_gates: principal_review_gates("go"),
            language_pack_guidance: language_pack_guidance("go"),
            semantic_intelligence: semantic_intelligence("go"),
            semantic_tool_readiness: semantic_tool_readiness("go"),
            toolchain_readiness: toolchain_readiness(root, "go"),
            proof_plan_status: proof_plan_status(root, "go", &["go test ./..."]),
            proof_plan_next_action: proof_plan_next_action(root, "go", &["go test ./..."]),
            proof_plan_summary: proof_plan_summary("go", &["go test ./..."]),
            suggested_checks: command_list(&["go test ./..."]),
            artifact_patterns: vec!["bin/**", "coverage/**"],
        };
    }

    ForgeRepoProfile {
        primary_language: "unknown",
        language_pack_id: "generic",
        detected_language_packs: vec!["generic"],
        detected_files: detected,
        quality_signals: quality_signals(root, "generic"),
        standards_sources: standards_sources(root),
        standards_source_fingerprints: standards_source_fingerprints(root),
            standards_source_summaries: standards_source_summaries(root),
        review_axes: review_axes("generic"),
        principal_review_gates: principal_review_gates("generic"),
        language_pack_guidance: Vec::new(),
        semantic_intelligence: semantic_intelligence("generic"),
        semantic_tool_readiness: semantic_tool_readiness("generic"),
        toolchain_readiness: Vec::new(),
        proof_plan_status: "operator_check_required",
        proof_plan_next_action:
            "Choose the check command this repo uses before starting medium or high-complexity work."
                .to_string(),
        proof_plan_summary: "Forge could not infer a stack-native proof command for this repo."
            .to_string(),
        suggested_checks: Vec::new(),
        artifact_patterns: common_artifact_patterns(),
    }
}

pub(crate) fn profile_repo_for_task(
    root: &Path,
    intent: &str,
    write_scope: &[String],
    selected_commands: &[String],
) -> ForgeRepoProfile {
    let base = profile_repo(root);
    if (base.detected_language_packs.contains(&"swift-spm") || native_macos_package_exists(root))
        && task_targets_native_macos(intent, write_scope, selected_commands)
    {
        let native_checks = selected_commands
            .iter()
            .map(String::as_str)
            .filter(|command| command.trim() == "make native-macos-operator-check")
            .collect::<Vec<_>>();
        return if native_checks.is_empty() {
            native_macos_profile(root, &["make native-macos-operator-check"])
        } else {
            native_macos_profile(root, &native_checks)
        };
    }
    if !base.detected_language_packs.contains(&"rust-cargo") {
        return base;
    }
    if !task_targets_rust(intent, write_scope, selected_commands) {
        return base;
    }
    let mut detected = Vec::new();
    let _ = has_marker(root, "Cargo.toml", &mut detected);
    let rust_checks = selected_commands
        .iter()
        .map(String::as_str)
        .filter(|command| command.trim_start().starts_with("cargo "))
        .collect::<Vec<_>>();
    if rust_checks.is_empty() {
        rust_cargo_profile(root, detected, &["cargo test"])
    } else {
        rust_cargo_profile(root, detected, &rust_checks)
    }
}

fn native_macos_profile<S: AsRef<str>>(root: &Path, suggested_checks: &[S]) -> ForgeRepoProfile {
    let detected_files = if root.join("apps/mdx-operator-macos/Package.swift").exists() {
        vec!["apps/mdx-operator-macos/Package.swift"]
    } else {
        vec!["Package.swift"]
    };
    ForgeRepoProfile {
        primary_language: "swift",
        language_pack_id: "swift-spm",
        detected_language_packs: native_macos_detected_language_packs(root),
        detected_files,
        quality_signals: quality_signals(root, "swift-spm"),
        standards_sources: standards_sources(root),
        standards_source_fingerprints: standards_source_fingerprints(root),
        standards_source_summaries: standards_source_summaries(root),
        review_axes: review_axes("swift-spm"),
        principal_review_gates: principal_review_gates("swift-spm"),
        language_pack_guidance: language_pack_guidance("swift-spm"),
        semantic_intelligence: semantic_intelligence("swift-spm"),
        semantic_tool_readiness: semantic_tool_readiness("swift-spm"),
        toolchain_readiness: native_macos_toolchain_readiness(root, suggested_checks),
        proof_plan_status: proof_plan_status(root, "swift-spm", suggested_checks),
        proof_plan_next_action: proof_plan_next_action(root, "swift-spm", suggested_checks),
        proof_plan_summary: proof_plan_summary("swift-spm", suggested_checks),
        suggested_checks: suggested_checks
            .iter()
            .map(|command| command.as_ref().to_string())
            .collect(),
        artifact_patterns: vec![".build/**", "DerivedData/**"],
    }
}

fn native_macos_package_exists(root: &Path) -> bool {
    root.join("apps/mdx-operator-macos/Package.swift").exists()
        || root.join("Package.swift").exists()
}

fn native_macos_detected_language_packs(root: &Path) -> Vec<&'static str> {
    let mut packs = detected_language_packs(root);
    if !packs.contains(&"swift-spm") {
        packs.push("swift-spm");
        packs.sort_unstable();
    }
    packs
}

fn rust_cargo_profile<S: AsRef<str>>(
    root: &Path,
    detected: Vec<&'static str>,
    suggested_checks: &[S],
) -> ForgeRepoProfile {
    ForgeRepoProfile {
        primary_language: "rust",
        language_pack_id: "rust-cargo",
        detected_language_packs: detected_language_packs(root),
        detected_files: detected,
        quality_signals: quality_signals(root, "rust-cargo"),
        standards_sources: standards_sources(root),
        standards_source_fingerprints: standards_source_fingerprints(root),
        standards_source_summaries: standards_source_summaries(root),
        review_axes: review_axes("rust-cargo"),
        principal_review_gates: principal_review_gates("rust-cargo"),
        language_pack_guidance: language_pack_guidance("rust-cargo"),
        semantic_intelligence: semantic_intelligence("rust-cargo"),
        semantic_tool_readiness: semantic_tool_readiness("rust-cargo"),
        toolchain_readiness: toolchain_readiness(root, "rust-cargo"),
        proof_plan_status: proof_plan_status(root, "rust-cargo", suggested_checks),
        proof_plan_next_action: proof_plan_next_action(root, "rust-cargo", suggested_checks),
        proof_plan_summary: proof_plan_summary("rust-cargo", suggested_checks),
        suggested_checks: suggested_checks
            .iter()
            .map(|command| command.as_ref().to_string())
            .collect(),
        artifact_patterns: vec!["target/**"],
    }
}

fn task_targets_native_macos(
    intent: &str,
    write_scope: &[String],
    selected_commands: &[String],
) -> bool {
    selected_commands.iter().any(|command| {
        command
            .trim()
            .starts_with("make native-macos-operator-check")
    }) || write_scope
        .iter()
        .any(|target| path_targets_native_macos(target))
        || mentioned_repo_paths(intent)
            .iter()
            .any(|target| path_targets_native_macos(target))
        || {
            let lower = intent.to_ascii_lowercase();
            lower.contains("swiftui")
                || lower.contains("swiftpm")
                || lower.contains("native macos")
                || lower.contains("macos app")
                || lower.contains("native-macos-operator-check")
        }
}

fn task_targets_rust(intent: &str, write_scope: &[String], selected_commands: &[String]) -> bool {
    selected_commands
        .iter()
        .any(|command| command.trim_start().starts_with("cargo "))
        || write_scope.iter().any(|target| path_targets_rust(target))
        || mentioned_repo_paths(intent)
            .iter()
            .any(|target| path_targets_rust(target))
        || intent.to_ascii_lowercase().contains("rust")
}

fn mentioned_repo_paths(text: &str) -> Vec<String> {
    text.split(|ch: char| ch.is_whitespace() || matches!(ch, ',' | ')' | '(' | '"' | '\'' | '`'))
        .map(|token| token.trim_matches(|ch: char| matches!(ch, '.' | ':' | ';')))
        .filter(|token| {
            token.contains('/')
                || token.ends_with(".rs")
                || matches!(*token, "Cargo.toml" | "Cargo.lock")
        })
        .map(ToString::to_string)
        .collect()
}

fn path_targets_rust(target: &str) -> bool {
    let target = target.trim().trim_start_matches("./").to_ascii_lowercase();
    target.ends_with(".rs")
        || target == "cargo.toml"
        || target == "cargo.lock"
        || target.starts_with("crates/")
}

fn path_targets_native_macos(target: &str) -> bool {
    let target = target.trim().trim_start_matches("./").to_ascii_lowercase();
    target == "apps/mdx-operator-macos"
        || target.starts_with("apps/mdx-operator-macos/")
        || target.ends_with(".swift")
        || target == "package.swift"
        || target == "make native-macos-operator-check"
}

fn node_setup_command(root: &Path) -> &'static str {
    if root.join("pnpm-lock.yaml").exists() {
        "pnpm install --frozen-lockfile"
    } else if root.join("yarn.lock").exists() {
        "yarn install --frozen-lockfile"
    } else if root.join("package-lock.json").exists() {
        "npm ci"
    } else {
        "npm install --no-package-lock"
    }
}

fn node_suggested_checks(root: &Path, package_text: &str, test_command: &str) -> Vec<String> {
    let mut checks = Vec::new();
    if node_setup_required(root, package_text) {
        checks.push(node_setup_command(root).to_string());
    }
    checks.push(test_command.to_string());
    checks
}

/// The command that actually runs a node repo's tests. A package-manager `test`
/// script wins when package.json declares one; otherwise, when the repo keeps
/// its tests as spec files under a conventional directory (the shape of a repo
/// that runs `node --test` rather than a test-runner script), use Node's built
/// in runner over that directory. Only when neither exists do we fall back to
/// the package-manager test, which at least names the missing script honestly.
fn node_test_command(root: &Path, package_text: &str, package_manager_test: &str) -> String {
    if package_has_test_script(package_text) {
        return package_manager_test.to_string();
    }
    for dir in ["test", "tests", "__tests__", "src"] {
        if node_test_dir_has_specs(&root.join(dir)) {
            return format!("node --test {dir}");
        }
    }
    package_manager_test.to_string()
}

/// True when package.json declares a real, runnable `test` script (not empty and
/// not the npm placeholder that exits 1).
fn package_has_test_script(package_text: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(package_text)
        .ok()
        .and_then(|value| {
            value
                .get("scripts")
                .and_then(|scripts| scripts.get("test"))
                .and_then(|test| test.as_str())
                .map(str::to_string)
        })
        .map(|script| {
            let script = script.trim();
            !script.is_empty()
                && !script.contains("no test specified")
                && !script.contains("Error: no test")
        })
        .unwrap_or(false)
}

/// True when a directory holds at least one node test spec file.
fn node_test_dir_has_specs(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        name.ends_with(".test.mjs")
            || name.ends_with(".test.js")
            || name.ends_with(".spec.mjs")
            || name.ends_with(".spec.js")
    })
}

fn node_setup_required(root: &Path, package_text: &str) -> bool {
    root.join("package-lock.json").exists()
        || root.join("pnpm-lock.yaml").exists()
        || root.join("yarn.lock").exists()
        || package_text.contains("\"packageManager\"")
        || package_text.contains("\"workspaces\"")
        || package_text.contains("\"dependencies\"")
        || package_text.contains("\"devDependencies\"")
        || package_text.contains("\"peerDependencies\"")
        || package_text.contains("\"optionalDependencies\"")
}

fn command_list(commands: &[&str]) -> Vec<String> {
    commands.iter().map(|command| command.to_string()).collect()
}

pub(crate) fn required_setup_commands(root: &Path, language_pack_id: &str) -> Vec<String> {
    match language_pack_id {
        "java-maven" => command_list(&["mvn -q -DskipTests dependency:go-offline"]),
        "gradle-jvm" | "android-gradle" if root.join("gradlew").exists() => {
            command_list(&["./gradlew dependencies"])
        }
        "node" => {
            let package_text =
                std::fs::read_to_string(root.join("package.json")).unwrap_or_default();
            if node_setup_required(root, &package_text) {
                command_list(&[node_setup_command(root)])
            } else {
                Vec::new()
            }
        }
        "dotnet" => command_list(&["dotnet restore"]),
        "python" if root.join("pyproject.toml").exists() || root.join("setup.py").exists() => {
            command_list(&[
                "python3 -m venv .venv && . .venv/bin/activate && python -m pip install -e . pytest",
            ])
        }
        "python" if root.join("requirements.txt").exists() => command_list(&[
            "python3 -m venv .venv && . .venv/bin/activate && python -m pip install -r requirements.txt pytest",
        ]),
        _ => Vec::new(),
    }
}

pub(crate) fn python_proof_command() -> &'static str {
    ". .venv/bin/activate && pytest"
}

fn python_suggested_checks(root: &Path) -> Vec<String> {
    let mut checks = required_setup_commands(root, "python");
    checks.push(python_proof_command().to_string());
    checks
}

fn swift_suggested_checks(root: &Path) -> Vec<String> {
    swift_executable_check_target(root)
        .map(|target| vec![format!("swift run {target}")])
        .unwrap_or_else(|| command_list(&["swift test"]))
}

fn swift_executable_check_target(root: &Path) -> Option<String> {
    let text = std::fs::read_to_string(root.join("Package.swift")).ok()?;
    let executable_marker = ".executableTarget";
    for segment in text.split(executable_marker).skip(1) {
        let Some(name_start) = segment.find("name:") else {
            continue;
        };
        let after_name = &segment[name_start + "name:".len()..];
        let Some(first_quote) = after_name.find('"') else {
            continue;
        };
        let quoted = &after_name[first_quote + 1..];
        let Some(second_quote) = quoted.find('"') else {
            continue;
        };
        let name = &quoted[..second_quote];
        if name.ends_with("Check") || name.ends_with("Checks") {
            return Some(name.to_string());
        }
    }
    None
}

pub(crate) fn is_setup_command(command: &str) -> bool {
    let command = command.trim();
    if command_contains_behavioral_proof(command) {
        return false;
    }
    command.starts_with("npm install")
        || command == "npm ci"
        || command.starts_with("pnpm install")
        || command.starts_with("yarn install")
        || command.contains("dependency:go-offline")
        || command == "./gradlew dependencies"
        || command == "dotnet restore"
        || command.starts_with("python3 -m venv .venv")
        || command.starts_with("python3 -m pip install")
        || command.starts_with("python -m pip install")
}

fn command_contains_behavioral_proof(command: &str) -> bool {
    command
        .split("&&")
        .flat_map(|part| part.split(';'))
        .map(|part| part.split_whitespace().collect::<Vec<_>>().join(" "))
        .any(|part| {
            let part = part.trim();
            part == "pytest"
                || part.starts_with("npm test")
                || part.starts_with("pnpm test")
                || part.starts_with("yarn test")
                || part.starts_with("node --test")
                || part.starts_with("cargo test")
                || part.starts_with("swift test")
                || part.starts_with("go test")
                || part.starts_with("dotnet test")
                || part == "mvn test"
                || part.ends_with("gradlew test")
                || part.starts_with("./gradlew test")
        })
}

pub(crate) fn profile_json(root: &Path) -> String {
    let profile = profile_repo(root);
    format!(
        r#"{{"primary_language":{},"language_pack_id":{},"detected_language_packs":[{}],"detected_files":[{}],"quality_signals":[{}],"standards_sources":[{}],"standards_source_fingerprints":[{}],"standards_source_summaries":[{}],"review_axes":[{}],"principal_review_gates":[{}],"language_pack_guidance":[{}],"semantic_intelligence":[{}],"semantic_tool_readiness":[{}],"toolchain_readiness":[{}],"proof_plan":{{"status":{},"next_action":{},"summary":{}}},"suggested_checks":[{}],"artifact_patterns":[{}]}}"#,
        json_string_literal(profile.primary_language),
        json_string_literal(profile.language_pack_id),
        json_array(&profile.detected_language_packs),
        json_array(&profile.detected_files),
        json_array(&profile.quality_signals),
        json_array(&profile.standards_sources),
        json_string_array(&profile.standards_source_fingerprints),
        json_string_array(&profile.standards_source_summaries),
        json_array(&profile.review_axes),
        json_array(&profile.principal_review_gates),
        json_array(&profile.language_pack_guidance),
        json_array(&profile.semantic_intelligence),
        json_string_array(&profile.semantic_tool_readiness),
        json_string_array(&profile.toolchain_readiness),
        json_string_literal(profile.proof_plan_status),
        json_string_literal(&profile.proof_plan_next_action),
        json_string_literal(&profile.proof_plan_summary),
        json_string_array(&profile.suggested_checks),
        json_array(&profile.artifact_patterns),
    )
}

#[allow(dead_code)]
pub(crate) fn suggested_checks(root: &Path) -> Vec<String> {
    profile_repo(root).suggested_checks
}

pub(crate) fn profile_json_with_current_setup(
    root: &Path,
    cached_profile: Option<String>,
) -> (String, &'static str) {
    let Some(cached_profile) = cached_profile else {
        return (profile_json(root), "computed_live");
    };
    if cached_profile_needs_setup_refresh(root, &cached_profile) {
        return (profile_json(root), "computed_live_stale_index");
    }
    (cached_profile, "repo_index_cached")
}

fn cached_profile_needs_setup_refresh(root: &Path, cached_profile: &str) -> bool {
    let Ok(profile) = serde_json::from_str::<serde_json::Value>(cached_profile) else {
        return true;
    };
    let language_pack_id = profile["language_pack_id"].as_str().unwrap_or("");
    let required_setup = required_setup_commands(root, language_pack_id);
    if required_setup.is_empty() {
        return false;
    }
    let checks = profile["suggested_checks"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    required_setup
        .iter()
        .any(|setup_command| !checks.iter().any(|check| check == setup_command))
}

pub(crate) fn is_artifact_path(path: &str) -> bool {
    let lower = path.replace('\\', "/").to_ascii_lowercase();
    let parts: Vec<&str> = lower.split('/').collect();
    parts.iter().any(|part| {
        matches!(
            *part,
            ".build"
                | "target"
                | "node_modules"
                | ".gradle"
                | ".next"
                | "dist"
                | "build"
                | "deriveddata"
                | "bin"
                | "obj"
                | "testresults"
                | ".venv"
                | ".pytest_cache"
                | "__pycache__"
                | ".dart_tool"
                | "coverage"
        )
    }) || parts
        .iter()
        .any(|part| part.ends_with(".egg-info") || part.ends_with(".dist-info"))
}

pub(crate) fn artifact_reason(path: &str) -> &'static str {
    let lower = path.replace('\\', "/").to_ascii_lowercase();
    for part in lower.split('/') {
        match part {
            ".build" => return "swiftpm build output",
            "target" => return "cargo or maven build output",
            "node_modules" => return "node dependency output",
            ".gradle" => return "gradle cache output",
            ".next" => return "next.js build output",
            "dist" => return "distribution build output",
            "build" => return "build output",
            "deriveddata" => return "xcode derived data",
            "bin" => return ".net or go build output",
            "obj" => return ".net intermediate output",
            "testresults" => return ".net test output",
            ".venv" => return "python virtual environment",
            ".pytest_cache" => return "pytest cache output",
            "__pycache__" => return "python bytecode cache",
            ".dart_tool" => return "dart tool output",
            "coverage" => return "coverage output",
            part if part.ends_with(".egg-info") || part.ends_with(".dist-info") => {
                return "python package metadata";
            }
            _ => {}
        }
    }
    ""
}

fn common_artifact_patterns() -> Vec<&'static str> {
    vec![
        ".build/**",
        "target/**",
        "node_modules/**",
        ".gradle/**",
        "build/**",
        "dist/**",
        ".next/**",
        "bin/**",
        "obj/**",
        "TestResults/**",
        ".venv/**",
        ".pytest_cache/**",
        "__pycache__/**",
        "*.egg-info/**",
        "*.dist-info/**",
        "DerivedData/**",
        "coverage/**",
    ]
}

fn detected_language_packs(root: &Path) -> Vec<&'static str> {
    let mut packs = Vec::new();
    push_if(
        has_any_name_suffix(root, ".xcworkspace") || has_any_name_suffix(root, ".xcodeproj"),
        &mut packs,
        "ios-xcode",
    );
    push_if(
        has_any_name_suffix(root, "Package.swift"),
        &mut packs,
        "swift-spm",
    );
    push_if(
        has_any_name_suffix(root, "pom.xml"),
        &mut packs,
        "java-maven",
    );
    let has_gradle = has_any_name_suffix(root, "settings.gradle")
        || has_any_name_suffix(root, "settings.gradle.kts")
        || has_any_name_suffix(root, "build.gradle")
        || has_any_name_suffix(root, "build.gradle.kts");
    let has_android = has_gradle && is_android_gradle_repo(root);
    push_if(has_android, &mut packs, "android-gradle");
    push_if(has_gradle && !has_android, &mut packs, "gradle-jvm");
    push_if(
        has_any_name_suffix(root, "global.json")
            || has_any_name_suffix(root, ".csproj")
            || has_any_name_suffix(root, ".sln"),
        &mut packs,
        "dotnet",
    );
    push_if(
        has_any_name_suffix(root, "package.json"),
        &mut packs,
        "node",
    );
    push_if(
        has_any_name_suffix(root, "Cargo.toml"),
        &mut packs,
        "rust-cargo",
    );
    push_if(
        has_any_name_suffix(root, "pyproject.toml")
            || has_any_name_suffix(root, "requirements.txt")
            || has_any_name_suffix(root, "setup.py"),
        &mut packs,
        "python",
    );
    push_if(has_any_name_suffix(root, "go.mod"), &mut packs, "go");
    if packs.is_empty() {
        packs.push("generic");
    }
    packs
}

fn review_axes(language_pack_id: &str) -> Vec<&'static str> {
    match language_pack_id {
        "ios-xcode" => vec![
            "xctest-or-ui-test-coverage",
            "idiomatic-swift-and-uikit-swiftui-boundaries",
            "scheme-target-and-entitlement-risk",
            "deriveddata-output-folded",
        ],
        "swift-spm" => vec![
            "behavioral-xctest-coverage",
            "idiomatic-swift-api-shape",
            "package-target-boundaries",
            "generated-build-output-folded",
        ],
        "java-maven" => vec![
            "junit-regression-coverage",
            "public-api-compatibility",
            "dependency-surface-minimized",
            "target-output-folded",
        ],
        "gradle-jvm" => vec![
            "unit-or-integration-test-coverage",
            "java-kotlin-style-fit",
            "public-api-compatibility",
            "gradle-cache-output-folded",
        ],
        "android-gradle" => vec![
            "android-local-unit-test-coverage",
            "kotlin-java-android-api-fit",
            "manifest-resource-and-permission-risk",
            "gradle-build-cache-output-folded",
        ],
        "dotnet" => vec![
            "test-project-regression-coverage",
            "public-contract-compatibility",
            "async-nullability-conventions",
            "bin-obj-output-folded",
        ],
        "node" => vec![
            "package-script-test-coverage",
            "typescript-or-module-style-fit",
            "dependency-churn-minimized",
            "build-and-coverage-output-folded",
        ],
        "rust-cargo" => vec![
            "cargo-test-coverage",
            "ownership-and-error-style-fit",
            "public-api-surface-minimized",
            "target-output-folded",
        ],
        "python" => vec![
            "pytest-behavior-coverage",
            "typing-and-import-style-fit",
            "dependency-surface-minimized",
            "cache-output-folded",
        ],
        "go" => vec![
            "go-test-coverage",
            "idiomatic-error-and-table-test-style",
            "public-api-compatibility",
            "coverage-output-folded",
        ],
        _ => vec![
            "explicit-user-checks",
            "small-reviewable-diff",
            "repo-conventions-followed",
            "generated-output-folded",
        ],
    }
}

fn principal_review_gates(language_pack_id: &str) -> Vec<&'static str> {
    let stack_gate = match language_pack_id {
        "ios-xcode" => "stack_idioms:ios_targets_schemes_entitlements_and_swift_style",
        "swift-spm" => "stack_idioms:swift_api_shape_and_package_boundaries",
        "java-maven" => "stack_idioms:junit_maven_lifecycle_and_public_api",
        "gradle-jvm" => "stack_idioms:jvm_gradle_or_kotlin_style_and_public_api",
        "android-gradle" => "stack_idioms:android_kotlin_java_lifecycle_and_manifest_boundaries",
        "dotnet" => "stack_idioms:dotnet_nullability_async_and_contract_shape",
        "node" => "stack_idioms:typescript_or_module_style_and_package_scripts",
        "rust-cargo" => "stack_idioms:ownership_error_style_and_public_api",
        "python" => "stack_idioms:typing_import_style_and_pytest_shape",
        "go" => "stack_idioms:go_error_style_table_tests_and_public_api",
        _ => "stack_idioms:repo_conventions_and_small_reviewable_diff",
    };
    vec![
        "behavior:changed_behavior_is_explicit",
        "tests:checks_cover_changed_behavior",
        stack_gate,
        "compatibility:public_contract_or_migration_risk_reviewed",
        "security:no_secret_or_authority_expansion",
        "maintainability:dependency_config_and_generated_churn_justified",
    ]
}

fn language_pack_guidance(language_pack_id: &str) -> Vec<&'static str> {
    match language_pack_id {
        "ios-xcode" => vec![
            "source_of_truth:xcode_workspace_project_targets_schemes",
            "idioms:swift_uikit_swiftui_xctest_boundaries",
            "proof:xcodebuild_test_with_scheme_destination",
            "noise:xcode_deriveddata_and_swiftpm_build_folded",
        ],
        "swift-spm" => vec![
            "source_of_truth:package_swift_targets_tests",
            "idioms:foundation_xctest_small_focused_types",
            "proof:swift_test",
            "noise:swiftpm_build_and_deriveddata_folded",
        ],
        "java-maven" => vec![
            "source_of_truth:pom_modules_dependencies_tests",
            "idioms:java_public_api_junit_regressions",
            "proof:mvn_test",
            "noise:maven_target_folded",
        ],
        "gradle-jvm" => vec![
            "source_of_truth:gradle_files_and_wrapper",
            "idioms:jvm_kotlin_style_public_api",
            "proof:gradlew_test",
            "noise:gradle_build_cache_folded",
        ],
        "android-gradle" => vec![
            "source_of_truth:gradle_android_manifest_resources",
            "idioms:android_kotlin_java_lifecycle_permissions",
            "proof:gradlew_test_debug_unit_test",
            "noise:android_gradle_build_cache_folded",
        ],
        "dotnet" => vec![
            "source_of_truth:solution_project_test_layout",
            "idioms:dotnet_nullability_async_contracts",
            "proof:dotnet_test",
            "noise:dotnet_bin_obj_testresults_folded",
        ],
        "node" => vec![
            "source_of_truth:package_json_and_lockfile",
            "idioms:typescript_module_script_conventions",
            "proof:selected_package_test_command",
            "noise:node_modules_build_dist_coverage_folded",
        ],
        "rust-cargo" => vec![
            "source_of_truth:cargo_workspace_layout",
            "idioms:ownership_errors_small_public_surface",
            "proof:cargo_test",
            "noise:cargo_target_folded",
        ],
        "python" => vec![
            "source_of_truth:pyproject_or_requirements",
            "idioms:typing_import_pytest_conventions",
            "proof:pytest",
            "noise:python_cache_dist_build_folded",
        ],
        "go" => vec![
            "source_of_truth:go_mod_modules",
            "idioms:explicit_errors_table_tests_public_api",
            "proof:go_test_all",
            "noise:go_bin_coverage_folded",
        ],
        _ => Vec::new(),
    }
}

fn semantic_intelligence(language_pack_id: &str) -> Vec<&'static str> {
    match language_pack_id {
        "ios-xcode" => vec![
            "lsp:sourcekit-lsp",
            "diagnostics:sourcekit_lsp_or_xcodebuild",
            "symbols:workspace_symbol_and_definition",
            "references:text_document_references",
            "structural:tree_sitter_swift",
            "fallback:rg_xcode_targets_and_tests",
        ],
        "swift-spm" => vec![
            "lsp:sourcekit-lsp",
            "diagnostics:sourcekit_lsp_or_swift_test",
            "symbols:workspace_symbol_and_definition",
            "references:text_document_references",
            "structural:tree_sitter_swift",
            "fallback:rg_package_targets_and_tests",
        ],
        "java-maven" => vec![
            "lsp:jdtls",
            "diagnostics:jdtls_or_maven_test",
            "symbols:workspace_symbol_and_definition",
            "references:text_document_references",
            "structural:tree_sitter_java",
            "fallback:rg_src_main_and_test",
        ],
        "gradle-jvm" => vec![
            "lsp:kotlin_language_server_or_jdtls",
            "diagnostics:lsp_or_gradle_test",
            "symbols:workspace_symbol_and_definition",
            "references:text_document_references",
            "structural:tree_sitter_kotlin_and_java",
            "fallback:rg_gradle_sources_and_tests",
        ],
        "android-gradle" => vec![
            "lsp:kotlin_language_server_or_jdtls",
            "diagnostics:lsp_or_gradle_android_unit_test",
            "symbols:workspace_symbol_definition_and_resource_refs",
            "references:text_document_references",
            "structural:tree_sitter_kotlin_java_and_xml",
            "fallback:rg_android_sources_resources_manifest",
        ],
        "dotnet" => vec![
            "lsp:omnisharp_or_csharp_ls",
            "diagnostics:roslyn_or_dotnet_test",
            "symbols:workspace_symbol_and_definition",
            "references:text_document_references",
            "structural:tree_sitter_c_sharp",
            "fallback:rg_solution_projects_and_tests",
        ],
        "node" => vec![
            "lsp:typescript_language_server",
            "diagnostics:tsserver_or_package_test",
            "symbols:workspace_symbol_and_definition",
            "references:text_document_references",
            "structural:tree_sitter_typescript_and_javascript",
            "fallback:rg_package_sources_and_tests",
        ],
        "rust-cargo" => vec![
            "lsp:rust-analyzer",
            "diagnostics:rust_analyzer_or_cargo_test",
            "symbols:workspace_symbol_and_definition",
            "references:text_document_references",
            "structural:tree_sitter_rust",
            "fallback:rg_cargo_sources_and_tests",
        ],
        "python" => vec![
            "lsp:pyright_or_basedpyright",
            "diagnostics:pyright_or_pytest",
            "symbols:workspace_symbol_and_definition",
            "references:text_document_references",
            "structural:tree_sitter_python",
            "fallback:rg_python_modules_and_tests",
        ],
        "go" => vec![
            "lsp:gopls",
            "diagnostics:gopls_or_go_test",
            "symbols:workspace_symbol_and_definition",
            "references:text_document_references",
            "structural:tree_sitter_go",
            "fallback:rg_go_packages_and_tests",
        ],
        _ => vec![
            "lsp:none_inferred",
            "diagnostics:selected_checks_only",
            "symbols:file_tree_and_rg",
            "references:rg_only",
            "structural:not_inferred",
            "fallback:operator_supplied_checks_and_repo_standards",
        ],
    }
}

fn semantic_tool_readiness(language_pack_id: &str) -> Vec<String> {
    let mut readiness = vec![
        readiness_line("anchor", "rg", command_available("rg")),
        readiness_line("fallback", "file_tree", true),
    ];
    match language_pack_id {
        "ios-xcode" | "swift-spm" => {
            readiness.push(readiness_line(
                "lsp",
                "sourcekit-lsp",
                command_available("sourcekit-lsp"),
            ));
            readiness.push(readiness_line("structural", "tree_sitter_swift", false));
        }
        "java-maven" => {
            readiness.push(readiness_line("lsp", "jdtls", command_available("jdtls")));
            readiness.push(readiness_line("structural", "tree_sitter_java", false));
        }
        "gradle-jvm" => {
            readiness.push(readiness_line(
                "lsp",
                "kotlin-language-server",
                command_available("kotlin-language-server"),
            ));
            readiness.push(readiness_line("lsp", "jdtls", command_available("jdtls")));
            readiness.push(readiness_line(
                "structural",
                "tree_sitter_kotlin_java",
                false,
            ));
        }
        "android-gradle" => {
            readiness.push(readiness_line(
                "lsp",
                "kotlin-language-server",
                command_available("kotlin-language-server"),
            ));
            readiness.push(readiness_line("lsp", "jdtls", command_available("jdtls")));
            readiness.push(readiness_line(
                "structural",
                "tree_sitter_kotlin_java_xml",
                false,
            ));
        }
        "dotnet" => {
            readiness.push(readiness_line(
                "lsp",
                "omnisharp",
                command_available("omnisharp"),
            ));
            readiness.push(readiness_line(
                "lsp",
                "csharp-ls",
                command_available("csharp-ls"),
            ));
            readiness.push(readiness_line("structural", "tree_sitter_c_sharp", false));
        }
        "node" => {
            readiness.push(readiness_line(
                "lsp",
                "typescript-language-server",
                command_available("typescript-language-server"),
            ));
            readiness.push(readiness_line(
                "structural",
                "tree_sitter_typescript_javascript",
                false,
            ));
        }
        "rust-cargo" => {
            readiness.push(readiness_line(
                "lsp",
                "rust-analyzer",
                command_available("rust-analyzer"),
            ));
            readiness.push(readiness_line("structural", "tree_sitter_rust", false));
        }
        "python" => {
            readiness.push(readiness_line(
                "lsp",
                "pyright",
                command_available("pyright"),
            ));
            readiness.push(readiness_line(
                "lsp",
                "basedpyright",
                command_available("basedpyright"),
            ));
            readiness.push(readiness_line("structural", "tree_sitter_python", false));
        }
        "go" => {
            readiness.push(readiness_line("lsp", "gopls", command_available("gopls")));
            readiness.push(readiness_line("structural", "tree_sitter_go", false));
        }
        _ => {}
    }
    readiness
}

fn toolchain_readiness(root: &Path, language_pack_id: &str) -> Vec<String> {
    match language_pack_id {
        "ios-xcode" => vec![
            readiness_line("tool", "xcodebuild", command_available("xcodebuild")),
            readiness_line("check", "xcodebuild test", command_available("xcodebuild")),
        ],
        "swift-spm" => {
            let proof_command = swift_suggested_checks(root)
                .into_iter()
                .last()
                .unwrap_or_else(|| "swift test".to_string());
            vec![
                readiness_line("tool", "swift", command_available("swift")),
                readiness_line("check", &proof_command, command_available("swift")),
            ]
        }
        "java-maven" => vec![
            readiness_line("tool", "java", command_available("java")),
            readiness_line("tool", "mvn", command_available("mvn")),
            readiness_line(
                "setup",
                "mvn -q -DskipTests dependency:go-offline",
                command_available("java") && command_available("mvn"),
            ),
            readiness_line(
                "check",
                "mvn test",
                command_available("java") && command_available("mvn"),
            ),
        ],
        "gradle-jvm" => {
            let wrapper = executable_file(root.join("gradlew").as_path());
            vec![
                readiness_line("tool", "java", command_available("java")),
                readiness_line("repo_tool", "gradlew", wrapper),
                readiness_line(
                    "setup",
                    "./gradlew dependencies",
                    command_available("java") && wrapper,
                ),
                readiness_line(
                    "check",
                    "./gradlew test",
                    command_available("java") && wrapper,
                ),
            ]
        }
        "android-gradle" => {
            let wrapper = executable_file(root.join("gradlew").as_path());
            vec![
                readiness_line("tool", "java", command_available("java")),
                readiness_line("repo_tool", "gradlew", wrapper),
                readiness_line(
                    "setup",
                    "./gradlew dependencies",
                    command_available("java") && wrapper,
                ),
                readiness_line(
                    "check",
                    "./gradlew testDebugUnitTest",
                    command_available("java") && wrapper,
                ),
            ]
        }
        "dotnet" => vec![
            readiness_line("tool", "dotnet", command_available("dotnet")),
            readiness_line("setup", "dotnet restore", command_available("dotnet")),
            readiness_line("check", "dotnet test", command_available("dotnet")),
        ],
        "node" => {
            let package_manager = if root.join("pnpm-lock.yaml").exists() {
                "pnpm"
            } else if root.join("yarn.lock").exists() {
                "yarn"
            } else {
                "npm"
            };
            let package_text =
                std::fs::read_to_string(root.join("package.json")).unwrap_or_default();
            let mut readiness = vec![
                readiness_line("tool", "node", command_available("node")),
                readiness_line("tool", package_manager, command_available(package_manager)),
            ];
            if node_setup_required(root, &package_text) {
                readiness.push(readiness_line(
                    "setup",
                    node_setup_command(root),
                    command_available("node") && command_available(package_manager),
                ));
            }
            readiness.push(readiness_line(
                "check",
                match package_manager {
                    "pnpm" => "pnpm test",
                    "yarn" => "yarn test",
                    _ => "npm test",
                },
                command_available("node") && command_available(package_manager),
            ));
            readiness
        }
        "rust-cargo" => vec![
            readiness_line("tool", "cargo", command_available("cargo")),
            readiness_line("check", "cargo test", command_available("cargo")),
        ],
        "python" => {
            let pytest_ready = command_available("pytest");
            let python_ready = command_available("python3") || command_available("python");
            let mut readiness = vec![
                readiness_line("tool", "python", python_ready),
                readiness_line("tool", "pytest", pytest_ready),
            ];
            if root.join("pyproject.toml").exists() || root.join("setup.py").exists() {
                readiness.push(readiness_line(
                    "setup",
                    "python3 -m venv .venv && . .venv/bin/activate && python -m pip install -e . pytest",
                    python_ready,
                ));
            } else if root.join("requirements.txt").exists() {
                readiness.push(readiness_line(
                    "setup",
                    "python3 -m venv .venv && . .venv/bin/activate && python -m pip install -r requirements.txt pytest",
                    python_ready,
                ));
            }
            readiness.push(readiness_line(
                "check",
                python_proof_command(),
                python_ready && pytest_ready,
            ));
            readiness
        }
        "go" => vec![
            readiness_line("tool", "go", command_available("go")),
            readiness_line("check", "go test ./...", command_available("go")),
        ],
        _ => Vec::new(),
    }
}

fn native_macos_toolchain_readiness<S: AsRef<str>>(root: &Path, checks: &[S]) -> Vec<String> {
    let proof_command = checks
        .last()
        .map(AsRef::as_ref)
        .unwrap_or("make native-macos-operator-check");
    vec![
        readiness_line("tool", "swift", command_available("swift")),
        readiness_line("tool", "make", command_available("make")),
        readiness_line(
            "check",
            proof_command,
            command_available("make")
                && root.join("apps/mdx-operator-macos/Package.swift").exists(),
        ),
    ]
}

fn proof_plan_status<S: AsRef<str>>(
    root: &Path,
    language_pack_id: &str,
    checks: &[S],
) -> &'static str {
    if language_pack_id == "generic" || checks.is_empty() {
        return "operator_check_required";
    }
    let readiness = toolchain_readiness(root, language_pack_id);
    if checks.iter().any(|check| {
        readiness
            .iter()
            .any(|item| item == &format!("{}=missing", readiness_check_key(check.as_ref())))
    }) {
        "setup_required"
    } else {
        "ready"
    }
}

fn proof_plan_next_action<S: AsRef<str>>(
    root: &Path,
    language_pack_id: &str,
    checks: &[S],
) -> String {
    match proof_plan_status(root, language_pack_id, checks) {
        "ready" => {
            if checks.len() > 1 {
                format!(
                    "Run setup `{}`, then proof `{}` in the isolated worktree.",
                    checks.first().map(AsRef::as_ref).unwrap_or(""),
                    checks.last().map(AsRef::as_ref).unwrap_or("")
                )
            } else {
                format!(
                    "Use `{}` as the proof command; Forge will run it in the isolated worktree.",
                    checks.first().map(AsRef::as_ref).unwrap_or("")
                )
            }
        }
        "setup_required" => format!(
            "Install or activate the local toolchain needed for `{}` before starting this run.",
            checks.last().map(AsRef::as_ref).unwrap_or("")
        ),
        _ => "Choose the check command this repo uses before starting medium or high-complexity work."
            .to_string(),
    }
}

fn proof_plan_summary<S: AsRef<str>>(language_pack_id: &str, checks: &[S]) -> String {
    if language_pack_id == "generic" || checks.is_empty() {
        "Forge could not infer a stack-native proof command for this repo.".to_string()
    } else {
        let command_line = if checks.len() > 1 {
            format!(
                "setup `{}`, then proof `{}`",
                checks.first().map(AsRef::as_ref).unwrap_or(""),
                checks.last().map(AsRef::as_ref).unwrap_or("")
            )
        } else {
            format!(
                "proof `{}`",
                checks.first().map(AsRef::as_ref).unwrap_or("")
            )
        };
        format!(
            "Detected {language_pack_id}; suggested {command_line} with repo-specific artifact noise folded from review."
        )
    }
}

fn readiness_line(kind: &str, name: &str, available: bool) -> String {
    format!(
        "{}:{}={}",
        kind,
        readiness_key(name),
        if available { "available" } else { "missing" }
    )
}

pub(crate) fn readiness_check_key(name: &str) -> String {
    format!("check:{}", readiness_key(name))
}

fn readiness_key(name: &str) -> String {
    let stripped = name.replace("./", "");
    stripped
        .trim_start_matches("./")
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '.' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn command_available(command: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| executable_file(&dir.join(command)))
}

fn executable_file(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn quality_signals(root: &Path, language_pack_id: &str) -> Vec<&'static str> {
    let mut signals = Vec::new();
    push_if(
        root.join(".github/workflows").exists(),
        &mut signals,
        "ci:github-actions",
    );
    push_if(
        root.join("bitbucket-pipelines.yml").exists(),
        &mut signals,
        "ci:bitbucket-pipelines",
    );
    push_if(
        root.join(".gitlab-ci.yml").exists(),
        &mut signals,
        "ci:gitlab",
    );
    push_if(root.join("README.md").exists(), &mut signals, "docs:readme");
    push_if(root.join("LICENSE").exists(), &mut signals, "docs:license");

    match language_pack_id {
        "ios-xcode" => {
            push_if(
                has_any_name_suffix(root, ".xcworkspace"),
                &mut signals,
                "tooling:xcode-workspace",
            );
            push_if(
                has_any_name_suffix(root, ".xcodeproj"),
                &mut signals,
                "tooling:xcode-project",
            );
            push_if(
                has_any_name_suffix(root, "Tests.swift"),
                &mut signals,
                "tests:xctest-files",
            );
        }
        "swift-spm" => {
            push_if(
                root.join("Tests").exists(),
                &mut signals,
                "tests:swiftpm-tests",
            );
        }
        "java-maven" => {
            push_if(
                root.join("src/test").exists(),
                &mut signals,
                "tests:maven-src-test",
            );
            push_if(
                std::fs::read_to_string(root.join("pom.xml"))
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .contains("junit"),
                &mut signals,
                "tests:junit",
            );
        }
        "gradle-jvm" => {
            push_if(
                root.join("src/test").exists(),
                &mut signals,
                "tests:gradle-src-test",
            );
            push_if(
                root.join("gradlew").exists(),
                &mut signals,
                "tooling:gradle-wrapper",
            );
        }
        "android-gradle" => {
            push_if(
                has_any_name_suffix(root, "AndroidManifest.xml"),
                &mut signals,
                "platform:android-manifest",
            );
            push_if(
                has_any_name_suffix(root, "Test.kt") || has_any_name_suffix(root, "Test.java"),
                &mut signals,
                "tests:android-local-unit-tests",
            );
            push_if(
                root.join("gradlew").exists(),
                &mut signals,
                "tooling:gradle-wrapper",
            );
        }
        "dotnet" => {
            push_if(
                has_any_name_suffix(root, "Tests.csproj"),
                &mut signals,
                "tests:dotnet-test-project",
            );
        }
        "node" => {
            let package_text =
                std::fs::read_to_string(root.join("package.json")).unwrap_or_default();
            push_if(
                package_text.contains("\"test\""),
                &mut signals,
                "tests:package-script",
            );
            push_if(
                root.join("pnpm-lock.yaml").exists(),
                &mut signals,
                "tooling:pnpm",
            );
            push_if(
                root.join("yarn.lock").exists(),
                &mut signals,
                "tooling:yarn",
            );
            push_if(
                root.join("package-lock.json").exists(),
                &mut signals,
                "tooling:npm-lock",
            );
        }
        "rust-cargo" => {
            push_if(
                root.join("tests").exists(),
                &mut signals,
                "tests:cargo-integration",
            );
            push_if(
                root.join("rustfmt.toml").exists(),
                &mut signals,
                "style:rustfmt",
            );
        }
        "python" => {
            push_if(
                root.join("tests").exists(),
                &mut signals,
                "tests:pytest-layout",
            );
            push_if(
                root.join("pytest.ini").exists(),
                &mut signals,
                "tests:pytest-config",
            );
            push_if(root.join("ruff.toml").exists(), &mut signals, "style:ruff");
        }
        "go" => {
            push_if(
                has_any_name_suffix(root, "_test.go"),
                &mut signals,
                "tests:go-test-files",
            );
        }
        _ => {}
    }
    signals
}

fn standards_sources(root: &Path) -> Vec<&'static str> {
    let mut sources = Vec::new();
    push_if(root.join("AGENTS.md").exists(), &mut sources, "AGENTS.md");
    push_if(root.join("CLAUDE.md").exists(), &mut sources, "CLAUDE.md");
    push_if(
        root.join("CONTRIBUTING.md").exists(),
        &mut sources,
        "CONTRIBUTING.md",
    );
    push_if(
        root.join("docs/CONTRIBUTING.md").exists(),
        &mut sources,
        "docs/CONTRIBUTING.md",
    );
    push_if(root.join("CODEOWNERS").exists(), &mut sources, "CODEOWNERS");
    push_if(
        root.join(".github/CODEOWNERS").exists(),
        &mut sources,
        ".github/CODEOWNERS",
    );
    push_if(
        root.join("SECURITY.md").exists(),
        &mut sources,
        "SECURITY.md",
    );
    push_if(
        root.join("docs/SECURITY.md").exists(),
        &mut sources,
        "docs/SECURITY.md",
    );
    push_if(
        root.join(".editorconfig").exists(),
        &mut sources,
        ".editorconfig",
    );
    push_if(root.join("README.md").exists(), &mut sources, "README.md");
    push_if(
        root.join(".github/workflows").exists(),
        &mut sources,
        ".github/workflows/**",
    );
    push_if(
        root.join("bitbucket-pipelines.yml").exists(),
        &mut sources,
        "bitbucket-pipelines.yml",
    );
    push_if(
        root.join(".gitlab-ci.yml").exists(),
        &mut sources,
        ".gitlab-ci.yml",
    );
    sources
}

fn standards_source_fingerprints(root: &Path) -> Vec<String> {
    standards_sources(root)
        .into_iter()
        .filter_map(|source| fingerprint_standard_source(root, source))
        .collect()
}

fn standards_source_summaries(root: &Path) -> Vec<String> {
    standards_sources(root)
        .into_iter()
        .map(|source| summarize_standard_source(root, source))
        .collect()
}

fn summarize_standard_source(root: &Path, source: &'static str) -> String {
    let mut tags = Vec::new();
    for tag in path_standard_tags(source) {
        push_string_tag(&mut tags, tag);
    }
    let text = match source {
        ".github/workflows/**" => standards_directory_text(root, ".github/workflows"),
        _ => standards_file_text(root, source),
    };
    let lower = text.to_ascii_lowercase();
    if contains_any(
        &lower,
        &["test", "xctest", "junit", "pytest", "vitest", "go test"],
    ) {
        push_string_tag(&mut tags, "mentions_tests");
    }
    if contains_any(
        &lower,
        &["lint", "format", "style", "editorconfig", "rustfmt", "ruff"],
    ) {
        push_string_tag(&mut tags, "mentions_style");
    }
    if contains_any(
        &lower,
        &[
            "security",
            "secret",
            "vulnerability",
            "permission",
            "credential",
        ],
    ) {
        push_string_tag(&mut tags, "mentions_security");
    }
    if contains_any(
        &lower,
        &[
            "review",
            "approval",
            "approve",
            "codeowner",
            "pull request",
            "merge",
        ],
    ) {
        push_string_tag(&mut tags, "mentions_review");
    }
    if contains_any(&lower, &["ci", "build", "pipeline", "workflow", "actions"]) {
        push_string_tag(&mut tags, "mentions_ci");
    }
    if contains_any(&lower, &["release", "deploy", "publish", "version"]) {
        push_string_tag(&mut tags, "mentions_release");
    }
    if contains_any(
        &lower,
        &[
            "compatibility",
            "migration",
            "public api",
            "breaking change",
        ],
    ) {
        push_string_tag(&mut tags, "mentions_compatibility");
    }
    if tags.is_empty() {
        push_string_tag(&mut tags, "present");
    }
    format!("{source}={}", tags.join("+"))
}

fn path_standard_tags(source: &str) -> Vec<&'static str> {
    match source {
        "AGENTS.md" | "CLAUDE.md" => vec!["agent_instructions"],
        "CONTRIBUTING.md" | "docs/CONTRIBUTING.md" => vec!["contribution_workflow"],
        "CODEOWNERS" | ".github/CODEOWNERS" => vec!["ownership_review"],
        "SECURITY.md" | "docs/SECURITY.md" => vec!["security_policy"],
        ".editorconfig" => vec!["style_config"],
        "README.md" => vec!["project_overview"],
        ".github/workflows/**" | "bitbucket-pipelines.yml" | ".gitlab-ci.yml" => {
            vec!["ci_workflow"]
        }
        _ => Vec::new(),
    }
}

fn standards_file_text(root: &Path, source: &str) -> String {
    let path = root.join(source);
    let Ok(metadata) = std::fs::symlink_metadata(&path) else {
        return String::new();
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 128 * 1024 {
        return String::new();
    }
    let Ok(text) = std::fs::read_to_string(path) else {
        return String::new();
    };
    text.chars().take(16_000).collect()
}

fn standards_directory_text(root: &Path, source: &str) -> String {
    let dir = root.join(source);
    let Ok(metadata) = std::fs::symlink_metadata(&dir) else {
        return String::new();
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return String::new();
    }
    let mut files = Vec::new();
    collect_regular_files(&dir, &dir, &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut text = String::new();
    for (relative, contents) in files.into_iter().take(16) {
        text.push_str(&relative);
        text.push('\n');
        text.push_str(
            &String::from_utf8_lossy(&contents)
                .chars()
                .take(2000)
                .collect::<String>(),
        );
        text.push('\n');
    }
    text
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

fn push_string_tag(tags: &mut Vec<String>, tag: &str) {
    if !tags.iter().any(|existing| existing == tag) {
        tags.push(tag.to_string());
    }
}

fn fingerprint_standard_source(root: &Path, source: &'static str) -> Option<String> {
    let bytes = match source {
        ".github/workflows/**" => fingerprint_directory_bytes(root, ".github/workflows"),
        _ => fingerprint_file_bytes(root, source),
    }?;
    Some(format!("{source}={}", fnv1a64(&bytes)))
}

fn fingerprint_file_bytes(root: &Path, source: &str) -> Option<Vec<u8>> {
    let path = root.join(source);
    let metadata = std::fs::symlink_metadata(&path).ok()?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return None;
    }
    std::fs::read(path).ok()
}

fn fingerprint_directory_bytes(root: &Path, source: &str) -> Option<Vec<u8>> {
    let dir = root.join(source);
    let metadata = std::fs::symlink_metadata(&dir).ok()?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return None;
    }
    let mut files = Vec::new();
    collect_regular_files(&dir, &dir, &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));
    if files.is_empty() {
        return None;
    }
    let mut bytes = Vec::new();
    for (relative, contents) in files.into_iter().take(64) {
        bytes.extend_from_slice(relative.as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&contents);
        bytes.push(0xff);
    }
    Some(bytes)
}

fn collect_regular_files(root: &Path, dir: &Path, files: &mut Vec<(String, Vec<u8>)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_regular_files(root, &path, files);
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        let Ok(contents) = std::fs::read(&path) else {
            continue;
        };
        let Some(relative) = path
            .strip_prefix(root)
            .ok()
            .and_then(|value| value.to_str())
            .map(|value| value.replace('\\', "/"))
        else {
            continue;
        };
        files.push((relative, contents));
    }
}

fn fnv1a64(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a64:{hash:016x}")
}

fn push_if(condition: bool, signals: &mut Vec<&'static str>, signal: &'static str) {
    if condition && !signals.contains(&signal) {
        signals.push(signal);
    }
}

fn has_any_name_suffix(root: &Path, suffix: &str) -> bool {
    let mut visited_entries = 0usize;
    has_any_name_suffix_bounded(root, suffix, &mut visited_entries)
}

fn has_any_name_suffix_bounded(root: &Path, suffix: &str, visited_entries: &mut usize) -> bool {
    let Ok(entries) = std::fs::read_dir(root) else {
        return false;
    };
    for entry in entries.flatten() {
        if *visited_entries >= MARKER_SCAN_MAX_ENTRIES {
            return false;
        }
        *visited_entries += 1;
        let path = entry.path();
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            // Never descend into build output, dependencies, or VCS internals.
            // They are not the repo's source: a Rust target/ or a node_modules/
            // holds GBs across thousands of files, so walking them turns marker
            // detection into a multi-second (here, 40s+) scan that blocks run
            // admission - and a manifest found inside them would misdetect the
            // repo anyway. Bounding the walk to source keeps admission fast and
            // deterministic, which is the boring-architecture contract.
            if is_ignored_scan_dir(&path) {
                continue;
            }
            if has_any_name_suffix_bounded(&path, suffix, visited_entries) {
                return true;
            }
        }
        if path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|name| name.ends_with(suffix))
        {
            return true;
        }
    }
    false
}

/// Directories a source scan must never descend into: build output, fetched
/// dependencies, and VCS internals. Markers (Cargo.toml, package.json, go.mod,
/// ...) live in source, never here. Matched by exact directory name so it holds
/// at any depth (a nested crate's target/, a sub-package's node_modules/).
fn is_ignored_scan_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(
            ".git"
                | "target"
                | "node_modules"
                | ".build"
                | "DerivedData"
                | "dist"
                | "build"
                | ".next"
                | ".svelte-kit"
                | ".nuxt"
                | "vendor"
                | ".venv"
                | "venv"
                | "__pycache__"
                | ".gradle"
                | "Pods"
                | ".terraform"
                | "coverage"
                | ".mypy_cache"
                | ".pytest_cache"
                | ".cargo"
                | ".turbo"
                | ".parcel-cache"
        )
    )
}

fn is_android_gradle_repo(root: &Path) -> bool {
    if has_any_name_suffix(root, "AndroidManifest.xml") {
        return true;
    }
    for gradle_file in [
        "settings.gradle",
        "settings.gradle.kts",
        "build.gradle",
        "build.gradle.kts",
    ] {
        let text = std::fs::read_to_string(root.join(gradle_file)).unwrap_or_default();
        if text.contains("com.android.application") || text.contains("com.android.library") {
            return true;
        }
    }
    false
}

fn json_array(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| json_string_literal(value))
        .collect::<Vec<_>>()
        .join(",")
}

fn json_string_array(values: &[String]) -> String {
    values
        .iter()
        .map(|value| json_string_literal(value))
        .collect::<Vec<_>>()
        .join(",")
}

fn has_marker(root: &Path, marker: &'static str, detected: &mut Vec<&'static str>) -> bool {
    if root.join(marker).exists() {
        detected.push(marker);
        true
    } else {
        false
    }
}

fn has_any_extension(
    root: &Path,
    extension: &'static str,
    detected: &mut Vec<&'static str>,
) -> bool {
    let Ok(entries) = std::fs::read_dir(root) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) == Some(extension) {
            detected.push(match extension {
                "csproj" => "*.csproj",
                "sln" => "*.sln",
                "xcodeproj" => "*.xcodeproj",
                "xcworkspace" => "*.xcworkspace",
                _ => "*",
            });
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swift_package_gets_swift_pack_and_spm_artifacts() {
        let dir = temp_dir("swift");
        std::fs::write(dir.join("Package.swift"), "// swift\n").expect("write");

        let profile = profile_repo(&dir);

        assert_eq!(profile.primary_language, "swift");
        assert_eq!(profile.language_pack_id, "swift-spm");
        assert_eq!(profile.detected_language_packs, vec!["swift-spm"]);
        assert_eq!(profile.suggested_checks, vec!["swift test"]);
        assert!(profile.review_axes.contains(&"behavioral-xctest-coverage"));
        assert!(
            profile
                .principal_review_gates
                .contains(&"tests:checks_cover_changed_behavior")
        );
        assert!(
            profile
                .language_pack_guidance
                .contains(&"source_of_truth:package_swift_targets_tests")
        );
        assert!(profile.language_pack_guidance.contains(&"proof:swift_test"));
        assert!(
            profile
                .toolchain_readiness
                .iter()
                .any(|item| item.starts_with("tool:swift="))
        );
        assert!(
            profile
                .toolchain_readiness
                .iter()
                .any(|item| item.starts_with("check:swift_test="))
        );
        assert!(profile.artifact_patterns.contains(&".build/**"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn swift_package_with_executable_check_prefers_swift_run_proof() {
        let dir = temp_dir("swift_executable_check");
        std::fs::write(
            dir.join("Package.swift"),
            r#"// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "Canary",
    products: [
        .executable(name: "CanaryCheck", targets: ["CanaryCheck"])
    ],
    targets: [
        .target(name: "Canary"),
        .executableTarget(name: "CanaryCheck", dependencies: ["Canary"])
    ]
)
"#,
        )
        .expect("write package");

        let profile = profile_repo(&dir);

        assert_eq!(profile.language_pack_id, "swift-spm");
        assert_eq!(profile.suggested_checks, vec!["swift run CanaryCheck"]);
        assert!(
            profile
                .proof_plan_summary
                .contains("proof `swift run CanaryCheck`")
        );
        assert!(
            profile
                .toolchain_readiness
                .iter()
                .any(|item| item.starts_with("check:swift_run_CanaryCheck="))
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn xcode_project_gets_ios_pack_and_mobile_review_gates() {
        let dir = temp_dir("ios");
        std::fs::create_dir_all(dir.join("App.xcodeproj")).expect("mkdir xcodeproj");
        std::fs::write(dir.join("AppTests.swift"), "import XCTest\n").expect("write tests");

        let profile = profile_repo(&dir);

        assert_eq!(profile.primary_language, "swift-ios");
        assert_eq!(profile.language_pack_id, "ios-xcode");
        assert_eq!(profile.detected_language_packs, vec!["ios-xcode"]);
        assert_eq!(profile.detected_files, vec!["*.xcodeproj"]);
        assert_eq!(profile.suggested_checks, vec!["xcodebuild test"]);
        assert!(
            profile
                .review_axes
                .contains(&"scheme-target-and-entitlement-risk")
        );
        assert!(
            profile
                .principal_review_gates
                .contains(&"stack_idioms:ios_targets_schemes_entitlements_and_swift_style")
        );
        assert!(
            profile
                .language_pack_guidance
                .contains(&"source_of_truth:xcode_workspace_project_targets_schemes")
        );
        assert!(
            profile
                .toolchain_readiness
                .iter()
                .any(|item| item.starts_with("check:xcodebuild_test="))
        );
        assert!(profile.quality_signals.contains(&"tooling:xcode-project"));
        assert!(profile.artifact_patterns.contains(&"DerivedData/**"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn gradle_repo_gets_gradle_jvm_pack() {
        let dir = temp_dir("gradle");
        std::fs::write(
            dir.join("build.gradle.kts"),
            "plugins { kotlin(\"jvm\") }\n",
        )
        .expect("write");

        let profile = profile_repo(&dir);

        assert_eq!(profile.primary_language, "kotlin-java");
        assert_eq!(profile.language_pack_id, "gradle-jvm");
        assert_eq!(
            profile.suggested_checks,
            vec!["./gradlew dependencies", "./gradlew test"]
        );
        assert!(profile.artifact_patterns.contains(&".gradle/**"));
        assert_eq!(profile.proof_plan_status, "setup_required");
        assert!(
            profile
                .proof_plan_next_action
                .contains("local toolchain needed for `./gradlew test`")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn android_gradle_repo_gets_android_pack_before_plain_gradle() {
        let dir = temp_dir("android");
        std::fs::write(
            dir.join("build.gradle.kts"),
            "plugins { id(\"com.android.application\") }\n",
        )
        .expect("write gradle");
        std::fs::create_dir_all(dir.join("app/src/main")).expect("mkdir android");
        std::fs::write(
            dir.join("app/src/main/AndroidManifest.xml"),
            "<manifest />\n",
        )
        .expect("write manifest");

        let profile = profile_repo(&dir);

        assert_eq!(profile.primary_language, "android-kotlin-java");
        assert_eq!(profile.language_pack_id, "android-gradle");
        assert_eq!(profile.detected_language_packs, vec!["android-gradle"]);
        assert_eq!(
            profile.suggested_checks,
            vec!["./gradlew dependencies", "./gradlew testDebugUnitTest"]
        );
        assert!(
            profile
                .review_axes
                .contains(&"manifest-resource-and-permission-risk")
        );
        assert!(
            profile
                .principal_review_gates
                .contains(&"stack_idioms:android_kotlin_java_lifecycle_and_manifest_boundaries")
        );
        assert!(
            profile
                .language_pack_guidance
                .contains(&"source_of_truth:gradle_android_manifest_resources")
        );
        assert!(
            profile
                .toolchain_readiness
                .iter()
                .any(|item| item.starts_with("check:gradlew_testDebugUnitTest="))
        );
        assert!(
            profile
                .quality_signals
                .contains(&"platform:android-manifest")
        );
        assert!(profile.artifact_patterns.contains(&".gradle/**"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn monorepo_records_neighboring_language_packs_without_changing_primary_defaults() {
        let dir = temp_dir("monorepo");
        std::fs::write(
            dir.join("package.json"),
            "{\"scripts\":{\"test\":\"node --test\"}}\n",
        )
        .expect("write package");
        std::fs::create_dir_all(dir.join("services/api")).expect("mkdir api");
        std::fs::write(dir.join("services/api/pom.xml"), "<project />\n").expect("write pom");
        std::fs::create_dir_all(dir.join("tools")).expect("mkdir tools");
        std::fs::write(
            dir.join("tools/Cargo.toml"),
            "[package]\nname=\"tool\"\nversion=\"0.1.0\"\n",
        )
        .expect("write cargo");

        let profile = profile_repo(&dir);
        let json = profile_json(&dir);

        assert_eq!(profile.language_pack_id, "node");
        assert_eq!(profile.suggested_checks, vec!["npm test"]);
        assert_eq!(
            profile.detected_language_packs,
            vec!["java-maven", "node", "rust-cargo"]
        );
        assert!(json.contains(r#""detected_language_packs":["java-maven","node","rust-cargo"]"#));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn node_repo_without_a_test_script_uses_node_test_over_the_test_dir() {
        // The shape of apps/mdx-host: a package.json with scripts but no `test`
        // script, and tests kept as spec files under test/. The old profile
        // suggested `npm test` (which fails, since there is no test script); the
        // fix derives `node --test test`.
        let dir = temp_dir("node_no_test_script");
        std::fs::write(
            dir.join("package.json"),
            "{\"scripts\":{\"build\":\"vite build\"},\"devDependencies\":{}}\n",
        )
        .expect("write package");
        std::fs::create_dir_all(dir.join("test")).expect("mkdir test");
        std::fs::write(dir.join("test/thing.test.mjs"), "// spec\n").expect("write spec");

        let profile = profile_repo(&dir);

        assert_eq!(profile.language_pack_id, "node");
        assert!(
            profile
                .suggested_checks
                .contains(&"node --test test".to_string()),
            "{:?}",
            profile.suggested_checks
        );
        assert!(
            !profile
                .suggested_checks
                .iter()
                .any(|check| check == "npm test" || check == "pnpm test"),
            "{:?}",
            profile.suggested_checks
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn dependency_free_node_repo_uses_test_without_install_setup() {
        let dir = temp_dir("node_no_deps");
        std::fs::write(
            dir.join("package.json"),
            "{\"scripts\":{\"test\":\"node --test\"}}\n",
        )
        .expect("write package");

        let profile = profile_repo(&dir);

        assert_eq!(profile.language_pack_id, "node");
        assert_eq!(profile.suggested_checks, vec!["npm test"]);
        assert!(required_setup_commands(&dir, "node").is_empty());
        assert!(
            !profile
                .toolchain_readiness
                .iter()
                .any(|item| item.starts_with("setup:"))
        );
        assert!(
            profile
                .toolchain_readiness
                .iter()
                .any(|item| item.starts_with("check:npm_test="))
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn stale_node_index_is_refreshed_to_include_setup_check() {
        let dir = temp_dir("node_stale_index");
        std::fs::write(
            dir.join("package.json"),
            "{\"scripts\":{\"test\":\"node test.js\"}}\n",
        )
        .expect("write package");
        std::fs::write(dir.join("package-lock.json"), "{}\n").expect("write lock");
        let stale = r#"{"language_pack_id":"node","suggested_checks":["npm test"]}"#.to_string();

        let (profile_json, source) = profile_json_with_current_setup(&dir, Some(stale));
        let value: serde_json::Value = serde_json::from_str(&profile_json).expect("json");

        assert_eq!(source, "computed_live_stale_index");
        assert_eq!(value["suggested_checks"][0], "npm ci");
        assert_eq!(value["suggested_checks"][1], "npm test");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn profile_records_repo_quality_signals_without_granting_authority() {
        let swift = temp_dir("swift_quality");
        std::fs::create_dir_all(swift.join(".github/workflows")).expect("mkdir ci");
        std::fs::create_dir_all(swift.join("Tests/AppTests")).expect("mkdir tests");
        std::fs::write(swift.join("Package.swift"), "// swift\n").expect("write");
        std::fs::write(swift.join("README.md"), "# canary\n").expect("write");

        let swift_profile = profile_repo(&swift);

        assert_eq!(swift_profile.language_pack_id, "swift-spm");
        assert!(swift_profile.quality_signals.contains(&"ci:github-actions"));
        assert!(
            swift_profile
                .standards_sources
                .contains(&".github/workflows/**")
        );
        assert!(swift_profile.quality_signals.contains(&"docs:readme"));
        assert!(swift_profile.standards_sources.contains(&"README.md"));
        assert!(
            swift_profile
                .quality_signals
                .contains(&"tests:swiftpm-tests")
        );
        let _ = std::fs::remove_dir_all(swift);

        let node = temp_dir("node_quality");
        std::fs::write(
            node.join("package.json"),
            "{\"scripts\":{\"test\":\"vitest\"}}\n",
        )
        .expect("write package");
        std::fs::write(node.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n")
            .expect("write lock");

        let node_profile = profile_repo(&node);

        assert_eq!(node_profile.language_pack_id, "node");
        assert!(
            node_profile
                .quality_signals
                .contains(&"tests:package-script")
        );
        assert!(node_profile.quality_signals.contains(&"tooling:pnpm"));
        let _ = std::fs::remove_dir_all(node);
    }

    #[test]
    fn profile_records_repo_standards_sources_without_reading_secrets_or_granting_authority() {
        let dir = temp_dir("standards_sources");
        std::fs::create_dir_all(dir.join(".github/workflows")).expect("mkdir workflows");
        std::fs::write(dir.join("Package.swift"), "// swift\n").expect("write package");
        std::fs::write(dir.join("AGENTS.md"), "# rules\n").expect("write agents");
        std::fs::write(dir.join(".github/CODEOWNERS"), "* @team\n").expect("write codeowners");
        std::fs::write(dir.join(".github/workflows/ci.yml"), "name: ci\n").expect("write ci");
        std::fs::write(dir.join("SECURITY.md"), "# security\n").expect("write security");
        std::fs::write(dir.join(".editorconfig"), "root = true\n").expect("write editorconfig");
        std::fs::write(dir.join(".env"), "SECRET=do-not-read\n").expect("write secret");

        let profile = profile_repo(&dir);
        let json = profile_json(&dir);

        assert_eq!(profile.language_pack_id, "swift-spm");
        assert!(profile.standards_sources.contains(&"AGENTS.md"));
        assert!(profile.standards_sources.contains(&".github/CODEOWNERS"));
        assert!(profile.standards_sources.contains(&"SECURITY.md"));
        assert!(profile.standards_sources.contains(&".editorconfig"));
        assert!(profile.standards_sources.contains(&".github/workflows/**"));
        assert!(
            profile
                .standards_source_fingerprints
                .iter()
                .any(|item| item.starts_with("AGENTS.md=fnv1a64:"))
        );
        assert!(
            profile
                .standards_source_summaries
                .iter()
                .any(|item| item == "AGENTS.md=agent_instructions")
        );
        assert!(
            profile
                .standards_source_summaries
                .iter()
                .any(|item| item == ".github/CODEOWNERS=ownership_review")
        );
        assert!(
            profile
                .standards_source_summaries
                .iter()
                .any(|item| item == "SECURITY.md=security_policy+mentions_security")
        );
        assert!(
            profile
                .standards_source_summaries
                .iter()
                .any(|item| item == ".github/workflows/**=ci_workflow+mentions_ci")
        );
        assert!(
            profile
                .standards_source_fingerprints
                .iter()
                .any(|item| item.starts_with(".github/workflows/**=fnv1a64:"))
        );
        assert!(
            !profile
                .standards_source_fingerprints
                .iter()
                .any(|item| item.contains(".env"))
        );
        assert!(json.contains(r#""standards_sources":["AGENTS.md","#));
        assert!(json.contains(
            r#""language_pack_guidance":["source_of_truth:package_swift_targets_tests","#
        ));
        assert!(json.contains(r#""standards_source_fingerprints":["AGENTS.md=fnv1a64:"#));
        assert!(json.contains(r#""standards_source_summaries":["AGENTS.md=agent_instructions","#));
        assert!(!json.contains("do-not-read"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn language_pack_canaries_cover_major_external_repo_families() {
        for canary in [
            Canary {
                label: "ios",
                marker: "Canary.xcodeproj",
                contents: "",
                pack: "ios-xcode",
                check: "xcodebuild test",
                artifact: "DerivedData/**",
            },
            Canary {
                label: "java",
                marker: "pom.xml",
                contents: "<project />\n",
                pack: "java-maven",
                check: "mvn -q -DskipTests dependency:go-offline,mvn test",
                artifact: "target/**",
            },
            Canary {
                label: "dotnet",
                marker: "App.csproj",
                contents: "<Project />\n",
                pack: "dotnet",
                check: "dotnet restore,dotnet test",
                artifact: "obj/**",
            },
            Canary {
                label: "node",
                marker: "package.json",
                contents: "{\"scripts\":{\"test\":\"node --test\"}}\n",
                pack: "node",
                check: "npm test",
                artifact: "node_modules/**",
            },
            Canary {
                label: "rust",
                marker: "Cargo.toml",
                contents: "[package]\nname=\"canary\"\nversion=\"0.1.0\"\n",
                pack: "rust-cargo",
                check: "cargo test",
                artifact: "target/**",
            },
            Canary {
                label: "python",
                marker: "pyproject.toml",
                contents: "[project]\nname=\"canary\"\n",
                pack: "python",
                check: "python3 -m venv .venv && . .venv/bin/activate && python -m pip install -e . pytest,. .venv/bin/activate && pytest",
                artifact: "__pycache__/**",
            },
            Canary {
                label: "go",
                marker: "go.mod",
                contents: "module example.com/canary\n",
                pack: "go",
                check: "go test ./...",
                artifact: "coverage/**",
            },
        ] {
            let dir = temp_dir(canary.label);
            std::fs::write(dir.join(canary.marker), canary.contents).expect("write");

            let profile = profile_repo(&dir);

            assert_eq!(profile.language_pack_id, canary.pack);
            assert_eq!(profile.suggested_checks.join(","), canary.check);
            assert!(!profile.review_axes.is_empty());
            assert!(profile.artifact_patterns.contains(&canary.artifact));
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn python_requirements_repo_gets_setup_before_pytest() {
        let dir = temp_dir("python_requirements");
        std::fs::write(dir.join("requirements.txt"), "pytest\nrequests\n").expect("requirements");

        let profile = profile_repo(&dir);

        assert_eq!(profile.language_pack_id, "python");
        assert_eq!(
            profile.suggested_checks,
            vec![
                "python3 -m venv .venv && . .venv/bin/activate && python -m pip install -r requirements.txt pytest",
                ". .venv/bin/activate && pytest"
            ]
        );
        assert!(
            profile
                .toolchain_readiness
                .iter()
                .any(|item| item.starts_with("setup:python3_-m_venv_.venv"))
        );
        assert_eq!(
            required_setup_commands(&dir, "python"),
            vec![
                "python3 -m venv .venv && . .venv/bin/activate && python -m pip install -r requirements.txt pytest"
            ]
        );
        assert!(is_setup_command(
            "python3 -m venv .venv && . .venv/bin/activate && python -m pip install -r requirements.txt pytest"
        ));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn combined_setup_and_test_command_is_behavioral_proof() {
        assert!(is_setup_command("npm install --no-package-lock"));
        assert!(!is_setup_command(
            "npm install --no-package-lock && npm test"
        ));
        assert!(!is_setup_command(
            "python3 -m venv .venv && . .venv/bin/activate && pytest"
        ));
        assert!(!is_setup_command(
            "./gradlew dependencies && ./gradlew test"
        ));
    }

    #[test]
    fn python_pyproject_repo_gets_editable_install_before_pytest() {
        let dir = temp_dir("python_pyproject");
        std::fs::write(
            dir.join("pyproject.toml"),
            "[project]\nname = \"canary\"\nversion = \"0.1.0\"\n",
        )
        .expect("pyproject");

        let profile = profile_repo(&dir);

        assert_eq!(profile.language_pack_id, "python");
        assert_eq!(
            profile.suggested_checks,
            vec![
                "python3 -m venv .venv && . .venv/bin/activate && python -m pip install -e . pytest",
                ". .venv/bin/activate && pytest"
            ]
        );
        assert!(
            profile
                .toolchain_readiness
                .iter()
                .any(|item| item.starts_with("setup:python3_-m_venv_.venv"))
        );
        assert_eq!(
            required_setup_commands(&dir, "python"),
            vec![
                "python3 -m venv .venv && . .venv/bin/activate && python -m pip install -e . pytest"
            ]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn python_setup_py_only_repo_uses_python_pack() {
        let dir = temp_dir("python_setup_py");
        std::fs::write(
            dir.join("setup.py"),
            "from setuptools import setup\nsetup(name='canary')\n",
        )
        .expect("setup");

        let profile = profile_repo(&dir);

        assert_eq!(profile.primary_language, "python");
        assert_eq!(profile.language_pack_id, "python");
        assert_eq!(
            profile.suggested_checks,
            vec![
                "python3 -m venv .venv && . .venv/bin/activate && python -m pip install -e . pytest",
                ". .venv/bin/activate && pytest"
            ]
        );
        assert!(profile.detected_language_packs.contains(&"python"));
        assert!(profile.detected_files.contains(&"setup.py"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn unknown_repo_uses_generic_pack_without_inventing_a_check() {
        let dir = temp_dir("generic");
        std::fs::write(dir.join("README.md"), "# canary\n").expect("write");

        let profile = profile_repo(&dir);

        assert_eq!(profile.language_pack_id, "generic");
        assert_eq!(profile.detected_language_packs, vec!["generic"]);
        assert!(profile.suggested_checks.is_empty());
        assert!(profile.language_pack_guidance.is_empty());
        assert!(profile.toolchain_readiness.is_empty());
        assert_eq!(profile.proof_plan_status, "operator_check_required");
        assert!(
            profile
                .proof_plan_next_action
                .contains("Choose the check command")
        );
        assert!(profile.review_axes.contains(&"explicit-user-checks"));
        assert!(profile.artifact_patterns.contains(&"target/**"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn task_profile_promotes_mixed_repo_to_rust_for_rust_scope() {
        let dir = temp_dir("mixed_rust_task");
        std::fs::create_dir_all(dir.join("crates/mdx-server/src")).expect("mkdir");
        std::fs::write(
            dir.join("package.json"),
            r#"{"packageManager":"pnpm@9.15.9","scripts":{"test":"pnpm test"}}"#,
        )
        .expect("package");
        std::fs::write(
            dir.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/mdx-server\"]\n",
        )
        .expect("cargo");

        let default_profile = profile_repo(&dir);
        assert_eq!(default_profile.language_pack_id, "node");
        assert!(
            default_profile
                .detected_language_packs
                .contains(&"rust-cargo")
        );

        let rust_profile = profile_repo_for_task(
            &dir,
            "Fix crates/mdx-server/src/forge_loop_runner.rs",
            &["crates/mdx-server/src/**".to_string()],
            &["cargo check -p mdx-server".to_string()],
        );
        assert_eq!(rust_profile.primary_language, "rust");
        assert_eq!(rust_profile.language_pack_id, "rust-cargo");
        assert_eq!(
            rust_profile.suggested_checks,
            vec!["cargo check -p mdx-server"]
        );
        assert!(rust_profile.artifact_patterns.contains(&"target/**"));
        assert!(!rust_profile.artifact_patterns.contains(&"node_modules/**"));

        let node_profile = profile_repo_for_task(
            &dir,
            "Fix apps/mdx-host/src/routes/forge/+page.svelte",
            &["apps/mdx-host/src/**".to_string()],
            &["pnpm test".to_string()],
        );
        assert_eq!(node_profile.language_pack_id, "node");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn task_profile_promotes_mixed_repo_to_swift_for_native_macos_scope() {
        let dir = temp_dir("mixed_native_macos_task");
        std::fs::create_dir_all(dir.join("apps/mdx-operator-macos/Sources/MDxOperator"))
            .expect("mkdir macos app");
        std::fs::write(
            dir.join("package.json"),
            r#"{"packageManager":"pnpm@9.15.9","scripts":{"test":"pnpm test"}}"#,
        )
        .expect("package");
        std::fs::write(
            dir.join("apps/mdx-operator-macos/Package.swift"),
            "// swift-tools-version: 5.9\n",
        )
        .expect("package swift");

        let default_profile = profile_repo(&dir);
        assert_eq!(default_profile.language_pack_id, "node");
        assert!(
            default_profile
                .detected_language_packs
                .contains(&"swift-spm")
        );

        let native_profile = profile_repo_for_task(
            &dir,
            "Improve the SwiftUI macOS app run detail in apps/mdx-operator-macos",
            &["apps/mdx-operator-macos/**".to_string()],
            &["make native-macos-operator-check".to_string()],
        );
        assert_eq!(native_profile.primary_language, "swift");
        assert_eq!(native_profile.language_pack_id, "swift-spm");
        assert_eq!(
            native_profile.detected_files,
            vec!["apps/mdx-operator-macos/Package.swift"]
        );
        assert_eq!(
            native_profile.suggested_checks,
            vec!["make native-macos-operator-check"]
        );
        assert!(native_profile.artifact_patterns.contains(&".build/**"));
        assert!(
            !native_profile
                .artifact_patterns
                .contains(&"node_modules/**")
        );
        assert!(native_profile.proof_plan_summary.contains("swift-spm"));

        let node_profile = profile_repo_for_task(
            &dir,
            "Fix apps/mdx-host/src/routes/forge/+page.svelte",
            &["apps/mdx-host/src/**".to_string()],
            &["pnpm test".to_string()],
        );
        assert_eq!(node_profile.language_pack_id, "node");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn common_build_outputs_are_artifacts() {
        assert!(is_artifact_path(".build/checkouts/Foo/Sources/Foo.swift"));
        assert!(is_artifact_path("target/debug/deps/app"));
        assert!(is_artifact_path("src/app/.next/server/page.js"));
        assert!(is_artifact_path(
            ".venv/lib/python3.12/site-packages/pytest/__init__.py"
        ));
        assert!(is_artifact_path("canary.egg-info/PKG-INFO"));
        assert!(!is_artifact_path("Sources/App/Slug.swift"));
        assert!(!is_artifact_path("src/build_tools/mod.rs"));
    }

    #[test]
    fn readiness_names_are_stable_for_check_tokens() {
        assert_eq!(readiness_key("swift test"), "swift_test");
        assert_eq!(readiness_key("xcodebuild test"), "xcodebuild_test");
        assert_eq!(readiness_key("./gradlew test"), "gradlew_test");
        assert_eq!(
            readiness_key("./gradlew testDebugUnitTest"),
            "gradlew_testDebugUnitTest"
        );
        assert_eq!(readiness_key("go test ./..."), "go_test_...");
        assert_eq!(
            readiness_key("swift run CanaryCheck"),
            "swift_run_CanaryCheck"
        );
        assert_eq!(readiness_check_key("swift test"), "check:swift_test");
    }

    fn temp_dir(label: &str) -> std::path::PathBuf {
        static NEXT_TEMP_DIR_ID: std::sync::atomic::AtomicUsize =
            std::sync::atomic::AtomicUsize::new(0);
        let id = NEXT_TEMP_DIR_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "mdx_forge_repo_profile_{label}_{}_{}_{}",
            std::process::id(),
            line!(),
            id
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("mkdir");
        path
    }

    struct Canary {
        label: &'static str,
        marker: &'static str,
        contents: &'static str,
        pack: &'static str,
        check: &'static str,
        artifact: &'static str,
    }

    #[test]
    fn marker_scan_skips_build_output_and_dependencies() {
        // The source root has a real Cargo.toml; the only go.mod and pom.xml
        // live inside node_modules/ and target/. The scanner must find the
        // source marker and must NOT descend into ignored directories - for
        // correctness (a manifest in a dependency would misdetect the repo)
        // and to keep run admission fast (walking target/ on a big repo is the
        // 40s stall this guards against).
        let root = temp_dir("ignore_dirs");
        std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"x\"\n").expect("write");
        let buried_dep = root.join("node_modules").join("some-dep");
        std::fs::create_dir_all(&buried_dep).expect("mkdir");
        std::fs::write(buried_dep.join("go.mod"), "module dep\n").expect("write");
        let buried_build = root.join("target").join("debug");
        std::fs::create_dir_all(&buried_build).expect("mkdir");
        std::fs::write(buried_build.join("pom.xml"), "<project/>").expect("write");
        #[cfg(unix)]
        {
            let symlink_target = root.with_file_name("ignore_dirs_linked_dependency");
            let _ = std::fs::remove_dir_all(&symlink_target);
            std::fs::create_dir_all(&symlink_target).expect("mkdir");
            std::fs::write(symlink_target.join("go.mod"), "module linked\n").expect("write");
            std::os::unix::fs::symlink(&symlink_target, root.join("linked-node_modules"))
                .expect("symlink");
        }

        assert!(has_any_name_suffix(&root, "Cargo.toml"));
        assert!(!has_any_name_suffix(&root, "go.mod"));
        assert!(!has_any_name_suffix(&root, "pom.xml"));

        assert!(is_ignored_scan_dir(&root.join("node_modules")));
        assert!(is_ignored_scan_dir(&root.join("target")));
        assert!(is_ignored_scan_dir(&root.join(".git")));
        assert!(!is_ignored_scan_dir(&root.join("crates")));

        let profile = profile_repo(&root);
        assert!(profile.detected_language_packs.contains(&"rust-cargo"));
        assert!(!profile.detected_language_packs.contains(&"go"));

        let _ = std::fs::remove_dir_all(&root);
        #[cfg(unix)]
        {
            let _ = std::fs::remove_dir_all(root.with_file_name("ignore_dirs_linked_dependency"));
        }
    }
}
