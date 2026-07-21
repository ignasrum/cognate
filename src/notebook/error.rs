use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotebookErrorKind {
    Validation,
    Storage,
    Recovery,
}

impl NotebookErrorKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Validation => "Validation",
            Self::Storage => "Storage",
            Self::Recovery => "Recovery",
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

    pub fn kind(&self) -> NotebookErrorKind {
        match self {
            Self::Validation { .. } => NotebookErrorKind::Validation,
            Self::Storage { .. } => NotebookErrorKind::Storage,
            Self::Recovery { .. } => NotebookErrorKind::Recovery,
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
        }
    }
}

pub trait EngineResultExt<T> {
    fn into_notebook_err(self) -> Result<T, NotebookError>;
}

impl<T> EngineResultExt<T> for Result<T, cognate_engine::EngineError> {
    fn into_notebook_err(self) -> Result<T, NotebookError> {
        self.map_err(NotebookError::from)
    }
}
