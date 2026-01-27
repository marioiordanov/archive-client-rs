use crate::{
    screens,
    services::auth::AccessTokenResponse,
    ui_error::{UiError, UiErrorKind},
};

#[derive(Clone, Debug)]
pub enum Message {
    Screen(ScreenMessage),
    Auth(AuthMessage),
}

#[derive(Clone, Debug)]
pub enum ScreenMessage {
    Login(screens::signin::Message),
}

#[derive(Clone, Debug)]
pub enum AuthMessage {
    AccessTokenReceived(Result<AccessTokenResponse, AuthError>),
    TokenRefreshed(Result<String, AuthError>),
    SignedOut,
}

#[derive(Clone, Debug)]
pub enum AuthError {
    CancelledByUser,
    TokenExpired,
    PermissionDenied,
}

impl From<AuthError> for UiError {
    fn from(value: AuthError) -> Self {
        match value {
            AuthError::CancelledByUser => UiError {
                title: "Sign-in cancelled".into(),
                detail: None,
                kind: UiErrorKind::Info,
            },
            AuthError::TokenExpired => UiError {
                title: "Session expired".into(),
                detail: Some("Please sign in again.".into()),
                kind: UiErrorKind::Warning,
            },
            AuthError::PermissionDenied => UiError {
                title: "Permission required".into(),
                detail: Some("We need Google Drive access to archive files.".into()),
                kind: UiErrorKind::Warning,
            },
        }
    }
}
