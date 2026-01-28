use crate::{
    screens,
    services::auth::AccessTokenResponse,
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
}

#[derive(Clone, Debug)]
pub enum OrgMessage {
    InvitationsLoaded(Result<Vec<crate::app::state::OrgInvitation>, OrgError>),
    OrgCreated(Result<String, OrgError>),
    OrgJoined(Result<(), OrgError>),
}

#[derive(Clone, Debug)]
pub enum AuthMessage {
    AccessTokenReceived(Result<AccessTokenResponse, AuthError>),
    TokenRefreshed(Result<String, AuthError>),
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
    TokenExpired, // this to be moved to errors related to drive api service

    #[error("Permission denied")]
    PermissionDenied,

    #[error("Invalid response from server")]
    InvalidResponse,

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
            CommonServiceError::InvalidResponse => UiError {
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

#[derive(Clone, Debug, thiserror::Error)]
pub enum OrgError {
    #[error("Root folder not found")]
    NoRootFolder,
    #[error(transparent)]
    Common(#[from] CommonServiceError),
}
