use crate::screens::{self};

pub enum Screen {
    SignIn(screens::signin::SignInScreen),
    GetOrCreateOrganisation,
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
    email: String,
    access_token: String,
    scopes: Vec<String>,
    refresh_token: String,
    expires_at: u32,
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
