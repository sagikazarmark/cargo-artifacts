#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalArtifact {
    pub source_path: String,
    pub destination_filename: Option<String>,
    pub package_id: Option<String>,
    pub cargo_target_name: Option<String>,
    pub cargo_target_kinds: Vec<String>,
    pub fresh: Option<bool>,
}

impl FinalArtifact {
    pub fn new(
        source_path: impl Into<String>,
        package_id: Option<String>,
        cargo_target_name: Option<String>,
        cargo_target_kinds: Vec<String>,
        fresh: Option<bool>,
    ) -> Self {
        let source_path = source_path.into();
        let destination_filename = file_name_from_path(&source_path).map(ToOwned::to_owned);

        Self {
            source_path,
            destination_filename,
            package_id,
            cargo_target_name,
            cargo_target_kinds,
            fresh,
        }
    }

    pub fn source_path(&self) -> &str {
        &self.source_path
    }

    pub fn destination_filename(&self) -> Option<&str> {
        self.destination_filename.as_deref()
    }
}

pub(crate) fn file_name_from_path(path: &str) -> Option<&str> {
    let trimmed = path.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() {
        return None;
    }

    trimmed
        .rsplit(['/', '\\'])
        .next()
        .filter(|file_name| !file_name.is_empty())
}
