use hyper::StatusCode;

use crate::{
    screens,
    services::{
        auth::{AccessTokenResponse, RefreshTokenResponse},
        org::{DashboardRowData, RootFolderEntry},
    },
    ui_error::{UiError, UiErrorKind},
};

#[derive(Clone, Debug)]
pub enum Message {
    Screen(ScreenMessage),
    Auth(AuthMessage),
    Org(OrgMessage),
}

#[derive(Clone, Debug)]
pub enum ScreenMessage {
    Login(screens::signin::Message),
    OrgSelection(screens::org_selection::Message),
    InviteMembers(screens::invite_members::Message),
    OrgDashboard(screens::org_dashboard::Message),
}

#[derive(Clone, Debug)]
pub enum OrgMessage {
    InvitationsLoaded(Result<Vec<crate::app::state::OrgInvitation>, OrgError>),
    OrgCreated(Result<RootFolderEntry, OrgError>),
    OrgJoined(Result<(), OrgError>),
    InviteSent(Result<(), OrgError>),
    InviteUserFinished {
        run_id: u64,
        email: String,
        result: Result<(), OrgError>,
    },
    DashboardLoaded(Result<Vec<DashboardRowData>, OrgError>),
    PermissionRevoked {
        folder_id: String,
        result: Result<(), OrgError>,
    },
}

#[derive(Clone, Debug)]
pub enum AuthMessage {
    AccessTokenReceived(Result<AccessTokenResponse, AuthError>),
    AccessTokenRefreshed(Result<RefreshTokenResponse, AuthError>),
    SignedOut,
}

#[derive(Clone, Debug, thiserror::Error)]
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
    TokenExpired,

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
            CommonServiceError::TokenExpired => UiError {
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

impl From<reqwest::Error> for CommonServiceError {
    fn from(e: reqwest::Error) -> Self {
        // If the error is associated with an HTTP status, classify it first.
        if let Some(status) = e.status() {
            return match status {
                StatusCode::UNAUTHORIZED => CommonServiceError::TokenExpired,
                // If you have a more specific variant (e.g., PermissionDenied), map it here.
                StatusCode::FORBIDDEN | StatusCode::NOT_FOUND | StatusCode::BAD_REQUEST => {
                    CommonServiceError::PermissionDenied
                }

                _ => CommonServiceError::InvalidResponse {
                    reason: format!("Contact the developer"),
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
    Common(CommonServiceError),
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
