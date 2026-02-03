use std::time::{SystemTime, UNIX_EPOCH};

use iced::Task;

use crate::{
    ArchiveClient,
    app::{
        self,
        message::{GlobalError, Message, OrgMessage, ScreenMessage},
        state::{Intent, Screen, SessionState, UserProfile},
    },
    screens::{self, signin::SignInScreen},
    services::{self, auth::AuthService, local_storage::LocalStorageService, org::OrgService},
};

impl ArchiveClient {
    fn invite_user_task(
        run_id: u64,
        email: String,
        org_id: String,
        access_token: String,
    ) -> Task<Message> {
        let email_for_async = email.clone();
        Task::perform(
            async move {
                OrgService::invite_user(&email_for_async, &org_id, &access_token)
                    .await
                    .map(|_| ())
            },
            move |result| {
                Message::Org(OrgMessage::InviteUserFinished {
                    run_id,
                    email,
                    result,
                })
            },
        )
    }
}

impl ArchiveClient {
    pub fn fetch_invitations_task(user_email: String, access_token: String) -> Task<Message> {
        Task::perform(
            async move { OrgService::fetch_invitations(&user_email, &access_token).await },
            |result| Message::Org(OrgMessage::InvitationsLoaded(result)),
        )
    }

    fn re_auth(&mut self) -> Task<Message> {
        self.app.session = SessionState::default();
        self.app.screen = Screen::SignIn(SignInScreen::default());
        Task::none()
    }

    fn run_intent(&self, intent: Intent) -> Task<Message> {
        match intent {
            Intent::FetchInvitations => {
                let email = self.app.session.user.email.clone();
                Self::fetch_invitations_task(email, self.app.session.user.access_token.clone())
            }
            Intent::CreateOrg => Task::perform(
                OrgService::get_or_create_organization(
                    self.app.session.user.access_token.clone(),
                    self.app.session.user.email.clone(),
                ),
                |organisation| Message::Org(OrgMessage::OrgCreated(organisation)),
            ),
        }
    }

    fn handle_error(&mut self, error: GlobalError) -> Task<Message> {
        match error {
            GlobalError::Common(app::message::CommonServiceError::TokenExpired) => {
                if !self.app.is_signed_in() {
                    self.re_auth()
                } else {
                    let refresh_token = self.app.session.user.refresh_token.clone();

                    Task::perform(
                        async move { AuthService::refresh_access_token(&refresh_token).await },
                        |response| {
                            Message::Auth(app::message::AuthMessage::AccessTokenRefreshed(response))
                        },
                    )
                }
            }
            _ => Task::none(),
        }
    }

    fn handle_org_messages(&mut self, message: app::message::OrgMessage) -> Task<Message> {
        match message {
            OrgMessage::InvitationsLoaded(Ok(invitations)) => {
                if let Screen::OrgSelection(screen) = &mut self.app.screen {
                    screen.invitations = invitations.clone();
                    screen.loading = false;
                }
                Task::none()
            }
            OrgMessage::OrgCreated(Ok(root_folder_entry)) => {
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
                self.app.screen = Screen::InviteMembers(screens::invite_members::InviteMembersScreen::new(org_id));

                Task::none()
            }
            OrgMessage::OrgJoined(Ok(_)) => todo!(),
            OrgMessage::InviteSent(Ok(_)) => todo!(),
            OrgMessage::InviteUserFinished {
                run_id,
                email,
                result,
            } => {
                if let Screen::InviteMembers(screen) = &mut self.app.screen {
                    match result {
                        Ok(()) => screen.push_history(
                            run_id,
                            email,
                            screens::invite_members::InviteStatus::Sent,
                        ),
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
                        return Self::invite_user_task(
                            run_id,
                            next_email,
                            org_id,
                            access_token,
                        );
                    }

                    screen.finish_current_email();
                }

                Task::none()
            }
            OrgMessage::OrgJoined(Err(e))
            | OrgMessage::InviteSent(Err(e))
            | OrgMessage::InvitationsLoaded(Err(e))
            | OrgMessage::OrgCreated(Err(e)) => self.handle_error(e.into()),
        }
    }

    fn handle_auth_messages(&mut self, message: app::message::AuthMessage) -> Task<Message> {
        match message {
            app::message::AuthMessage::AccessTokenRefreshed(Err(_))
            | app::message::AuthMessage::SignedOut => self.re_auth(),
            app::message::AuthMessage::AccessTokenRefreshed(Ok(refreshed_token)) => {
                self.app.session.user.access_token = refreshed_token.access_token;
                self.app.session.user.token_type = refreshed_token.token_type;
                self.app.session.user.expires_at = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    + refreshed_token.expires_in;

                LocalStorageService::save_object(
                    &self.app.session.user,
                    services::local_storage::ObjectType::UserProfile,
                );

                if let Some(intent) = self.app.retry_intent {
                    self.run_intent(intent)
                } else {
                    Task::none()
                }
            }
            app::message::AuthMessage::AccessTokenReceived(Err(auth_error)) => {
                if let Screen::SignIn(screen) = &mut self.app.screen {
                    screen.error = Some(auth_error.into());
                }
                Task::none()
            }
            app::message::AuthMessage::AccessTokenReceived(Ok(access_token)) => {
                self.app.session.auth = app::state::AuthState::SignedIn;
                let email = services::auth::AuthService::extract_email_from_access_token(
                    &access_token.id_token,
                );

                let user_email = email.clone();
                let user_profile = UserProfile {
                    email,
                    scopes: access_token
                        .scope
                        .split(" ")
                        .map(|s| s.to_string())
                        .collect(),
                    refresh_token: access_token.refresh_token,
                    expires_at: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                        + access_token.expires_in,
                    token_type: access_token.token_type,
                    access_token: access_token.access_token,
                };

                LocalStorageService::save_object(
                    &user_profile,
                    services::local_storage::ObjectType::UserProfile,
                );

                self.app.session.user = user_profile;
                self.app.session.auth = app::state::AuthState::SignedIn;

                // Navigate to organization selection screen
                self.app.screen =
                    Screen::OrgSelection(screens::org_selection::OrgSelectionScreen::new());
                self.app.retry_intent = Some(app::state::Intent::FetchInvitations);

                // Fetch invitations for the user
                Self::fetch_invitations_task(user_email, self.app.session.user.access_token.clone())
            }
        }
    }

    // changes the state, switch screen if needed
    pub fn update(&mut self, message: Message) -> Task<Message> {
        println!("{message:?}");

        match message {
            Message::Screen(ScreenMessage::Login(
                msg @ screens::signin::Message::SignInClicked,
            )) => {
                if let Screen::SignIn(screen) = &mut self.app.screen {
                    screen.update(msg.clone());
                    Task::perform(AuthService::get_drive_access_token(), |access_token| {
                        Message::Auth(app::message::AuthMessage::AccessTokenReceived(access_token))
                    })
                } else {
                    Task::none()
                }
            }
            Message::Screen(ScreenMessage::Login(msg @ screens::signin::Message::ClearError)) => {
                if let Screen::SignIn(screen) = &mut self.app.screen {
                    screen.update(msg);
                }

                Task::none()
            }
            Message::Auth(auth_msg) => self.handle_auth_messages(auth_msg),
            Message::Org(org_msg) => self.handle_org_messages(org_msg),
            Message::Screen(ScreenMessage::OrgSelection(
                msg @ screens::org_selection::Message::CreateOrgClicked,
            )) => {
                if let Screen::OrgSelection(screen) = &mut self.app.screen {
                    screen.update(msg.clone());
                    self.app.retry_intent = Some(Intent::CreateOrg);

                    Task::perform(
                        OrgService::get_or_create_organization(
                            self.app.session.user.access_token.clone(),
                            self.app.session.user.email.clone(),
                        ),
                        |organisation| Message::Org(OrgMessage::OrgCreated(organisation)),
                    )
                } else {
                    Task::none()
                }
            }
            Message::Screen(ScreenMessage::OrgSelection(
                screens::org_selection::Message::JoinOrgClicked(org_id),
            )) => {
                // TODO: Join the organization with the given org_id
                println!("Join organization clicked: {}", org_id);
                Task::none()
            }
            Message::Screen(ScreenMessage::InviteMembers(msg)) => {
                match msg {
                    screen_msg @ screens::invite_members::Message::Edit(_) => {
                        if let Screen::InviteMembers(screen) = &mut self.app.screen {
                            screen.update(screen_msg);
                        }
                        Task::none()
                    }
                    screens::invite_members::Message::SendInvitesClicked => {
                        if let Screen::InviteMembers(screen) = &mut self.app.screen {
                            let Some(run_id) = screen.begin_run() else {
                                return Task::none();
                            };

                            let Some(email) = screen.pop_next_email() else {
                                screen.finish_current_email();
                                return Task::none();
                            };

                            let org_id = screen.org_id.clone();
                            let access_token = self.app.session.user.access_token.clone();
                            return Self::invite_user_task(run_id, email, org_id, access_token);
                        }
                        Task::none()
                    }
                    screens::invite_members::Message::ContinueClicked => {
                        let can_continue = if let Screen::InviteMembers(screen) = &self.app.screen {
                            screen.can_continue()
                        } else {
                            false
                        };

                        if !can_continue {
                            return Task::none();
                        }

                        self.app.screen =
                            Screen::OrgSelection(screens::org_selection::OrgSelectionScreen::new());

                        let email = self.app.session.user.email.clone();
                        let access_token = self.app.session.user.access_token.clone();
                        Self::fetch_invitations_task(email, access_token)
                    }
                }
            }
        }
    }
}
