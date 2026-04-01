use std::fmt;
use std::path::{Component, Path, PathBuf};

use super::NotebookError;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NotebookRelativePath(String);

impl NotebookRelativePath {
    pub fn parse(path_kind: &'static str, value: &str) -> Result<Self, NotebookError> {
        if value.trim().is_empty() {
            return Err(NotebookError::validation(
                "path validation",
                format!("Invalid {} '{}': path cannot be empty.", path_kind, value),
            ));
        }

        let mut has_normal_component = false;
        for component in Path::new(value).components() {
            match component {
                Component::Normal(_) => has_normal_component = true,
                Component::CurDir => {
                    return Err(NotebookError::validation(
                        "path validation",
                        format!(
                            "Invalid {} '{}': '.' path components are not allowed.",
                            path_kind, value
                        ),
                    ));
                }
                Component::ParentDir => {
                    return Err(NotebookError::validation(
                        "path validation",
                        format!(
                            "Invalid {} '{}': '..' path components are not allowed.",
                            path_kind, value
                        ),
                    ));
                }
                Component::RootDir | Component::Prefix(_) => {
                    return Err(NotebookError::validation(
                        "path validation",
                        format!(
                            "Invalid {} '{}': absolute paths are not allowed.",
                            path_kind, value
                        ),
                    ));
                }
            }
        }

        if !has_normal_component {
            return Err(NotebookError::validation(
                "path validation",
                format!(
                    "Invalid {} '{}': path must contain at least one normal component.",
                    path_kind, value
                ),
            ));
        }

        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }

    pub fn join_under(&self, root: &Path) -> PathBuf {
        root.join(self.as_path())
    }

    pub fn into_string(self) -> String {
        self.0
    }

    pub fn sanitized_for_temp_name(&self) -> String {
        self.as_path()
            .components()
            .filter_map(|component| match component {
                Component::Normal(component) => Some(component.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect::<Vec<String>>()
            .join("__")
    }
}

impl AsRef<str> for NotebookRelativePath {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for NotebookRelativePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
