use iced::Task;
use log::warn;

use crate::{
    ArchiveClient,
    app::{
        self,
        message::{Message, OrgError, OrgMessage},
        state::{AppState, Intent, Screen},
    },
    screens::{self, invite_members::InviteMembersScreen},
    services::{self, local_storage::LocalStorageService},
};

impl ArchiveClient {
    pub fn handle_org_messages(&mut self, message: app::message::OrgMessage) -> Task<Message> {
        match (&mut self.screen, message) {
            (Screen::OrgSelection(screen), OrgMessage::InvitationsLoaded(Ok(invitations))) => {
                Self::on_invitations_loaded_ok(screen, invitations)
            }

            (_, OrgMessage::OrgCreated(Ok(root_folder_entry))) => {
                self.on_org_created_ok(root_folder_entry)
            }
            (
                Screen::OrgDashboard(screen),
                OrgMessage::InviteUserFinished {
                    run_id,
                    email,
                    result: Ok(()),
                },
            ) => Self::on_dashboard_invite_user_finished_ok(&mut self.app, screen, run_id, email),

            (Screen::OrgDashboard(screen), OrgMessage::DashboardLoaded(Ok(rows))) => {
                Self::on_dashboard_loaded_ok(screen, rows)
            }
            (
                Screen::OrgDashboard(screen),
                OrgMessage::PermissionRevoked {
                    folder_id,
                    result: Ok(()),
                },
            ) => Self::on_permission_revoked_ok(screen, folder_id),
            (
                Screen::OrgDashboard(_),
                OrgMessage::InviteUserFinished {
                    result:
                        Err(e @ OrgError::Common(app::message::CommonServiceError::TokenExpired)),
                    ..
                },
            ) => self.handle_error(e.into()),
            (
                Screen::OrgDashboard(screen),
                OrgMessage::InviteUserFinished {
                    run_id,
                    email,
                    result: Err(e),
                },
            ) => {
                Self::on_dashboard_invite_user_finished_err(screen, &mut self.app, run_id, email, e)
            }

            (
                Screen::OrgDashboard(screen),
                OrgMessage::PermissionRevoked {
                    folder_id,
                    result: Err(e),
                },
            ) => Self::on_permission_revoked_err(screen, folder_id, e),

            (_, OrgMessage::OrgCreated(Err(e)))
            | (_, OrgMessage::DashboardLoaded(Err(e)))
            | (_, OrgMessage::InvitationsLoaded(Err(e)))
            | (_, OrgMessage::OrgJoined(Err(e)))
            | (_, OrgMessage::InviteSent(Err(e))) => self.handle_error(e.into()),
            (screen, msg) => {
                warn!("unhandled message {msg:?} from {screen} ");
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
    ) -> Task<Message> {
        self.app.org.status = app::state::OrgStatus::Ready;
        self.app.org.config = app::state::OrgConfig {
            archive_folder_id: root_folder_entry.id,
            archive_folder_name: root_folder_entry.name,
        };
        LocalStorageService::save_object(&self.app.org, services::local_storage::ObjectType::Org);

        // Next step: show dashboard with invite panel open
        let org_id = self.app.org.config.archive_folder_id.clone();
        let mut screen = screens::org_dashboard::OrgDashboardScreen::new(org_id.clone());
        screen.show_invite_panel = true;
        self.screen = Screen::OrgDashboard(screen);

        self.app.retry_intent = Some(Intent::LoadDashboard {
            org_id: org_id.clone(),
        });

        Self::load_dashboard_task(org_id, self.app.session.user.access_token.clone())
    }

    fn on_invite_user_finished_ok(
        state: &mut crate::app::state::AppState,
        screen: &mut screens::invite_members::InviteMembersScreen,
        run_id: u64,
        email: String,
    ) -> Task<Message> {
        screen.push_history(run_id, email, screens::invite_members::InviteStatus::Sent);

        Self::on_invite_user_finished_continue(state, screen, run_id)
    }

    fn on_invite_user_finished_err(
        screen: &mut InviteMembersScreen,
        state: &mut AppState,
        run_id: u64,
        email: String,
        error: OrgError,
    ) -> Task<Message> {
        // TODO: on unsuccessful invitation, delete the user folder from the root folder in DRIVE
        screen.push_history(
            run_id,
            email,
            screens::invite_members::InviteStatus::Error(error.to_string()),
        );

        Self::on_invite_user_finished_continue(state, screen, run_id)
    }

    fn on_invite_user_finished_continue(
        state: &mut crate::app::state::AppState,
        screen: &mut screens::invite_members::InviteMembersScreen,
        run_id: u64,
    ) -> Task<Message> {
        if let Some(next_email) = screen.pop_next_email() {
            let org_id = screen.org_id.clone();
            let access_token = state.session.user.access_token.clone();
            state.retry_intent = Some(Intent::SendInvitations {
                run_id,
                org_id: org_id.clone(),
                email: next_email.clone(),
            });

            Self::invite_user_task(run_id, next_email, org_id, access_token)
        } else {
            screen.finish_current_email();
            Task::none()
        }
    }

    fn on_dashboard_invite_user_finished_ok(
        state: &mut crate::app::state::AppState,
        screen: &mut screens::org_dashboard::OrgDashboardScreen,
        run_id: u64,
        email: String,
    ) -> Task<Message> {
        screen.invite_push_history(run_id, email, screens::org_dashboard::InviteStatus::Sent);

        Self::on_dashboard_invite_user_finished_continue(state, screen, run_id)
    }

    fn on_dashboard_invite_user_finished_err(
        screen: &mut screens::org_dashboard::OrgDashboardScreen,
        state: &mut AppState,
        run_id: u64,
        email: String,
        error: OrgError,
    ) -> Task<Message> {
        screen.invite_push_history(
            run_id,
            email,
            screens::org_dashboard::InviteStatus::Error(error.to_string()),
        );

        Self::on_dashboard_invite_user_finished_continue(state, screen, run_id)
    }

    fn on_dashboard_invite_user_finished_continue(
        state: &mut crate::app::state::AppState,
        screen: &mut screens::org_dashboard::OrgDashboardScreen,
        run_id: u64,
    ) -> Task<Message> {
        if let Some(next_email) = screen.invite_pop_next_email() {
            let org_id = screen.org_id.clone();
            let access_token = state.session.user.access_token.clone();
            state.retry_intent = Some(Intent::SendInvitations {
                run_id,
                org_id: org_id.clone(),
                email: next_email.clone(),
            });

            Self::invite_user_task(run_id, next_email, org_id, access_token)
        } else {
            screen.invite_finish_current_email();
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

        screen.set_rows(screen_rows);

        Task::none()
    }

    fn on_permission_revoked_ok(
        screen: &mut screens::org_dashboard::OrgDashboardScreen,
        folder_id: String,
    ) -> Task<Message> {
        screen.set_removing(&folder_id, false);

        if let Some(row) = screen.rows.iter_mut().find(|r| r.folder_id == folder_id) {
            row.permission_id = None;
        }

        Task::none()
    }

    fn on_permission_revoked_err(
        screen: &mut screens::org_dashboard::OrgDashboardScreen,
        folder_id: String,
        error: OrgError,
    ) -> Task<Message> {
        screen.set_removing(&folder_id, false);
        screen.set_error(error.to_string());
        Task::none()
    }
}
