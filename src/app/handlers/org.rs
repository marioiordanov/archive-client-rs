use iced::Task;
use log::warn;

use crate::{
    ArchiveClient,
    app::{
        self,
        message::{Message, OrgError, OrgMessage},
        state::{Intent, Screen},
    },
    screens,
    services::{self, local_storage::LocalStorageService},
};

impl ArchiveClient {
    pub fn handle_org_messages(&mut self, message: app::message::OrgMessage) -> Task<Message> {
        let mut transition: Option<Screen> = None;
        let task = Task::none();

        match (&mut self.screen, message) {
            (Screen::OrgSelection(screen), OrgMessage::InvitationsLoaded(Ok(invitations))) => {
                screen.invitations = invitations;
                screen.loading = false;
            }

            (_, OrgMessage::OrgCreated(Ok(root_folder_entry))) => {
                self.app.org.status = app::state::OrgStatus::Ready;
                self.app.org.config = app::state::OrgConfig {
                    archive_folder_id: root_folder_entry.id,
                    archive_folder_name: root_folder_entry.name,
                };
                LocalStorageService::save_object(
                    &self.app.org,
                    services::local_storage::ObjectType::Org,
                );

                // Forced next step: invite members
                let org_id = self.app.org.config.archive_folder_id.clone();
                transition = Some(Screen::InviteMembers(
                    screens::invite_members::InviteMembersScreen::new(org_id),
                ));
            }

            (Screen::InviteMembers(_), OrgMessage::InviteUserFinished {result: Err(org_error @ OrgError::Common(app::message::CommonServiceError::TokenExpired)), .. }) => {
                let global_error = org_error.into();
                return self.handle_error(global_error);
            }
            (
                Screen::InviteMembers(screen),
                OrgMessage::InviteUserFinished {
                    run_id,
                    email,
                    result,
                },
            ) => {
                match result {
                    Ok(()) => screen.push_history(
                        run_id,
                        email,
                        screens::invite_members::InviteStatus::Sent,
                    ),
                    // TODO: on unsuccessful invitation, delete the user folder from the root folder in DRIVE
                    Err(e) => screen.push_history(
                        run_id,
                        email,
                        screens::invite_members::InviteStatus::Error(e.to_string()),
                    ),
                }

                // Continue sequentially
                if let Some(next_email) = screen.pop_next_email() {
                    let org_id = screen.org_id.clone();
                    let access_token = self.app.session.user.access_token.clone();
                    self.app.retry_intent = Some(Intent::SendInvitations {
                        run_id,
                        org_id: org_id.clone(),
                        email: next_email.clone(),
                    });

                    return Self::invite_user_task(run_id, next_email, org_id, access_token);
                }

                screen.finish_current_email();
            }

            (Screen::OrgDashboard(screen), OrgMessage::DashboardLoaded(Ok(rows))) => {
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
            }
            (Screen::OrgDashboard(screen), OrgMessage::PermissionRevoked { folder_id, result }) => {
                screen.set_removing(&folder_id, false);

                match result {
                    Ok(()) => {
                        if let Some(row) = screen.rows.iter_mut().find(|r| r.folder_id == folder_id)
                        {
                            row.permission_id = None;
                        }
                    }
                    Err(e) => {
                        screen.set_error(e.to_string());
                    }
                }
            }
            (
                _,
                OrgMessage::OrgJoined(Err(e))
                | OrgMessage::InviteSent(Err(e))
                | OrgMessage::InvitationsLoaded(Err(e))
                | OrgMessage::OrgCreated(Err(e))
                | OrgMessage::DashboardLoaded(Err(e)),
            ) => return self.handle_error(e.into()),
            (screen, msg) => {
                warn!("unhandled message {msg:?} from {screen} ");
            }

        }

        if let Some(screen) = transition {
            self.screen = screen;
        }

        task
    }
}
