use iced::Task;

use crate::{
    ArchiveClient,
    app::{
        message::{Message, ScreenMessage},
        state::{Intent, Role, Screen},
    },
    screens,
    services::local_storage::{LocalStorageService, ObjectType},
};

impl ArchiveClient {
    pub fn handle_message(&mut self, message: Message) -> (Task<Message>, Option<Screen>) {
        let default = (Task::none(), None);
        match (&mut self.screen, message) {
            (
                Screen::SignIn(screen),
                Message::Screen(ScreenMessage::Login(
                    msg @ screens::signin::Message::SignInClicked,
                )),
            ) => {
                screen.update(msg);
                let task = ArchiveClient::get_access_token_task();

                (task, None)
            }
            (
                Screen::SignIn(screen),
                Message::Screen(ScreenMessage::Login(msg @ screens::signin::Message::ClearError)),
            ) => {
                screen.update(msg);
                default
            }
            (_, Message::Auth(auth_msg)) => (self.handle_auth_messages(auth_msg), None),
            (_, Message::Org(org_msg)) => (self.handle_org_messages(org_msg), None),
            (_, Message::Sync(sync_msg)) => (self.handle_sync_messages(sync_msg), None),
            (
                Screen::OrgSelection(screen),
                Message::Screen(ScreenMessage::OrgSelection(
                    msg @ screens::org_selection::Message::CreateOrgClicked,
                )),
            ) => {
                screen.update(msg.clone());
                self.app.retry_intent = Some(Intent::CreateOrg);

                let task = ArchiveClient::get_or_create_organization_task(
                    self.app.session.user.email.clone(),
                    self.app.session.user.access_token.clone(),
                );
                (task, None)
            }
            (
                Screen::OrgSelection(screen),
                Message::Screen(ScreenMessage::OrgSelection(
                    screens::org_selection::Message::JoinOrgClicked { org_id, org_name },
                )),
            ) => {
                // Update the selection screen state (shows loading) immediately.
                screen.update(screens::org_selection::Message::JoinOrgClicked {
                    org_id: org_id.clone(),
                    org_name: org_name.clone(),
                });

                self.app.org.status = crate::app::state::OrgStatus::Ready;
                self.app.session.user.role = Some(Role::User);
                self.app.org.config.archive_folder_id = org_id;
                self.app.org.config.archive_folder_name = org_name;
                // Keep any existing mapping if present (e.g. user re-joins same org id).

                LocalStorageService::save_object(&self.app.org, ObjectType::Org);
                LocalStorageService::save_object(&self.app.session.user, ObjectType::UserProfile);

                self.app.retry_intent = None;
                self.screen = Screen::OrgSync(screens::org_sync::OrgSyncScreen::new(
                    self.app.org.config.local_folder_path.clone(),
                ));

                default
            }
            (
                Screen::OrgDashboard(screen),
                Message::Screen(ScreenMessage::OrgDashboard(
                    msg @ screens::org_dashboard::Message::InviteMembersClicked,
                )),
            ) => {
                screen.update(msg);
                default
            }
            (
                Screen::OrgDashboard(screen),
                Message::Screen(ScreenMessage::OrgDashboard(
                    msg @ screens::org_dashboard::Message::InviteEdit(_),
                )),
            ) => {
                screen.update(msg);
                default
            }
            (
                Screen::OrgDashboard(screen),
                Message::Screen(ScreenMessage::OrgDashboard(
                    msg @ screens::org_dashboard::Message::InviteSendClicked,
                )),
            ) => {
                screen.update(msg);

                // Decide whether we can kick off the async work:
                let Some((run_id, email)) = screen.invite_current_task() else {
                    return default;
                };

                let org_id = self.app.get_org_id().to_string();
                let access_token = self.app.session.user.access_token.clone();

                self.app.retry_intent = Some(Intent::SendInvitations {
                    run_id,
                    org_id: org_id.clone(),
                    email: email.clone(),
                });

                (
                    Self::invite_user_task(run_id, email, org_id, access_token),
                    None,
                )
            }
            (
                Screen::OrgDashboard(screen),
                Message::Screen(ScreenMessage::OrgDashboard(
                    msg @ screens::org_dashboard::Message::InviteDoneClicked,
                )),
            ) => {
                if screen.invite_can_done() {
                    screen.update(msg);

                    let org_id = self.app.org.config.archive_folder_id.clone();
                    let access_token = self.app.session.user.access_token.clone();
                    self.app.retry_intent = Some(Intent::LoadDashboard {
                        org_id: org_id.clone(),
                    });
                    (Self::load_dashboard_task(org_id, access_token), None)
                } else {
                    default
                }
            }
            (
                Screen::OrgDashboard(screen),
                Message::Screen(ScreenMessage::OrgDashboard(
                    msg @ screens::org_dashboard::Message::RefreshClicked,
                )),
            ) => {
                screen.update(msg);

                let org_id = self.app.get_org_id().to_string();
                let access_token = self.app.session.user.access_token.clone();
                self.app.retry_intent = Some(Intent::LoadDashboard {
                    org_id: org_id.clone(),
                });

                (Self::load_dashboard_task(org_id, access_token), None)
            }
            (
                Screen::OrgDashboard(screen),
                Message::Screen(ScreenMessage::OrgDashboard(
                    screens::org_dashboard::Message::RemoveAccessClicked {
                        email,
                        folder_id,
                        permission_id,
                    },
                )),
            ) => {
                screen.update(screens::org_dashboard::Message::RemoveAccessClicked {
                    email: email.clone(),
                    folder_id: folder_id.clone(),
                    permission_id: permission_id.clone(),
                });

                let access_token = self.app.session.user.access_token.clone();
                (
                    Self::revoke_permission_task(folder_id, email, permission_id, access_token),
                    None,
                )
            }
            (
                Screen::OrgSync(screen),
                Message::Screen(ScreenMessage::OrgSync(msg @ screens::org_sync::Message::LocalFolderChanged(_))),
            ) => {
                screen.update(msg);
                default
            }
            (
                Screen::OrgSync(screen),
                Message::Screen(ScreenMessage::OrgSync(msg @ screens::org_sync::Message::ClearLogClicked)),
            ) => {
                screen.update(msg);
                default
            }
            (
                Screen::OrgSync(screen),
                Message::Screen(ScreenMessage::OrgSync(msg @ screens::org_sync::Message::StopWatchingClicked)),
            ) => {
                screen.update(msg);
                default
            }
            (
                Screen::OrgSync(screen),
                Message::Screen(ScreenMessage::OrgSync(msg @ screens::org_sync::Message::SaveMappingClicked)),
            ) => {
                screen.update(msg);

                let input = screen.local_folder_input.trim().to_string();
                if input.is_empty() {
                    screen.status_line = Some("Enter a folder path first.".to_string());
                    return default;
                }

                let path = std::path::PathBuf::from(&input);
                if !path.exists() || !path.is_dir() {
                    screen.status_line = Some("Folder does not exist (or is not a directory).".to_string());
                    return default;
                }

                self.app.org.config.local_folder_path = Some(input.clone());
                screen.mapped_folder = Some(input);
                LocalStorageService::save_object(&self.app.org, ObjectType::Org);

                default
            }
            (
                Screen::OrgSync(screen),
                Message::Screen(ScreenMessage::OrgSync(msg @ screens::org_sync::Message::StartWatchingClicked)),
            ) => {
                // Best-effort: auto-save mapping if it's valid.
                let input = screen.local_folder_input.trim().to_string();
                let path = std::path::PathBuf::from(&input);
                if input.is_empty() || !path.exists() || !path.is_dir() {
                    screen.status_line = Some("Set a valid local folder path first.".to_string());
                    screen.watching = false;
                    return default;
                }

                if self.app.org.config.local_folder_path.as_deref() != Some(&input) {
                    self.app.org.config.local_folder_path = Some(input.clone());
                    screen.mapped_folder = Some(input);
                    LocalStorageService::save_object(&self.app.org, ObjectType::Org);
                }

                screen.update(msg);
                default
            }
            _ => default,
        }
    }
}
