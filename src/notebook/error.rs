use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotebookErrorKind {
    Validation,
    Storage,
    Recovery,
    Conflict,
    Search,
}

impl NotebookErrorKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Validation => "Validation",
            Self::Storage => "Storage",
            Self::Recovery => "Recovery",
            Self::Conflict => "Conflict",
            Self::Search => "Search",
        }
    }
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum NotebookError {
    #[error("{context}: {detail}")]
    Validation {
        context: &'static str,
        detail: String,
    },
    #[error("{context}: {detail}")]
    Storage {
        context: &'static str,
        detail: String,
    },
    #[error("{context}: {detail}")]
    Recovery {
        context: &'static str,
        detail: String,
    },
    #[error("{context}: {detail}")]
    Search {
        context: &'static str,
        detail: String,
    },
    #[error("{context}: server revision {server_revision} conflicts with the local draft")]
    Conflict {
        context: &'static str,
        note_path: Option<String>,
        local_content: String,
        server_content: String,
        server_revision: String,
    },
}

#[allow(dead_code)]
impl NotebookError {
    pub fn validation(context: &'static str, detail: impl Into<String>) -> Self {
        Self::Validation {
            context,
            detail: detail.into(),
        }
    }

    pub fn storage(context: &'static str, detail: impl Into<String>) -> Self {
        Self::Storage {
            context,
            detail: detail.into(),
        }
    }

    pub fn recovery(context: &'static str, detail: impl Into<String>) -> Self {
        Self::Recovery {
            context,
            detail: detail.into(),
        }
    }

    pub fn search(context: &'static str, detail: impl Into<String>) -> Self {
        Self::Search {
            context,
            detail: detail.into(),
        }
    }

    pub fn conflict(
        context: &'static str,
        local_content: impl Into<String>,
        server_content: impl Into<String>,
        server_revision: impl Into<String>,
    ) -> Self {
        Self::Conflict {
            context,
            note_path: None,
            local_content: local_content.into(),
            server_content: server_content.into(),
            server_revision: server_revision.into(),
        }
    }

    pub fn conflict_for_note(
        context: &'static str,
        note_path: impl Into<String>,
        local_content: impl Into<String>,
        server_content: impl Into<String>,
        server_revision: impl Into<String>,
    ) -> Self {
        Self::Conflict {
            context,
            note_path: Some(note_path.into()),
            local_content: local_content.into(),
            server_content: server_content.into(),
            server_revision: server_revision.into(),
        }
    }

    pub fn kind(&self) -> NotebookErrorKind {
        match self {
            Self::Validation { .. } => NotebookErrorKind::Validation,
            Self::Storage { .. } => NotebookErrorKind::Storage,
            Self::Recovery { .. } => NotebookErrorKind::Recovery,
            Self::Conflict { .. } => NotebookErrorKind::Conflict,
            Self::Search { .. } => NotebookErrorKind::Search,
        }
    }

    pub fn ui_message(&self) -> String {
        format!("{} error: {}", self.kind().label(), self)
    }
}

impl From<cognate_engine::EngineError> for NotebookError {
    fn from(err: cognate_engine::EngineError) -> Self {
        match err {
            cognate_engine::EngineError::Serialization(msg) => {
                NotebookError::storage("serialization", msg)
            }
            cognate_engine::EngineError::Deserialization(msg) => {
                NotebookError::storage("deserialization", msg)
            }
            cognate_engine::EngineError::Validation { context, detail } => {
                NotebookError::Validation { context, detail }
            }
            cognate_engine::EngineError::Storage { context, detail } => {
                NotebookError::Storage { context, detail }
            }
            cognate_engine::EngineError::Recovery { context, detail } => {
                NotebookError::Recovery { context, detail }
            }
            cognate_engine::EngineError::LockUnavailable {
                context,
                resource,
                detail,
            } => NotebookError::Storage {
                context,
                detail: format!("{resource}: {detail}"),
            },
            cognate_engine::EngineError::Conflict { context, detail } => {
                NotebookError::Storage { context, detail }
            }
        }
    }
}
