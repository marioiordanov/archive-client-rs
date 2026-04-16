use core::fmt;
use std::{collections::HashMap, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    UserState,
    screens::{self},
    services::file_index::FileIndex,
};
pub enum Screen {
    SignIn(screens::signin::SignInScreen),
    OrgSelection(screens::org_selection::OrgSelectionScreen),
    OrgDashboard(screens::org_dashboard::OrgDashboardScreen),
    OrgSync(screens::org_sync::OrgSyncScreen),
}

impl fmt::Display for Screen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Screen::SignIn(_) => write!(f, "SignIn"),
            Screen::OrgSelection(_) => write!(f, "OrgSelection"),
            Screen::OrgDashboard(_) => write!(f, "OrgDashboard"),
            Screen::OrgSync(_) => write!(f, "OrgSync"),
        }
    }
}

#[derive(Clone)]
pub enum Intent {
    FetchInvitations,
    CreateOrg,
    SendInvitations {
        run_id: u64,
        org_id: String,
        email: String,
    },
    LoadDashboard {
        org_id: String,
    },
    InitialSync {
        root_dir: PathBuf,
        root_dir_id: String,
        progress: HashMap<PathBuf, String>,
    },
}

pub struct AppState {
    pub(crate) session: SessionState,
    pub(crate) org: OrgState,
    pub(crate) index: FileIndex,

    pub(crate) user_state: UserState,
    pub retry_intent: Option<Intent>,
}

impl AppState {
    pub fn is_signed_in(&self) -> bool {
        self.session.auth == AuthState::SignedIn
    }

    pub fn is_org_ready(&self) -> bool {
        self.org.status == OrgStatus::Ready
    }

    pub fn is_org_created(&self) -> bool {
        self.org.status == OrgStatus::Created
    }

    pub fn get_org_id(&self) -> &str {
        &self.org.config.archive_folder_id
    }

    pub fn get_access_token(&self) -> &str {
        &self.session.user.access_token
    }

    pub fn get_local_folder(&self) -> Option<PathBuf> {
        self.org
            .config
            .local_folder_path
            .as_ref()
            .map(|v| PathBuf::from(v))
    }
}

#[derive(Default)]
pub struct SessionState {
    pub user: UserProfile,
    pub auth: AuthState,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
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

    #[serde(default)]
    pub role: Option<Role>,
}

pub struct UserData {
    pub email: String,
    pub access_token: String,
    pub refresh_token: String,
}

impl From<UserProfile> for UserData {
    fn from(value: UserProfile) -> Self {
        Self {
            email: value.email,
            access_token: value.access_token,
            refresh_token: value.refresh_token,
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OrgState {
    pub config: OrgConfig,
    pub status: OrgStatus,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OrgConfig {
    pub archive_folder_id: String,   // Drive folder ID (source of truth)
    pub archive_folder_name: String, // cached display

    /// Local folder mapped to this org folder. Used by the member sync flow.
    pub local_folder_path: Option<String>,
}

#[derive(Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrgStatus {
    #[default]
    Unknown,
    Created,
    Loading,
    Ready,
}

#[derive(Debug, Clone)]
pub struct OrgInvitation {
    pub org_id: String,
    pub org_name: String,
    pub invited_by: String,
}
