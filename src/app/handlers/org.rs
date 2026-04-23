use iced::Task;
use log::warn;

use crate::{
    ArchiveClient, UserState,
    app::{
        self,
        message::{Message, OrgError, OrgMessage},
        state::{AppState, Intent, OrgState, Screen, UserProfile},
    },
    screens::{self, org_dashboard::DashboardRow},
    services::{self, local_storage::LocalStorageService},
};

impl ArchiveClient {
    pub fn handle_org_messages(&mut self, message: app::message::OrgMessage) -> Task<Message> {
        match (&self.app.user_state, &mut self.screen, message) {
            (
                crate::UserState::SignedIn { .. },
                Screen::OrgSelection(screen),
                OrgMessage::InvitationsLoaded(Ok(invitations)),
            ) => Self::on_invitations_loaded_ok(screen, invitations),

            (
                crate::UserState::SignedIn { user_data },
                _,
                OrgMessage::OrgCreated(Ok(root_folder_entry)),
            ) => self.on_org_created_ok(root_folder_entry, user_data.access_token.clone()),
            (
                crate::UserState::OrgCreated { org_id, user_data },
                Screen::OrgDashboard(screen),
                OrgMessage::InviteUserFinished {
                    run_id,
                    email,
                    result: Ok((folder_id, permission_id)),
                },
            ) => {
                let org_id = org_id.clone();
                let access_token = user_data.access_token.clone();
                Self::on_dashboard_invite_user_finished_ok(
                    &mut self.app,
                    screen,
                    run_id,
                    email,
                    folder_id,
                    permission_id,
                    org_id,
                    access_token,
                )
            }

            (
                UserState::OrgCreated { .. },
                Screen::OrgDashboard(screen),
                OrgMessage::DashboardLoaded(Ok(rows)),
            ) => Self::on_dashboard_loaded_ok(screen, rows),
            (
                UserState::OrgCreated { .. },
                Screen::OrgDashboard(screen),
                OrgMessage::PermissionRevoked {
                    folder_id,
                    result: Ok(()),
                },
            ) => Self::on_permission_revoked_ok(screen, folder_id),
            (
                UserState::OrgCreated { .. },
                Screen::OrgDashboard(_),
                OrgMessage::InviteUserFinished {
                    result:
                        Err(
                            e
                            @ OrgError::Common(app::message::CommonServiceError::TokenExpired(..)),
                        ),
                    ..
                },
            ) => self.handle_error(e.into()),
            (
                UserState::OrgCreated { org_id, user_data },
                Screen::OrgDashboard(screen),
                OrgMessage::InviteUserFinished {
                    run_id,
                    email,
                    result: Err(e),
                },
            ) => {
                let org_id = org_id.clone();
                let access_token = user_data.access_token.clone();
                Self::on_dashboard_invite_user_finished_err(
                    screen,
                    &mut self.app,
                    run_id,
                    email,
                    e,
                    org_id,
                    access_token,
                )
            }

            (
                UserState::OrgCreated { .. },
                Screen::OrgDashboard(screen),
                OrgMessage::PermissionRevoked {
                    folder_id,
                    result: Err(e),
                },
            ) => Self::on_permission_revoked_err(screen, folder_id, e),

            (UserState::OrgCreated { .. }, _, OrgMessage::OrgCreated(Err(e)))
            | (UserState::OrgCreated { .. }, _, OrgMessage::DashboardLoaded(Err(e)))
            | (UserState::SignedIn { .. }, _, OrgMessage::InvitationsLoaded(Err(e)))
            | (UserState::SignedIn { .. }, _, OrgMessage::OrgJoined(Err(e)))
            | (UserState::OrgCreated { .. }, _, OrgMessage::InviteSent(Err(e))) => {
                self.handle_error(e.into())
            }
            (user_state, screen, msg) => {
                warn!("state {user_state} unhandled message {msg:?} from {screen}");
                Task::none()
            }
        }
    }

    fn on_invitations_loaded_ok(
        screen: &mut screens::org_selection::OrgSelectionScreen,
        invitations: Vec<crate::app::state::OrgInvitation>,
    ) -> Task<Message> {
        screen.invitations = invitations;
        screen.loading = false;

        Task::none()
    }

    fn on_org_created_ok(
        &mut self,
        root_folder_entry: services::org::RootFolderEntry,
        access_token: String,
    ) -> Task<Message> {
        LocalStorageService::update_object::<OrgState, _>(
            services::local_storage::ObjectType::Org,
            |org| {
                org.status = app::state::OrgStatus::Created;
                org.config = app::state::OrgConfig {
                    archive_folder_id: root_folder_entry.id.clone(),
                    archive_folder_name: root_folder_entry.name,
                    local_folder_path: None,
                }
            },
        );

        LocalStorageService::update_object::<UserProfile, _>(
            services::local_storage::ObjectType::UserProfile,
            |user| {
                user.role = Some(app::state::Role::Owner);
            },
        );

        // Next step: show dashboard with invite panel open
        let org_id = root_folder_entry.id;
        let mut screen = screens::org_dashboard::OrgDashboardScreen::new();
        screen.show_invite_panel = true;
        self.screen = Screen::OrgDashboard(screen);

        self.app.pending_intents.push(Intent::LoadDashboard {
            org_id: org_id.clone(),
        });

        self.app.user_state.org_create(org_id.clone());

        Self::load_dashboard_task(org_id, access_token)
    }

    fn on_dashboard_invite_user_finished_ok(
        state: &mut crate::app::state::AppState,
        screen: &mut screens::org_dashboard::OrgDashboardScreen,
        run_id: u64,
        email: String,
        folder_id: String,
        permission_id: String,
        org_id: String,
        access_token: String,
    ) -> Task<Message> {
        screen.update(screens::org_dashboard::Message::RecordInviteInLog {
            run_id,
            email: email.clone(),
            status: screens::org_dashboard::InviteStatus::Sent,
        });
        screen.update(screens::org_dashboard::Message::AddRow {
            row: DashboardRow {
                email,
                folder_id,
                active: false,
                permission_id: Some(permission_id),
                removing: false,
            },
        });

        Self::on_dashboard_invite_user_finished_continue(
            state,
            screen,
            run_id,
            org_id,
            access_token,
        )
    }

    fn on_dashboard_invite_user_finished_err(
        screen: &mut screens::org_dashboard::OrgDashboardScreen,
        state: &mut AppState,
        run_id: u64,
        email: String,
        error: OrgError,
        org_id: String,
        access_token: String,
    ) -> Task<Message> {
        screen.update(screens::org_dashboard::Message::RecordInviteInLog {
            run_id,
            email,
            status: screens::org_dashboard::InviteStatus::Error(error.to_string()),
        });

        Self::on_dashboard_invite_user_finished_continue(
            state,
            screen,
            run_id,
            org_id,
            access_token,
        )
    }

    fn on_dashboard_invite_user_finished_continue(
        state: &mut crate::app::state::AppState,
        screen: &mut screens::org_dashboard::OrgDashboardScreen,
        run_id: u64,
        org_id: String,
        access_token: String,
    ) -> Task<Message> {
        screen.update(screens::org_dashboard::Message::InviteNextEmail);

        if let Some((_, next_email)) = screen.invite_current_task() {
            state.pending_intents.push(Intent::SendInvitations {
                run_id,
                org_id: org_id.clone(),
                email: next_email.clone(),
            });

            Self::invite_user_task(run_id, next_email, org_id, access_token)
        } else {
            screen.update(screens::org_dashboard::Message::InviteFinishEmail);
            Task::none()
        }
    }

    fn on_dashboard_loaded_ok(
        screen: &mut screens::org_dashboard::OrgDashboardScreen,
        rows: Vec<services::org::DashboardRowData>,
    ) -> Task<Message> {
        let screen_rows = rows
            .into_iter()
            .map(|r| screens::org_dashboard::DashboardRow {
                email: r.email,
                folder_id: r.folder_id,
                active: r.active,
                permission_id: r.permission_id,
                removing: false,
            })
            .collect();

        screen.update(screens::org_dashboard::Message::DashboardRowsLoaded { rows: screen_rows });

        Task::none()
    }

    fn on_permission_revoked_ok(
        screen: &mut screens::org_dashboard::OrgDashboardScreen,
        folder_id: String,
    ) -> Task<Message> {
        screen.update(screens::org_dashboard::Message::StopRemoveAccessAction {
            folder_id: folder_id.clone(),
        });
        screen.update(screens::org_dashboard::Message::RemoveAccessRow { folder_id });

        Task::none()
    }

    fn on_permission_revoked_err(
        screen: &mut screens::org_dashboard::OrgDashboardScreen,
        folder_id: String,
        error: OrgError,
    ) -> Task<Message> {
        screen.update(screens::org_dashboard::Message::StopRemoveAccessAction {
            folder_id: folder_id.clone(),
        });
        screen.update(screens::org_dashboard::Message::ShowError {
            error: error.to_string(),
        });

        Task::none()
    }
}
