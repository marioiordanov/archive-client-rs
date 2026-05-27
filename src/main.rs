use std::path::PathBuf;

use iced::{Element, Subscription, Task};
use lazy_static::lazy_static;
use log::warn;

mod app;
mod constants;
mod screens;
mod services;
mod ui_error;

use crate::{
    app::{
        message::Message,
        state::{
            AppState, Intent, OrgState, Role, Screen, SessionState, UserData,
            UserProfile,
        },
        subscriptions::tcp_server_subscription,
    },
    services::{
        file_index::FileIndex, local_storage::LocalStorageService, resolver::Resolver,
        revisions_cache::Cache,
    },
};

lazy_static! {
    static ref HTTP: reqwest::Client = reqwest::Client::new();
}

fn main() -> iced::Result {
    env_logger::Builder::from_default_env()
        .format_timestamp_millis()
        .init();

    dotenvy::dotenv().map_err(|e| iced::Error::WindowCreationFailed(e.into()))?;

    iced::application(
        ArchiveClient::boot,
        ArchiveClient::update,
        ArchiveClient::view,
    )
    .centered()
    .subscription(ArchiveClient::subscription)
    .run()
}

struct ArchiveClient {
    app: AppState,
    screen: Screen,
}

//                             /-> OrgCreated (owner)
// FLOWS: SignedOut->Signedin /
//                            \
//                             \-> OrgJoined -> OrgSyncing -> OrgSynced (user)
enum UserState {
    SignedOut,
    SignedIn {
        user_data: UserData,
    },
    OrgCreated {
        org_id: String,
        user_data: UserData,
    },
    OrgJoined {
        root_folder_id: String,
        user_data: UserData,
    },
    OrgSynced {
        resolver: Resolver,
        root_folder_id: String,
        root_dir: PathBuf,
        user_data: UserData,
        revisions_cache: Cache,
    },
}

impl UserState {
    pub(crate) fn sign_out(&mut self) {
        *self = UserState::SignedOut;
    }
    pub(crate) fn sign_in(&mut self, user_data: UserData) {
        if let UserState::SignedOut = self {
            *self = UserState::SignedIn { user_data }
        } else {
            warn!("impossible to sign in from {}", self);
        }
    }

    pub(crate) fn org_create(&mut self, org_id: String) {
        if let UserState::SignedIn { user_data } = self {
            *self = UserState::OrgCreated {
                org_id,
                user_data: user_data.clone(),
            }
        } else {
            warn!("impossible to create org from {}", self);
        }
    }

    pub(crate) fn org_joined(&mut self, root_folder_id: String) {
        if let UserState::SignedIn { user_data } = self {
            *self = UserState::OrgJoined {
                root_folder_id,
                user_data: user_data.clone(),
            };
        } else {
            warn!("impossible to join org from {}", self);
        }
    }

    pub(crate) fn org_synced(&mut self, resolver: Resolver, root_dir: PathBuf) {
        if let UserState::OrgJoined {
            user_data,
            root_folder_id,
        } = self
        {
            *self = UserState::OrgSynced {
                resolver,
                root_folder_id: root_folder_id.clone(),
                root_dir,
                user_data: user_data.clone(),
                revisions_cache: Cache::default(),
            };
        } else {
            warn!("impossible to sync org from {}", self);
        }
    }
}

impl std::fmt::Display for UserState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            UserState::SignedOut => "Signed out",
            UserState::SignedIn { .. } => "Signed in",
            UserState::OrgCreated { .. } => "Owner org created",
            UserState::OrgJoined { .. } => "User joined org",
            UserState::OrgSynced { .. } => "User org synced",
        };

        f.write_str(str)
    }
}

impl ArchiveClient {
    fn boot() -> (Self, Task<Message>) {
        println!("Starting Archive Client...");

        let user_profile = LocalStorageService::load_object::<UserProfile>(
            services::local_storage::ObjectType::UserProfile,
        );

        let has_user_profile = user_profile.is_some();

        let org_profile =
            LocalStorageService::load_object::<OrgState>(services::local_storage::ObjectType::Org);

        let has_org_profile = org_profile.is_some();

        let org = org_profile.unwrap_or_default();

        let mut session = if let Some(user) = user_profile {
            SessionState {
                user,
                auth: app::state::AuthState::SignedIn,
            }
        } else {
            SessionState::default()
        };

        let (state, screen, next_task) = match (has_user_profile, has_org_profile) {
            (true, true) => {
                // Role belongs to the user, not the organization.
                // Backward-compat heuristic if role is missing:
                // - if a local folder is mapped, treat as member/user
                // - otherwise treat as owner
                let role_was_none = session.user.role.is_none();
                let inferred_role = match session.user.role.clone() {
                    Some(r) => Some(r),
                    None if org.config.local_folder_path.is_some() => Some(Role::User),
                    None => Some(Role::Owner),
                };

                session.user.role = inferred_role;

                if role_was_none {
                    LocalStorageService::save_object(
                        &session.user,
                        services::local_storage::ObjectType::UserProfile,
                    );
                }

                match session.user.role.as_ref() {
                    Some(Role::Owner) => {
                        let next_task = ArchiveClient::load_dashboard_task(
                            org.config.archive_folder_id.clone(),
                            session.user.access_token.clone(),
                        );
                        let screen = app::state::Screen::OrgDashboard(
                            screens::org_dashboard::OrgDashboardScreen::new(),
                        );
                        let org_id = org.config.archive_folder_id.clone();
                        let state = AppState {
                            pending_intents: vec![Intent::LoadDashboard {
                                org_id: org_id.clone(),
                            }],
                            user_state: UserState::OrgCreated {
                                org_id,
                                user_data: session.user.clone().into(),
                            },
                            pending_refresh: false,
                        };

                        (state, screen, next_task)
                    }
                    Some(Role::User) => {
                        let mapped = org.config.local_folder_path.clone();
                        let screen = app::state::Screen::OrgSync(
                            screens::org_sync::OrgSyncScreen::new(mapped.clone()),
                        );

                        let user_state = if let Some(root_dir) = mapped
                            .clone()
                            .map(PathBuf::from)
                            .filter(|_| org.status == app::state::OrgStatus::Ready)
                        {
                            let resolver = Resolver::new(
                                root_dir.clone(),
                                FileIndex::load(org.config.archive_folder_id.clone()),
                            );

                            UserState::OrgSynced {
                                resolver,
                                root_dir,
                                root_folder_id: org.config.archive_folder_id,
                                user_data: session.user.clone().into(),
                                revisions_cache: Cache::default(),
                            }
                        } else {
                            UserState::OrgJoined {
                                root_folder_id: org.config.archive_folder_id,
                                user_data: session.user.clone().into(),
                            }
                        };

                        let state = AppState {
                            user_state,
                            pending_intents: vec![],
                            pending_refresh: false,
                        };
                        let next_task = Task::none(); // initial_sync. If nothing in the directory, then create folder structure and upload files

                        (state, screen, next_task)
                    }
                    None => {
                        // Should be unreachable due to inference above.
                        panic!("Should be unreachable");
                    }
                }
            }
            (true, false) => {
                let screen = app::state::Screen::OrgSelection(
                    screens::org_selection::OrgSelectionScreen::new(),
                );

                let next_task = Self::fetch_invitations_task(
                    session.user.email.clone(),
                    session.user.access_token.clone(),
                );

                let state = AppState {
                    user_state: UserState::SignedIn {
                        user_data: session.user.clone().into(),
                    },
                    pending_intents: vec![Intent::FetchInvitations],
                    pending_refresh: false,
                };

                (state, screen, next_task)
            }
            (false, false) => (
                AppState {
                    user_state: UserState::SignedOut,
                    pending_intents: vec![],
                    pending_refresh: false,
                },
                app::state::Screen::SignIn(screens::signin::SignInScreen::default()),
                Task::none(),
            ),
            (false, true) => panic!("Impossible"),
        };

        (Self { app: state, screen }, next_task)
    }

    // update the UI
    fn view(&self) -> Element<'_, Message> {
        println!("Rendering view for screen {}", self.screen);

        let folder_name = match &self.app.user_state {
            crate::UserState::SignedOut => "",
            crate::UserState::SignedIn { user_data }
            | crate::UserState::OrgCreated { user_data, .. }
            | crate::UserState::OrgJoined { user_data, .. }
            | crate::UserState::OrgSynced { user_data, .. } => &user_data.email,
        };

        

        match &self.screen {
            app::state::Screen::SignIn(screen) => screen.view().map(|m| Message::Screen(m.into())),
            app::state::Screen::OrgSelection(screen) => {
                screen.view().map(|m| Message::Screen(m.into()))
            }
            app::state::Screen::OrgDashboard(screen) => {
                screen.view().map(|m| Message::Screen(m.into()))
            }
            app::state::Screen::OrgSync(screen) => {
                screen.view(folder_name).map(|m| Message::Screen(m.into()))
            }
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let fs_watch = match (&self.screen, &self.app.user_state) {
            (Screen::OrgSync(screen), UserState::OrgSynced { root_dir, .. }) => {
                crate::app::subscriptions::fs_watch_subscription(root_dir.clone())
            }
            _ => Subscription::none(),
        };

        let unix = match self.app.user_state {
            UserState::OrgSynced { .. } => tcp_server_subscription(),
            _ => Subscription::none(),
        };

        Subscription::batch([fs_watch, unix])
    }
}
