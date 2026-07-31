use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotebookErrorKind {
    Initialization,
    Api,
    Validation,
    Storage,
    Recovery,
    Conflict,
    Search,
}

impl NotebookErrorKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Initialization => "Initialization",
            Self::Api => "API",
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
    Initialization {
        context: &'static str,
        detail: String,
    },
    #[error("{operation}: {detail}")]
    Api {
        operation: &'static str,
        status: Option<u16>,
        code: Option<String>,
        detail: String,
        retryable: bool,
    },
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
    pub fn initialization(context: &'static str, detail: impl Into<String>) -> Self {
        Self::Initialization {
            context,
            detail: detail.into(),
        }
    }

    pub fn api(
        operation: &'static str,
        status: Option<u16>,
        code: Option<String>,
        detail: impl Into<String>,
        retryable: bool,
    ) -> Self {
        Self::Api {
            operation,
            status,
            code,
            detail: detail.into(),
            retryable,
        }
    }

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
            Self::Initialization { .. } => NotebookErrorKind::Initialization,
            Self::Api { .. } => NotebookErrorKind::Api,
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

#[cfg(test)]
mod tests {
    use super::{NotebookError, NotebookErrorKind};

    #[test]
    fn initialization_errors_preserve_embedded_api_context() {
        let error = NotebookError::initialization(
            "embedded API",
            "Could not start embedded API: failed to bind loopback",
        );
        assert_eq!(error.kind(), NotebookErrorKind::Initialization);
        assert!(error.ui_message().contains("failed to bind loopback"));
    }

    #[test]
    fn api_errors_preserve_status_code_and_retryability() {
        let error = NotebookError::api(
            "load notes",
            Some(503),
            Some("search_unavailable".to_string()),
            "service unavailable",
            true,
        );
        assert!(matches!(
            error,
            NotebookError::Api {
                status: Some(503),
                retryable: true,
                ..
            }
        ));
    }
}
