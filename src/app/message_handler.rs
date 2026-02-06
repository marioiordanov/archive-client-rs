use iced::Task;

use crate::{
    ArchiveClient,
    app::{
        self,
        message::{Message, OrgMessage, ScreenMessage},
        state::{Intent, Screen},
    },
    screens,
    services::{auth::AuthService, org::OrgService},
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
                let task = Task::perform(AuthService::get_drive_access_token(), |access_token| {
                    Message::Auth(app::message::AuthMessage::AccessTokenReceived(access_token))
                });

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

                let task = Task::perform(
                    OrgService::get_or_create_organization(
                        self.app.session.user.access_token.clone(),
                        self.app.session.user.email.clone(),
                    ),
                    |organisation| Message::Org(OrgMessage::OrgCreated(organisation)),
                );
                (task, None)
            }
            (
                Screen::InviteMembers(screen),
                Message::Screen(ScreenMessage::InviteMembers(
                    screen_msg @ screens::invite_members::Message::Edit(_),
                )),
            ) => {
                screen.update(screen_msg);
                default
            }
            (
                Screen::InviteMembers(screen),
                Message::Screen(ScreenMessage::InviteMembers(
                    screens::invite_members::Message::SendInvitesClicked,
                )),
            ) => {
                let Some(run_id) = screen.begin_run() else {
                    return default;
                };

                let Some(email) = screen.pop_next_email() else {
                    screen.finish_current_email();
                    return default;
                };

                let org_id = screen.org_id.clone();
                let access_token = self.app.session.user.access_token.clone();
                self.app.retry_intent = Some(Intent::SendInvitations {
                    run_id: run_id,
                    org_id: org_id.clone(),
                    email: email.clone(),
                });

                (
                    Self::invite_user_task(run_id, email, org_id, access_token),
                    None,
                )
            }
            (
                Screen::InviteMembers(screen),
                Message::Screen(ScreenMessage::InviteMembers(
                    screens::invite_members::Message::ContinueClicked,
                )),
            ) => {
                let can_continue = screen.can_continue();

                if !can_continue {
                    return default;
                }

                let org_id = self.app.org.config.archive_folder_id.clone();
                self.screen = Screen::OrgDashboard(
                    screens::org_dashboard::OrgDashboardScreen::new(org_id.clone()),
                );

                self.app.retry_intent = Some(Intent::LoadDashboard {
                    org_id: org_id.clone(),
                });
                let access_token = self.app.session.user.access_token.clone();
                (Self::load_dashboard_task(org_id, access_token), None)
            }
            (
                Screen::OrgDashboard(screen),
                Message::Screen(ScreenMessage::OrgDashboard(
                    msg @ screens::org_dashboard::Message::InviteMembersClicked,
                )),
            ) => {
                screen.toggle_invite_panel();
                let _ = msg;
                default
            }
            (
                Screen::OrgDashboard(screen),
                Message::Screen(ScreenMessage::OrgDashboard(
                    screens::org_dashboard::Message::InviteEdit(action),
                )),
            ) => {
                screen.invite_edit(action);
                default
            }
            (
                Screen::OrgDashboard(screen),
                Message::Screen(ScreenMessage::OrgDashboard(
                    screens::org_dashboard::Message::InviteSendClicked,
                )),
            ) => {
                let Some(run_id) = screen.invite_begin_run() else {
                    return default;
                };

                let Some(email) = screen.invite_pop_next_email() else {
                    screen.invite_finish_current_email();
                    return default;
                };

                let org_id = screen.org_id.clone();
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
                    screens::org_dashboard::Message::InviteDoneClicked,
                )),
            ) => {
                if !screen.invite_can_done() {
                    return default;
                }

                screen.show_invite_panel = false;
                screen.loading = true;
                screen.error = None;

                let org_id = screen.org_id.clone();
                let access_token = self.app.session.user.access_token.clone();
                self.app.retry_intent = Some(Intent::LoadDashboard {
                    org_id: org_id.clone(),
                });
                (Self::load_dashboard_task(org_id, access_token), None)
            }
            (
                Screen::OrgDashboard(screen),
                Message::Screen(ScreenMessage::OrgDashboard(
                    screens::org_dashboard::Message::RefreshClicked,
                )),
            ) => {
                screen.loading = true;
                screen.error = None;
                let org_id = screen.org_id.clone();
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
                screen.set_removing(&folder_id, true);
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
