use std::collections::HashSet;
use std::io::{self, BufRead};

use cargo_metadata::{Artifact, Message};
use serde_json::Value;
use thiserror::Error;

use crate::artifact::FinalArtifact;
use crate::select::{CompilerArtifactCandidate, select_final_artifacts};

#[derive(Debug, Error)]
pub enum CollectError {
    #[error("failed to read Cargo Build Message Stream: {0}")]
    Io(#[from] io::Error),
    #[error("malformed JSON in Cargo Build Message Stream on line {line}: {source}")]
    MalformedJson {
        line: usize,
        source: serde_json::Error,
    },
    #[error("malformed Cargo Build Message Stream message on line {line}: {message}")]
    MalformedCargoMessage { line: usize, message: String },
    #[error("Cargo build did not finish successfully")]
    BuildFailed,
    #[error("Cargo Build Message Stream ended before build-finished")]
    MissingBuildFinished,
}

pub fn list_from_stream<R: BufRead>(reader: R) -> Result<Vec<FinalArtifact>, CollectError> {
    collect_final_artifacts(reader)
}

pub fn collect_final_artifacts<R: BufRead>(reader: R) -> Result<Vec<FinalArtifact>, CollectError> {
    let mut candidates = Vec::new();
    let mut saw_build_finished = false;

    for (line_index, line) in reader.lines().enumerate() {
        let line_number = line_index + 1;
        let line = line?;
        let Some(message) = parse_cargo_message_line(&line, line_number)? else {
            continue;
        };

        match message {
            Message::CompilerArtifact(artifact) => {
                candidates.extend(candidates_from_artifact(artifact))
            }
            Message::BuildFinished(finished) => {
                saw_build_finished = true;
                if !finished.success {
                    return Err(CollectError::BuildFailed);
                }
                break;
            }
            Message::CompilerMessage(_)
            | Message::BuildScriptExecuted(_)
            | Message::TextLine(_) => {}
            _ => {}
        }
    }

    if !saw_build_finished {
        return Err(CollectError::MissingBuildFinished);
    }

    Ok(select_final_artifacts(candidates))
}

fn parse_cargo_message_line(
    line: &str,
    line_number: usize,
) -> Result<Option<Message>, CollectError> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || !looks_json_like(trimmed) {
        return Ok(None);
    }

    let value: Value =
        serde_json::from_str(line).map_err(|source| CollectError::MalformedJson {
            line: line_number,
            source,
        })?;

    let reason = value.get("reason").and_then(Value::as_str).ok_or_else(|| {
        CollectError::MalformedCargoMessage {
            line: line_number,
            message: "missing string reason field".to_owned(),
        }
    })?;

    if !is_known_cargo_message_reason(reason) {
        return Ok(None);
    }

    serde_json::from_value(value)
        .map(Some)
        .map_err(|source| CollectError::MalformedCargoMessage {
            line: line_number,
            message: source.to_string(),
        })
}

fn looks_json_like(line: &str) -> bool {
    line.starts_with('{') || line.starts_with('[')
}

fn is_known_cargo_message_reason(reason: &str) -> bool {
    matches!(
        reason,
        "compiler-artifact" | "compiler-message" | "build-script-executed" | "build-finished"
    )
}

fn candidates_from_artifact(artifact: Artifact) -> Vec<CompilerArtifactCandidate> {
    let mut seen = HashSet::new();
    let package_id = artifact.package_id.to_string();
    let cargo_target_name = artifact.target.name;
    let cargo_target_kinds = artifact
        .target
        .kind
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    artifact
        .filenames
        .into_iter()
        .map(|filename| filename.to_string())
        .filter(|source_path| seen.insert(source_path.clone()))
        .map(|source_path| CompilerArtifactCandidate {
            source_path,
            package_id: Some(package_id.clone()),
            cargo_target_name: Some(cargo_target_name.clone()),
            cargo_target_kinds: cargo_target_kinds.clone(),
            fresh: Some(artifact.fresh),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn compiler_artifact(filenames: &[&str]) -> String {
        json!({
            "reason": "compiler-artifact",
            "package_id": "path+file:///workspace/app#0.1.0",
            "target": {
                "kind": ["bin"],
                "crate_types": ["bin"],
                "name": "app",
                "src_path": "/workspace/app/src/main.rs",
                "edition": "2024",
                "doc": true,
                "doctest": false,
                "test": true
            },
            "profile": {
                "opt_level": "0",
                "debuginfo": 2,
                "debug_assertions": true,
                "overflow_checks": true,
                "test": false
            },
            "features": [],
            "filenames": filenames,
            "executable": null,
            "fresh": false
        })
        .to_string()
    }

    fn build_finished(success: bool) -> String {
        json!({
            "reason": "build-finished",
            "success": success
        })
        .to_string()
    }

    fn collect(input: &str) -> Result<Vec<FinalArtifact>, CollectError> {
        collect_final_artifacts(input.as_bytes())
    }

    #[test]
    fn collects_final_artifacts_after_successful_build() {
        let input = format!(
            "{}\n{}\n",
            compiler_artifact(&["/workspace/target/debug/app"]),
            build_finished(true)
        );

        let artifacts = collect(&input).unwrap();

        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].source_path(), "/workspace/target/debug/app");
    }

    #[test]
    fn failed_build_fails_without_artifacts() {
        let input = format!(
            "{}\n{}\n",
            compiler_artifact(&["/workspace/target/debug/app"]),
            build_finished(false)
        );

        assert!(matches!(collect(&input), Err(CollectError::BuildFailed)));
    }

    #[test]
    fn missing_build_finished_fails() {
        let input = format!("{}\n", compiler_artifact(&["/workspace/target/debug/app"]));

        assert!(matches!(
            collect(&input),
            Err(CollectError::MissingBuildFinished)
        ));
    }

    #[test]
    fn malformed_json_looking_lines_fail() {
        let input = format!("{{not json}}\n{}\n", build_finished(true));

        assert!(matches!(
            collect(&input),
            Err(CollectError::MalformedJson { line: 1, .. })
        ));
    }

    #[test]
    fn non_json_text_lines_are_ignored() {
        let input = format!(
            "   Compiling app v0.1.0\n{}\n{}\n",
            compiler_artifact(&["/workspace/target/debug/app"]),
            build_finished(true)
        );

        let artifacts = collect(&input).unwrap();

        assert_eq!(artifacts.len(), 1);
    }

    #[test]
    fn unknown_well_formed_messages_are_ignored() {
        let input = format!(
            "{}\n{}\n{}\n",
            json!({"reason": "future-cargo-message", "ok": true}),
            compiler_artifact(&["/workspace/target/debug/app"]),
            build_finished(true)
        );

        let artifacts = collect(&input).unwrap();

        assert_eq!(artifacts.len(), 1);
    }

    #[test]
    fn stops_at_first_build_finished() {
        let input = format!(
            "{}\n{}\n{}\n",
            compiler_artifact(&["/workspace/target/debug/app"]),
            build_finished(true),
            compiler_artifact(&["/workspace/target/debug/after"]),
        );

        let artifacts = collect(&input).unwrap();

        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].source_path(), "/workspace/target/debug/app");
    }

    #[test]
    fn successful_empty_result_is_valid() {
        let input = format!("{}\n", build_finished(true));

        let artifacts = collect(&input).unwrap();

        assert!(artifacts.is_empty());
    }

    #[test]
    fn deduplicates_source_paths_in_first_seen_order() {
        let input = format!(
            "{}\n{}\n{}\n",
            compiler_artifact(&["/workspace/target/debug/app"]),
            compiler_artifact(&[
                "/workspace/target/debug/helper",
                "/workspace/target/debug/app",
            ]),
            build_finished(true)
        );

        let artifacts = collect(&input).unwrap();
        let source_paths = artifacts
            .iter()
            .map(FinalArtifact::source_path)
            .collect::<Vec<_>>();

        assert_eq!(
            source_paths,
            vec![
                "/workspace/target/debug/app",
                "/workspace/target/debug/helper"
            ]
        );
    }
}
