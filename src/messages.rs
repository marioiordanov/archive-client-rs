use crate::AccessTokenResponse;

#[derive(Debug, Clone)]
pub(crate) enum Message {
    SignInPressed,
    SignInFinished(AccessTokenResponse),
}
