pub mod artifact;
pub mod cli;
pub mod collect;
pub mod export;
pub mod select;

pub use artifact::FinalArtifact;
pub use collect::{CollectError, collect_final_artifacts, list_from_stream};
pub use export::{CopyWarning, ExportError, copy_final_artifacts};

use std::io::BufRead;
use std::path::Path;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CopyFromStreamError {
    #[error(transparent)]
    Collect(#[from] CollectError),
    #[error(transparent)]
    Export(#[from] ExportError),
}

pub fn copy_from_stream<R, P>(
    reader: R,
    out_dir: P,
) -> Result<Vec<CopyWarning>, CopyFromStreamError>
where
    R: BufRead,
    P: AsRef<Path>,
{
    let final_artifacts = collect_final_artifacts(reader)?;
    Ok(copy_final_artifacts(&final_artifacts, out_dir)?)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tempfile::TempDir;

    use super::*;

    fn build_finished(success: bool) -> String {
        json!({
            "reason": "build-finished",
            "success": success
        })
        .to_string()
    }

    #[test]
    fn copy_from_stream_does_not_create_output_directory_for_failed_build() {
        let temp = TempDir::new().unwrap();
        let out_dir = temp.path().join("artifacts");
        let input = format!("{}\n", build_finished(false));

        let error = copy_from_stream(input.as_bytes(), &out_dir).unwrap_err();

        assert!(matches!(error, CopyFromStreamError::Collect(_)));
        assert!(!out_dir.exists());
    }

    #[test]
    fn copy_from_stream_does_not_create_output_directory_for_truncated_stream() {
        let temp = TempDir::new().unwrap();
        let out_dir = temp.path().join("artifacts");

        let error = copy_from_stream("not json\n".as_bytes(), &out_dir).unwrap_err();

        assert!(matches!(error, CopyFromStreamError::Collect(_)));
        assert!(!out_dir.exists());
    }
}
