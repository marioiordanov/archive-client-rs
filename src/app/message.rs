use hyper::StatusCode;

use std::path::PathBuf;

use crate::{
    app::handlers::external_commands::FileWithRevision,
    screens,
    services::{
        auth::{AccessTokenResponse, RefreshTokenResponse},
        drive::DriveFile,
        file_index::FileIndex,
        org::{DashboardRowData, RootFolderEntry},
    },
    ui_error::{UiError, UiErrorKind},
};

#[derive(Debug)]
pub enum Message {
    Screen(ScreenMessage),
    Auth(AuthMessage),
    Org(OrgMessage),
    Sync(SyncMessage),
    UnixSocket(UnixSocketCommand),
}

#[derive(Debug)]
pub(crate) enum LoadingRevisions {
    Loading,
    Loaded(Vec<FileWithRevision>),
    Error,
}

#[derive(Debug)]
pub enum UnixSocketCommand {
    GetFileRevisions {
        path: PathBuf,
        force_refresh: bool,
        sender: Option<Box<tokio::sync::oneshot::Sender<LoadingRevisions>>>,
    },
    DownloadFileAtPath {
        file_id: String,
        revision_id: String,
        modified_time: String,
        sender: Box<tokio::sync::oneshot::Sender<String>>,
    },
    UnixCommandCompleted {
        command: Option<Box<UnixSocketCommand>>,
        error: Option<CommonServiceError>,
    },
}

#[derive(Debug)]
pub enum ScreenMessage {
    Login(screens::signin::Message),
    OrgSelection(screens::org_selection::Message),
    OrgDashboard(screens::org_dashboard::Message),
    OrgSync(screens::org_sync::Message),
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum OrgMessage {
    InvitationsLoaded(Result<Vec<crate::app::state::OrgInvitation>, OrgError>),
    OrgCreated(Result<RootFolderEntry, OrgError>),
    OrgJoined(Result<(), OrgError>),
    InviteSent(Result<(), OrgError>),
    InviteUserFinished {
        run_id: u64,
        email: String,
        result: Result<(String, String), OrgError>,
    },
    DashboardLoaded(Result<Vec<DashboardRowData>, OrgError>),
    PermissionRevoked {
        folder_id: String,
        result: Result<(), OrgError>,
    },
    AuditLogLoaded {
        result: Result<(Vec<crate::services::drive::Activity>, Option<String>), OrgError>,
    },
}

// always with absolute paths
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncAction {
    Upload(PathBuf),
    MoveAndUpload { from: PathBuf, to: PathBuf },
    Move { from: PathBuf, to: PathBuf },
    Delete(PathBuf),
    EnsureFolder(PathBuf),
    RemoveFolder(PathBuf),
    MoveFolder { from: PathBuf, to: PathBuf },
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum SyncMessage {
    InitialSyncCompleted {
        root_dir: PathBuf,
        result: Result<FileIndex, SyncError>,
    },
    /// Debounced + deduplicated actions emitted by the filesystem watcher subscription.
    ActionsReady(Vec<SyncAction>),

    BatchCompleted,

    /// Result of uploading a changed file.
    UploadFinished {
        path: PathBuf,
        result: Result<String, SyncError>,
    },

    MoveThenUploadFinished {
        from: PathBuf,
        to: PathBuf,
        result: Result<String, SyncError>,
    },

    MoveFinished {
        from_path: PathBuf,
        to_path: PathBuf,
        result: Result<String, SyncError>,
    },

    /// Result of creating or ensuring a folder exists.
    FolderEnsureFinished {
        path: PathBuf,
        result: Result<(), SyncError>,
    },

    /// Initial Folder creation
    FolderCreatedFinished {
        path: String,
        result: Result<DriveFile, SyncError>,
    },

    /// Result of opening a specific Drive revision.
    OpenRevisionFinished {
        path: String,
        result: Result<String, SyncError>,
    },
    RemoveFinished {
        path: PathBuf,
        object_was_on_remote: bool,
        result: Result<(), SyncError>,
    },
}

#[derive(Clone, Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum SyncError {
    #[error("Invalid local folder: {0}")]
    InvalidLocalFolder(String),

    #[error("I/O error: {0}")]
    Io(String),

    #[error("{0:?} doesnt exist on remote")]
    PathDoesNotExistOnRemote(PathBuf),

    #[error(transparent)]
    Common(#[from] CommonServiceError),
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum AuthMessage {
    AccessTokenReceived(Result<AccessTokenResponse, AuthError>),
    AccessTokenRefreshed(Result<RefreshTokenResponse, AuthError>),
    SignedOut,
}

#[derive(Clone, Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum AuthError {
    #[error("Sign-in cancelled by user")]
    CancelledByUser,

    #[error(transparent)]
    Common(#[from] CommonServiceError),
}

impl From<AuthError> for UiError {
    fn from(value: AuthError) -> Self {
        match value {
            AuthError::CancelledByUser => UiError {
                title: "Sign-in cancelled".into(),
                detail: None,
                kind: UiErrorKind::Info,
            },
            AuthError::Common(common) => common.into(),
        }
    }
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum CommonServiceError {
    #[error("Session expired")]
    TokenExpired(String),

    #[error("Permission denied")]
    PermissionDenied,

    #[error("Invalid response from server: {}", reason)]
    InvalidResponse { reason: String },

    #[error("Network error occurred")]
    NetworkError,

    #[error("Unknown error: {0}")]
    Unknown(String),
}

impl From<CommonServiceError> for UiError {
    fn from(value: CommonServiceError) -> Self {
        match value {
            CommonServiceError::TokenExpired(..) => UiError {
                title: "Session expired".into(),
                detail: Some("Please sign in again.".into()),
                kind: UiErrorKind::Warning,
            },
            CommonServiceError::PermissionDenied => UiError {
                title: "Permission required".into(),
                detail: Some("We need Google Drive access to archive files.".into()),
                kind: UiErrorKind::Warning,
            },
            CommonServiceError::InvalidResponse { .. } => UiError {
                title: "".into(),
                detail: Some("Malformed response. Contact the developer".into()),
                kind: UiErrorKind::Error,
            },
            CommonServiceError::NetworkError => UiError {
                title: "Connectivity error".into(),
                detail: Some("Please check you internet connection".into()),
                kind: UiErrorKind::Info,
            },
            CommonServiceError::Unknown(reason) => UiError {
                title: "".into(),
                detail: Some(reason),
                kind: UiErrorKind::Error,
            },
        }
    }
}

impl From<(reqwest::Error, String)> for CommonServiceError {
    fn from((e, access_token): (reqwest::Error, String)) -> Self {
        // If the error is associated with an HTTP status, classify it first.
        if let Some(status) = e.status() {
            return match status {
                StatusCode::UNAUTHORIZED => CommonServiceError::TokenExpired(access_token),
                // If you have a more specific variant (e.g., PermissionDenied), map it here.
                StatusCode::FORBIDDEN | StatusCode::NOT_FOUND | StatusCode::BAD_REQUEST => {
                    CommonServiceError::PermissionDenied
                }

                _ => CommonServiceError::InvalidResponse {
                    reason: "Contact the developer".to_string(),
                },
            };
        }

        // Transport-ish failures (no HTTP status available)
        if e.is_timeout() || e.is_connect() || e.is_request() {
            return CommonServiceError::NetworkError;
        }

        // Response decode/body issues, etc.
        CommonServiceError::InvalidResponse {
            reason: e.to_string(),
        }
    }
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum OrgError {
    #[error("Root folder not found")]
    NoRootFolder,
    #[error("Invalid email address")]
    InvalidEmailInvitation,
    #[error(transparent)]
    Common(#[from] CommonServiceError),
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum GlobalError {
    #[error(transparent)]
    AuthError(AuthError),
    #[error(transparent)]
    OrgError(OrgError),
    #[error(transparent)]
    SyncError(SyncError),
    #[error(transparent)]
    Common(CommonServiceError),
}

impl From<SyncError> for GlobalError {
    fn from(value: SyncError) -> Self {
        if let SyncError::Common(e) = value {
            GlobalError::Common(e)
        } else {
            GlobalError::SyncError(value)
        }
    }
}

impl From<AuthError> for GlobalError {
    fn from(value: AuthError) -> Self {
        if let AuthError::Common(e) = value {
            GlobalError::Common(e)
        } else {
            GlobalError::AuthError(value)
        }
    }
}

impl From<OrgError> for GlobalError {
    fn from(value: OrgError) -> Self {
        if let OrgError::Common(e) = value {
            GlobalError::Common(e)
        } else {
            GlobalError::OrgError(value)
        }
    }
}

impl From<(reqwest::Error, String)> for OrgError {
    fn from((error, access_token): (reqwest::Error, String)) -> Self {
        if let Some(status) = error.status() {
            match status {
                StatusCode::BAD_REQUEST => OrgError::InvalidEmailInvitation,
                _ => OrgError::Common(CommonServiceError::from((error, access_token))),
            }
        } else {
            OrgError::Common(CommonServiceError::from((error, access_token)))
        }
    }
}
