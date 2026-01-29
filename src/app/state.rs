use core::fmt;

use serde::{Deserialize, Serialize};

use crate::screens::{self};
pub enum Screen {
    SignIn(screens::signin::SignInScreen),
    OrgSelection(screens::org_selection::OrgSelectionScreen),
}

impl fmt::Display for Screen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Screen::SignIn(_) => write!(f, "SignIn"),
            Screen::OrgSelection(_) => write!(f, "OrgSelection"),
        }
    }
}

#[derive(Clone, Copy)]
pub enum Intent {
    FetchInvitations,
    CreateOrg,
}

pub struct AppState {
    pub(crate) screen: Screen,
    pub(crate) session: SessionState,
    pub(crate) org: OrgState,

    pub retry_intent: Option<Intent>,
}

impl AppState {
    pub fn is_signed_in(&self) -> bool {
        self.session.auth == AuthState::SignedIn
    }

    pub fn is_org_created(&self) -> bool {
        self.org.status == OrgStatus::Ready
    }
}

#[derive(Default)]
pub struct SessionState {
    pub user: UserProfile,
    pub role: Option<Role>,
    pub auth: AuthState,
}

pub enum Role {
    Owner,
    User,
}

#[derive(Default, PartialEq, Eq)]
pub enum AuthState {
    #[default]
    SignedOut,
    SignedIn,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct UserProfile {
    pub email: String,
    pub access_token: String,
    pub scopes: Vec<String>,
    pub refresh_token: String,
    pub expires_at: u64,
    pub token_type: String,
}

#[derive(Default, Serialize, Deserialize)]
pub struct OrgState {
    pub config: OrgConfig,
    pub status: OrgStatus,
}

#[derive(Default, Serialize, Deserialize)]
pub struct OrgConfig {
    pub archive_folder_id: String,   // Drive folder ID (source of truth)
    pub archive_folder_name: String, // cached display
}

#[derive(Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrgStatus {
    #[default]
    Unknown,
    Loading,
    Ready,
}

#[derive(Debug, Clone)]
pub struct OrgInvitation {
    pub org_id: String,
    pub org_name: String,
    pub invited_by: String,
    pub invited_at: u64,
}
