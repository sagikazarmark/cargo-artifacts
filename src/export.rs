use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::artifact::FinalArtifact;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyWarning {
    pub destination_path: PathBuf,
    pub previous_source_path: String,
    pub current_source_path: String,
    pub previous_package_id: Option<String>,
    pub current_package_id: Option<String>,
    pub previous_cargo_target_name: Option<String>,
    pub current_cargo_target_name: Option<String>,
    pub previous_cargo_target_kinds: Vec<String>,
    pub current_cargo_target_kinds: Vec<String>,
}

impl fmt::Display for CopyWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "destination filename collision for {}: {} ({}) overwritten by {} ({})",
            self.destination_path.display(),
            self.previous_source_path,
            artifact_context(
                self.previous_package_id.as_deref(),
                self.previous_cargo_target_name.as_deref(),
                &self.previous_cargo_target_kinds,
            ),
            self.current_source_path,
            artifact_context(
                self.current_package_id.as_deref(),
                self.current_cargo_target_name.as_deref(),
                &self.current_cargo_target_kinds,
            )
        )
    }
}

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("source path has no filename: {source_path}")]
    MissingDestinationFilename { source_path: String },
    #[error("output path exists and is not a directory: {path}")]
    InvalidOutputDirectory { path: PathBuf },
    #[error("source Final Artifact does not exist: {path}")]
    MissingSource { path: PathBuf },
    #[error("failed to create output directory {path}: {source}")]
    CreateOutputDirectory { path: PathBuf, source: io::Error },
    #[error("failed to inspect source Final Artifact {path}: {source}")]
    InspectSource { path: PathBuf, source: io::Error },
    #[error("failed to replace destination {path}: {source}")]
    ReplaceDestination { path: PathBuf, source: io::Error },
    #[error("failed to copy Final Artifact from {source_path} to {destination_path}: {source}")]
    Copy {
        source_path: PathBuf,
        destination_path: PathBuf,
        source: io::Error,
    },
}

pub fn copy_final_artifacts<P>(
    artifacts: &[FinalArtifact],
    out_dir: P,
) -> Result<Vec<CopyWarning>, ExportError>
where
    P: AsRef<Path>,
{
    let out_dir = out_dir.as_ref();
    let copies = plan_copies(artifacts, out_dir)?;
    validate_or_create_output_directory(out_dir)?;

    let mut warnings = Vec::new();
    let mut first_by_destination = HashMap::<PathBuf, &FinalArtifact>::new();

    for (artifact, destination_path) in copies {
        if let Some(previous) = first_by_destination.get(&destination_path) {
            if previous.source_path != artifact.source_path {
                warnings.push(collision_warning(
                    previous,
                    artifact,
                    destination_path.clone(),
                ));
            }
        } else {
            first_by_destination.insert(destination_path.clone(), artifact);
        }

        copy_one(artifact, &destination_path)?;
    }

    Ok(warnings)
}

fn plan_copies<'a>(
    artifacts: &'a [FinalArtifact],
    out_dir: &Path,
) -> Result<Vec<(&'a FinalArtifact, PathBuf)>, ExportError> {
    artifacts
        .iter()
        .map(|artifact| {
            let destination_filename = artifact.destination_filename().ok_or_else(|| {
                ExportError::MissingDestinationFilename {
                    source_path: artifact.source_path.clone(),
                }
            })?;
            Ok((artifact, out_dir.join(destination_filename)))
        })
        .collect()
}

fn validate_or_create_output_directory(out_dir: &Path) -> Result<(), ExportError> {
    if out_dir.exists() && !out_dir.is_dir() {
        return Err(ExportError::InvalidOutputDirectory {
            path: out_dir.to_path_buf(),
        });
    }

    fs::create_dir_all(out_dir).map_err(|source| ExportError::CreateOutputDirectory {
        path: out_dir.to_path_buf(),
        source,
    })
}

fn copy_one(artifact: &FinalArtifact, destination_path: &Path) -> Result<(), ExportError> {
    let source_path = PathBuf::from(&artifact.source_path);
    let source_metadata = fs::metadata(&source_path).map_err(|source| {
        if source.kind() == io::ErrorKind::NotFound {
            ExportError::MissingSource {
                path: source_path.clone(),
            }
        } else {
            ExportError::InspectSource {
                path: source_path.clone(),
                source,
            }
        }
    })?;

    remove_existing_destination(destination_path)?;

    if source_metadata.is_dir() {
        copy_dir_recursive(&source_path, destination_path)
    } else {
        fs::copy(&source_path, destination_path)
            .map(|_| ())
            .map_err(|source| ExportError::Copy {
                source_path,
                destination_path: destination_path.to_path_buf(),
                source,
            })
    }
}

fn copy_dir_recursive(source_path: &Path, destination_path: &Path) -> Result<(), ExportError> {
    fs::create_dir(destination_path).map_err(|source| ExportError::Copy {
        source_path: source_path.to_path_buf(),
        destination_path: destination_path.to_path_buf(),
        source,
    })?;

    for entry in fs::read_dir(source_path).map_err(|source| ExportError::Copy {
        source_path: source_path.to_path_buf(),
        destination_path: destination_path.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| ExportError::Copy {
            source_path: source_path.to_path_buf(),
            destination_path: destination_path.to_path_buf(),
            source,
        })?;
        let child_source_path = entry.path();
        let child_destination_path = destination_path.join(entry.file_name());
        let child_metadata =
            fs::metadata(&child_source_path).map_err(|source| ExportError::InspectSource {
                path: child_source_path.clone(),
                source,
            })?;

        if child_metadata.is_dir() {
            copy_dir_recursive(&child_source_path, &child_destination_path)?;
        } else {
            fs::copy(&child_source_path, &child_destination_path)
                .map(|_| ())
                .map_err(|source| ExportError::Copy {
                    source_path: child_source_path,
                    destination_path: child_destination_path,
                    source,
                })?;
        }
    }

    Ok(())
}

fn remove_existing_destination(destination_path: &Path) -> Result<(), ExportError> {
    let Ok(metadata) = fs::symlink_metadata(destination_path) else {
        return Ok(());
    };

    let result = if metadata.is_dir() {
        fs::remove_dir_all(destination_path)
    } else {
        fs::remove_file(destination_path)
    };

    result.map_err(|source| ExportError::ReplaceDestination {
        path: destination_path.to_path_buf(),
        source,
    })
}

fn collision_warning(
    previous: &FinalArtifact,
    current: &FinalArtifact,
    destination_path: PathBuf,
) -> CopyWarning {
    CopyWarning {
        destination_path,
        previous_source_path: previous.source_path.clone(),
        current_source_path: current.source_path.clone(),
        previous_package_id: previous.package_id.clone(),
        current_package_id: current.package_id.clone(),
        previous_cargo_target_name: previous.cargo_target_name.clone(),
        current_cargo_target_name: current.cargo_target_name.clone(),
        previous_cargo_target_kinds: previous.cargo_target_kinds.clone(),
        current_cargo_target_kinds: current.cargo_target_kinds.clone(),
    }
}

fn artifact_context(
    package_id: Option<&str>,
    cargo_target_name: Option<&str>,
    cargo_target_kinds: &[String],
) -> String {
    let package_id = package_id.unwrap_or("unknown package");
    let cargo_target_name = cargo_target_name.unwrap_or("unknown Cargo Target");
    let cargo_target_kinds = if cargo_target_kinds.is_empty() {
        "unknown kind".to_owned()
    } else {
        cargo_target_kinds.join(",")
    };

    format!("package {package_id}, Cargo Target {cargo_target_name} [{cargo_target_kinds}]")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn artifact(path: &Path, target_name: &str) -> FinalArtifact {
        FinalArtifact::new(
            path.to_string_lossy().into_owned(),
            Some(format!("path+file:///workspace/{target_name}#0.1.0")),
            Some(target_name.to_owned()),
            vec!["bin".to_owned()],
            Some(false),
        )
    }

    #[test]
    fn creates_output_directory_for_successful_empty_copy() {
        let temp = TempDir::new().unwrap();
        let out_dir = temp.path().join("artifacts");

        let warnings = copy_final_artifacts(&[], &out_dir).unwrap();

        assert!(warnings.is_empty());
        assert!(out_dir.is_dir());
    }

    #[test]
    fn fails_when_output_destination_is_a_file() {
        let temp = TempDir::new().unwrap();
        let out_dir = temp.path().join("artifacts");
        fs::write(&out_dir, "not a directory").unwrap();

        let error = copy_final_artifacts(&[], &out_dir).unwrap_err();

        assert!(matches!(error, ExportError::InvalidOutputDirectory { .. }));
    }

    #[test]
    fn missing_source_paths_fail_clearly() {
        let temp = TempDir::new().unwrap();
        let missing_source = temp.path().join("target/debug/app");
        let out_dir = temp.path().join("artifacts");

        let error =
            copy_final_artifacts(&[artifact(&missing_source, "app")], &out_dir).unwrap_err();

        assert!(matches!(error, ExportError::MissingSource { .. }));
    }

    #[test]
    fn replaces_existing_file_destinations() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("target/debug/app");
        let out_dir = temp.path().join("artifacts");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::create_dir_all(&out_dir).unwrap();
        fs::write(&source, "new").unwrap();
        fs::write(out_dir.join("app"), "old").unwrap();

        copy_final_artifacts(&[artifact(&source, "app")], &out_dir).unwrap();

        assert_eq!(fs::read_to_string(out_dir.join("app")).unwrap(), "new");
    }

    #[test]
    fn replaces_existing_directory_destinations_recursively() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("target/debug/app.dSYM");
        let out_dir = temp.path().join("artifacts");
        fs::create_dir_all(source.join("Contents/Resources/DWARF")).unwrap();
        fs::write(source.join("Contents/Resources/DWARF/app"), "debug info").unwrap();
        fs::create_dir_all(out_dir.join("app.dSYM/stale")).unwrap();
        fs::write(out_dir.join("app.dSYM/stale/file"), "stale").unwrap();

        copy_final_artifacts(&[artifact(&source, "app")], &out_dir).unwrap();

        assert_eq!(
            fs::read_to_string(out_dir.join("app.dSYM/Contents/Resources/DWARF/app")).unwrap(),
            "debug info"
        );
        assert!(!out_dir.join("app.dSYM/stale").exists());
    }

    #[test]
    fn source_paths_without_filenames_fail() {
        let temp = TempDir::new().unwrap();
        let source = if cfg!(windows) { "C:\\\\" } else { "/" };
        let artifact = FinalArtifact::new(source, None, None, Vec::new(), None);

        let error = copy_final_artifacts(&[artifact], temp.path()).unwrap_err();

        assert!(matches!(
            error,
            ExportError::MissingDestinationFilename { .. }
        ));
    }

    #[test]
    fn collision_warnings_and_stream_order_overwrite_behavior() {
        let temp = TempDir::new().unwrap();
        let first_dir = temp.path().join("first/target/debug");
        let second_dir = temp.path().join("second/target/debug");
        let first_source = first_dir.join("app");
        let second_source = second_dir.join("app");
        let out_dir = temp.path().join("artifacts");
        fs::create_dir_all(&first_dir).unwrap();
        fs::create_dir_all(&second_dir).unwrap();
        fs::write(&first_source, "first").unwrap();
        fs::write(&second_source, "second").unwrap();

        let warnings = copy_final_artifacts(
            &[
                artifact(&first_source, "first"),
                artifact(&second_source, "second"),
            ],
            &out_dir,
        )
        .unwrap();

        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].previous_source_path,
            first_source.to_string_lossy()
        );
        assert_eq!(
            warnings[0].current_source_path,
            second_source.to_string_lossy()
        );
        assert_eq!(fs::read_to_string(out_dir.join("app")).unwrap(), "second");
    }

    #[cfg(unix)]
    #[test]
    fn preserves_executable_permissions_on_unix() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();
        let source = temp.path().join("target/debug/app");
        let out_dir = temp.path().join("artifacts");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(0o755)).unwrap();

        copy_final_artifacts(&[artifact(&source, "app")], &out_dir).unwrap();

        let mode = fs::metadata(out_dir.join("app"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o755);
    }
}
