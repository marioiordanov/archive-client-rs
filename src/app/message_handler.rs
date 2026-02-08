use iced::Task;

use crate::{
    ArchiveClient,
    app::{
        message::{Message, ScreenMessage},
        state::{Intent, Screen},
    },
    screens,
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
            _ => default,
        }
    }
}
