use thiserror::Error;

#[allow(dead_code)]
#[derive(Error, Debug)]
pub enum ZforgeError {
    #[error("config not found — run: zf init")]
    ConfigNotFound,

    #[error("task '{0}' not found")]
    TaskNotFound(String),

    #[error("task '{0}' already exists")]
    TaskAlreadyExists(String),

    #[error("invalid task ID '{0}'. Format: PROJECT-NUMBER (e.g. TASK-123)")]
    InvalidTaskId(String),

    #[error("invalid state transition from {from} to {to}")]
    InvalidTransition { from: String, to: String },

    #[error("state {state} insufficient — need {required}\nRun: {hint}")]
    StateInsufficient {
        state: String,
        required: String,
        hint: String,
    },

    #[error("artifact '{0}' not found")]
    ArtifactNotFound(String),

    #[error("unknown artifact '{0}'. Valid: testspec, plan, verify")]
    UnknownArtifact(String),

    #[error("template not found: {0}")]
    TemplateNotFound(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
