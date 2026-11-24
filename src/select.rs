use std::collections::HashSet;

use crate::artifact::FinalArtifact;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilerArtifactCandidate {
    pub source_path: String,
    pub package_id: Option<String>,
    pub cargo_target_name: Option<String>,
    pub cargo_target_kinds: Vec<String>,
    pub fresh: Option<bool>,
}

pub fn select_final_artifacts(
    candidates: impl IntoIterator<Item = CompilerArtifactCandidate>,
) -> Vec<FinalArtifact> {
    let mut seen_source_paths = HashSet::new();
    let mut final_artifacts = Vec::new();

    for candidate in candidates {
        if !is_final_artifact_candidate(&candidate) {
            continue;
        }

        if !seen_source_paths.insert(candidate.source_path.clone()) {
            continue;
        }

        final_artifacts.push(FinalArtifact::new(
            candidate.source_path,
            candidate.package_id,
            candidate.cargo_target_name,
            candidate.cargo_target_kinds,
            candidate.fresh,
        ));
    }

    final_artifacts
}

fn is_final_artifact_candidate(candidate: &CompilerArtifactCandidate) -> bool {
    if candidate
        .cargo_target_kinds
        .iter()
        .any(|kind| kind == "custom-build")
    {
        return false;
    }

    is_final_artifact_path(&candidate.source_path)
}

pub fn is_final_artifact_path(path: &str) -> bool {
    let components = split_path_components(path);
    if components.len() < 3 {
        return false;
    }

    let Some(file_name) = components.last() else {
        return false;
    };
    if is_internal_file(file_name) {
        return false;
    }

    let file_index = components.len() - 1;
    let target_index = (file_index >= 2 && components[file_index - 1] == "examples")
        .then(|| public_profile_output_target(&components, file_index - 2))
        .flatten()
        .or_else(|| public_profile_output_target(&components, file_index - 1));

    // Only the target directory's own layout decides: the directories above it (a checkout under
    // `build/`, Nix's `/build` sandbox) are not Cargo's.
    target_index.is_some_and(|target_index| {
        !components[target_index + 1..file_index]
            .iter()
            .any(|component| is_internal_dir(component))
    })
}

fn split_path_components(path: &str) -> Vec<&str> {
    path.split(['/', '\\'])
        .filter(|component| !component.is_empty())
        .collect()
}

/// The index of the `target` directory when `profile_index` is a public profile output directory
/// (`target/<profile>` or `target/<compilation target>/<profile>`).
fn public_profile_output_target(components: &[&str], profile_index: usize) -> Option<usize> {
    let profile = components.get(profile_index)?;
    if !is_plausible_profile_dir(profile) {
        return None;
    }

    if profile_index >= 1 && components[profile_index - 1] == "target" {
        return Some(profile_index - 1);
    }

    (profile_index >= 2
        && components[profile_index - 2] == "target"
        && is_plausible_compilation_target_dir(components[profile_index - 1]))
    .then(|| profile_index - 2)
}

fn is_plausible_profile_dir(component: &str) -> bool {
    !component.is_empty()
        && !component.starts_with('.')
        && !matches!(component, "doc" | "package" | "tmp")
}

fn is_plausible_compilation_target_dir(component: &str) -> bool {
    component.contains('-') || component.ends_with(".json")
}

fn is_internal_dir(component: &str) -> bool {
    matches!(component, "deps" | "build" | ".fingerprint" | "incremental")
}

fn is_internal_file(file_name: &str) -> bool {
    file_name.ends_with(".rmeta") || file_name.ends_with(".d")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(path: &str) -> CompilerArtifactCandidate {
        CompilerArtifactCandidate {
            source_path: path.to_owned(),
            package_id: Some("path+file:///workspace/app#0.1.0".to_owned()),
            cargo_target_name: Some("app".to_owned()),
            cargo_target_kinds: vec!["bin".to_owned()],
            fresh: Some(false),
        }
    }

    fn selected(paths: &[&str]) -> Vec<String> {
        select_final_artifacts(paths.iter().map(|path| candidate(path)))
            .into_iter()
            .map(|artifact| artifact.source_path)
            .collect()
    }

    #[test]
    fn includes_direct_profile_output_paths() {
        assert_eq!(
            selected(&[
                "/workspace/target/debug/app",
                "/workspace/target/release/libapp.rlib",
            ]),
            vec![
                "/workspace/target/debug/app",
                "/workspace/target/release/libapp.rlib",
            ]
        );
    }

    #[test]
    fn includes_compilation_target_profile_output_paths() {
        assert_eq!(
            selected(&["/workspace/target/x86_64-unknown-linux-gnu/debug/app"]),
            vec!["/workspace/target/x86_64-unknown-linux-gnu/debug/app"]
        );
    }

    #[test]
    fn includes_examples_output_paths() {
        assert_eq!(
            selected(&[
                "/workspace/target/debug/examples/demo",
                "/workspace/target/x86_64-unknown-linux-gnu/release/examples/demo",
            ]),
            vec![
                "/workspace/target/debug/examples/demo",
                "/workspace/target/x86_64-unknown-linux-gnu/release/examples/demo",
            ]
        );
    }

    #[test]
    fn excludes_dependency_and_internal_paths() {
        assert_eq!(
            selected(&[
                "/workspace/target/debug/deps/libdep.rlib",
                "/workspace/target/debug/build/app/build-script-build",
                "/workspace/target/debug/.fingerprint/app/lib-app.json",
                "/workspace/target/debug/incremental/app/s-cache",
            ]),
            Vec::<String>::new()
        );
    }

    #[test]
    fn includes_paths_whose_ancestors_are_named_like_internal_dirs() {
        // Nix's Linux sandbox builds in /build; a checkout can sit anywhere.
        assert_eq!(
            selected(&[
                "/build/source/target/debug/app",
                "/home/me/deps/app/target/x86_64-unknown-linux-gnu/release/app",
                "/srv/incremental/.fingerprint/target/debug/examples/demo",
            ]),
            vec![
                "/build/source/target/debug/app",
                "/home/me/deps/app/target/x86_64-unknown-linux-gnu/release/app",
                "/srv/incremental/.fingerprint/target/debug/examples/demo",
            ]
        );
    }

    #[test]
    fn excludes_internal_paths_under_an_ancestor_named_build() {
        assert_eq!(
            selected(&[
                "/build/source/target/debug/deps/libdep.rlib",
                "/build/source/target/debug/build/app/build-script-build",
            ]),
            Vec::<String>::new()
        );
    }

    #[test]
    fn excludes_rmeta_and_depinfo_files() {
        assert_eq!(
            selected(&[
                "/workspace/target/debug/libapp.rmeta",
                "/workspace/target/debug/app.d",
            ]),
            Vec::<String>::new()
        );
    }

    #[test]
    fn excludes_documentation_output() {
        assert_eq!(
            selected(&["/workspace/target/doc/app/index.html"]),
            Vec::<String>::new()
        );
    }

    #[test]
    fn excludes_test_executables_under_deps() {
        assert_eq!(
            selected(&["/workspace/target/debug/deps/app_test-123456"]),
            Vec::<String>::new()
        );
    }

    #[test]
    fn excludes_custom_build_targets_even_if_path_matches() {
        let mut build_script = candidate("/workspace/target/debug/build-script-build");
        build_script.cargo_target_kinds = vec!["custom-build".to_owned()];

        assert!(select_final_artifacts([build_script]).is_empty());
    }

    #[test]
    fn includes_platform_and_sbom_companions_alongside_final_artifacts() {
        assert_eq!(
            selected(&[
                "/workspace/target/debug/app.dSYM",
                "/workspace/target/debug/app.pdb",
                "/workspace/target/debug/app.cargo-sbom.json",
                "/workspace/target/debug/app.dll.lib",
            ]),
            vec![
                "/workspace/target/debug/app.dSYM",
                "/workspace/target/debug/app.pdb",
                "/workspace/target/debug/app.cargo-sbom.json",
                "/workspace/target/debug/app.dll.lib",
            ]
        );
    }

    #[test]
    fn preserves_first_seen_order_after_deduplicating_exact_source_paths() {
        assert_eq!(
            selected(&[
                "/workspace/target/debug/app",
                "/workspace/target/debug/helper",
                "/workspace/target/debug/app",
            ]),
            vec![
                "/workspace/target/debug/app",
                "/workspace/target/debug/helper",
            ]
        );
    }

    #[test]
    fn accepts_windows_style_separators_for_saved_logs() {
        assert!(is_final_artifact_path(
            r#"C:\workspace\target\debug\app.exe"#
        ));
        assert!(!is_final_artifact_path(
            r#"C:\workspace\target\debug\deps\app.exe"#
        ));
    }
}
