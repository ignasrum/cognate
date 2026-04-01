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
