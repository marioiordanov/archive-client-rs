use crate::{AccessTokenResponse, screens::signin::SignInScreen};

pub(crate) mod signin;

pub(crate) enum Screen {
    SignIn(SignInScreen),
}
