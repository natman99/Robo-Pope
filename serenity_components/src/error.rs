//! Framework errors. All fallible operations return [`Result`].

use std::fmt;

/// The error type for framework operations.
///
/// Errors carry the Page/Modal/execution context they occurred in so that
/// persisted-reference failures, missing values and Discord limit violations
/// are all explicit rather than panics.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// A Page with this ID was already registered.
    DuplicatePageId { page_id: String },
    /// A Modal with this ID was already registered.
    DuplicateModalId { modal_id: String },
    /// Two interactive components in one Page declared the same execution ID.
    DuplicateExecId { page_id: String, exec_id: String },
    /// An execution ID uses the reserved framework prefix (`__`).
    ReservedExecId { page_id: String, exec_id: String },
    /// No Page is registered under this ID.
    UnknownPage { page_id: String },
    /// No Modal is registered under this ID.
    UnknownModal { modal_id: String },
    /// The resolved Page/Modal has no execution with this ID.
    UnknownExec { def_id: String, exec_id: String },
    /// A Discord `custom_id` did not resolve to a stored execution record.
    UnknownCustomId { custom_id: String },
    /// The custom ID belongs to a Page/Modal whose data type does not match.
    DataMismatch { def_id: String },
    /// Persisted data could not be deserialized into the Page/Modal data type.
    Deserialize {
        def_id: String,
        source: serde_json::Error,
    },
    /// A value could not be serialized.
    Serialize { source: serde_json::Error },
    /// A resolved component or message violates a Discord limit.
    DiscordLimit {
        limit: &'static str,
        limit_value: usize,
        actual: usize,
        context: String,
    },
    /// A component configuration is invalid once resolved (e.g. a Section
    /// without an accessory, or an empty button row).
    InvalidComponent { kind: &'static str, detail: String },
    /// A single component cannot fit within Discord's limits.
    ///
    /// Pagination only ever splits between complete components; a component
    /// that does not fit on an empty page is an error unless it has an
    /// explicitly supported splitting strategy.
    ComponentTooLarge { kind: &'static str, detail: String },
    /// A Frame callback returned an error, or its contents could not be used.
    FrameFailed { page_id: String, source: Box<Error> },
    /// More than one paginating Frame in a single Page (not yet supported).
    MultipleFrames { page_id: String },
    /// A Frame produced more content than fits in one message, but the Page
    /// was not built with `.paginate()`.
    FrameNeedsPagination { page_id: String },
    /// A Modal submission is missing a value for the component ID.
    MissingSubmissionValue {
        modal_id: String,
        component_id: String,
    },
    /// A Modal submission value has the wrong shape for the requested type.
    InvalidSubmissionValue {
        modal_id: String,
        component_id: String,
        expected: &'static str,
    },
    /// A Discord API or model error.
    Discord { source: serenity::Error },
    /// An error raised by user code inside a callback.
    External {
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// A framework-generated error with a plain message.
    Message { message: String },
}

impl Error {
    /// Wrap an arbitrary user error.
    pub fn external(e: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::External {
            source: Box::new(e),
        }
    }

    /// A framework-generated message error.
    #[must_use]
    pub fn msg(msg: impl Into<String>) -> Self {
        Self::Message {
            message: msg.into(),
        }
    }

    pub(crate) fn limit(
        limit: &'static str,
        limit_value: usize,
        actual: usize,
        context: impl Into<String>,
    ) -> Self {
        Self::DiscordLimit {
            limit,
            limit_value,
            actual,
            context: context.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicatePageId { page_id } => {
                write!(f, "page '{page_id}' is already registered")
            }
            Self::DuplicateModalId { modal_id } => {
                write!(f, "modal '{modal_id}' is already registered")
            }
            Self::DuplicateExecId { page_id, exec_id } => {
                write!(
                    f,
                    "execution id '{exec_id}' is duplicated within page '{page_id}'"
                )
            }
            Self::ReservedExecId { page_id, exec_id } => {
                write!(
                    f,
                    "execution id '{exec_id}' in page '{page_id}' uses reserved prefix '__'"
                )
            }
            Self::UnknownPage { page_id } => write!(f, "no page registered as '{page_id}'"),
            Self::UnknownModal { modal_id } => write!(f, "no modal registered as '{modal_id}'"),
            Self::UnknownExec { def_id, exec_id } => {
                write!(f, "'{def_id}' has no execution '{exec_id}'")
            }
            Self::UnknownCustomId { custom_id } => {
                write!(f, "custom id '{custom_id}' is not a known execution")
            }
            Self::DataMismatch { def_id } => {
                write!(
                    f,
                    "registered definition '{def_id}' has a different data type"
                )
            }
            Self::Deserialize { def_id, source } => {
                write!(
                    f,
                    "persisted data for '{def_id}' failed to deserialize: {source}"
                )
            }
            Self::Serialize { source } => write!(f, "serialization failed: {source}"),
            Self::DiscordLimit {
                limit,
                limit_value,
                actual,
                context,
            } => {
                write!(
                    f,
                    "{context} exceeds Discord limit {limit} <= {limit_value} (got {actual})"
                )
            }
            Self::InvalidComponent { kind, detail } => {
                write!(f, "invalid {kind} component: {detail}")
            }
            Self::ComponentTooLarge { kind, detail } => {
                write!(
                    f,
                    "a {kind} component cannot fit within Discord's limits: {detail}"
                )
            }
            Self::FrameFailed { page_id, source } => {
                write!(f, "frame in page '{page_id}' failed: {source}")
            }
            Self::MultipleFrames { page_id } => {
                write!(
                    f,
                    "page '{page_id}' has more than one frame; only one is supported"
                )
            }
            Self::FrameNeedsPagination { page_id } => {
                write!(
                    f,
                    "frame in page '{page_id}' exceeds one message; call .paginate()"
                )
            }
            Self::MissingSubmissionValue {
                modal_id,
                component_id,
            } => {
                write!(
                    f,
                    "modal '{modal_id}' submission has no value for '{component_id}'"
                )
            }
            Self::InvalidSubmissionValue {
                modal_id,
                component_id,
                expected,
            } => {
                write!(
                    f,
                    "modal '{modal_id}' submission value '{component_id}' is not {expected}"
                )
            }
            Self::Discord { source } => write!(f, "discord error: {source}"),
            Self::External { source } => write!(f, "{source}"),
            Self::Message { message } => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Deserialize { source, .. } | Self::Serialize { source } => Some(source),
            Self::FrameFailed { source, .. } => Some(source),
            Self::Discord { source } => Some(source),
            Self::External { source } => Some(source.as_ref()),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(source: serde_json::Error) -> Self {
        Self::Serialize { source }
    }
}

impl From<serenity::Error> for Error {
    fn from(source: serenity::Error) -> Self {
        Self::Discord { source }
    }
}

/// The framework result alias.
pub type Result<T> = std::result::Result<T, Error>;
