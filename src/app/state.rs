use crate::screens::{self};

pub enum Screen {
    SignIn(screens::signin::SignInScreen),
    OrgSelection(screens::org_selection::OrgSelectionScreen),
    ListFiles,
    Syncing,
}

pub struct AppState {
    pub(crate) screen: Screen,
    pub(crate) session: SessionState,
    pub(crate) org: OrgState,
}

pub struct SessionState {
    pub user: Option<UserProfile>,
    pub role: Option<Role>,
    pub auth: AuthState,
}

pub enum Role {
    Owner,
    User,
}

pub enum AuthState {
    SignedOut,
    SignedIn,
}

pub struct UserProfile {
    pub email: String,
    pub access_token: String,
    pub scopes: Vec<String>,
    pub refresh_token: String,
    pub expires_at: u64,
    pub token_type: String,
}

pub struct OrgState {
    pub config: Option<OrgConfig>,
    pub status: OrgStatus,
}

pub struct OrgConfig {
    pub archive_folder_id: String,   // Drive folder ID (source of truth)
    pub archive_folder_name: String, // cached display
}

pub enum OrgStatus {
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
