use iced::Task;

use crate::{
    ArchiveClient, UserState,
    app::{
        message::{Message, ScreenMessage},
        state::{Intent, OrgState, Role, Screen, UserProfile},
    },
    screens,
    services::local_storage::{LocalStorageService, ObjectType},
};

impl ArchiveClient {
    pub fn handle_message(&mut self, message: Message) -> Task<Message> {
        match (&mut self.app.user_state, &mut self.screen, message) {
            (
                UserState::SignedOut,
                Screen::SignIn(screen),
                Message::Screen(ScreenMessage::Login(
                    msg @ screens::signin::Message::SignInClicked,
                )),
            ) => {
                screen.update(msg);
                ArchiveClient::get_access_token_task()
            }
            (
                UserState::SignedOut,
                Screen::SignIn(screen),
                Message::Screen(ScreenMessage::Login(msg @ screens::signin::Message::ClearError)),
            ) => {
                screen.update(msg);
                Task::none()
            }
            (_, _, Message::UnixSocket(cmd)) => self.handle_unix_socket_commands(cmd),
            (_, _, Message::Auth(auth_msg)) => self.handle_auth_messages(auth_msg),
            (user_state, _, Message::Org(org_msg))
                if !matches!(user_state, UserState::SignedOut) =>
            {
                self.handle_org_messages(org_msg)
            }
            (
                UserState::OrgJoined { .. } | UserState::OrgSynced { .. },
                _,
                Message::Sync(sync_msg),
            ) => self.handle_sync_messages(sync_msg),
            (
                UserState::SignedIn { user_data },
                Screen::OrgSelection(screen),
                Message::Screen(ScreenMessage::OrgSelection(
                    msg @ screens::org_selection::Message::CreateOrgClicked,
                )),
            ) => {
                screen.update(msg.clone());
                self.app.pending_intents.push(Intent::CreateOrg);

                ArchiveClient::get_or_create_organization_task(
                    user_data.email.clone(),
                    user_data.access_token.clone(),
                )
            }
            (
                UserState::SignedIn { .. },
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

                // Keep any existing mapping if present (e.g. user re-joins same org id).
                let mut org = LocalStorageService::load_object::<OrgState>(ObjectType::Org)
                    .unwrap_or_default();
                org.status = crate::app::state::OrgStatus::Created;
                org.config.archive_folder_id = org_id.clone();
                org.config.archive_folder_name = org_name;
                LocalStorageService::save_object(&org, ObjectType::Org);

                self.app.user_state.org_joined(org_id);

                LocalStorageService::update_object::<UserProfile, _>(
                    ObjectType::UserProfile,
                    |user| {
                        user.role = Some(Role::User);
                    },
                );

                self.screen = Screen::OrgSync(screens::org_sync::OrgSyncScreen::new(
                    org.config.local_folder_path,
                ));

                Task::none()
            }
            (
                UserState::OrgCreated { .. },
                Screen::OrgDashboard(screen),
                Message::Screen(ScreenMessage::OrgDashboard(
                    msg @ screens::org_dashboard::Message::InviteMembersClicked,
                )),
            ) => {
                screen.update(msg);
                Task::none()
            }
            (
                UserState::OrgCreated { .. },
                Screen::OrgDashboard(screen),
                Message::Screen(ScreenMessage::OrgDashboard(
                    msg @ screens::org_dashboard::Message::InviteEdit(_),
                )),
            ) => {
                screen.update(msg);
                Task::none()
            }
            (
                UserState::OrgCreated { org_id, user_data },
                Screen::OrgDashboard(screen),
                Message::Screen(ScreenMessage::OrgDashboard(
                    msg @ screens::org_dashboard::Message::InviteSendClicked,
                )),
            ) => {
                screen.update(msg);

                // Decide whether we can kick off the async work:
                let Some((run_id, email)) = screen.invite_current_task() else {
                    return Task::none();
                };

                let access_token = user_data.access_token.clone();

                self.app.pending_intents.push(Intent::SendInvitations {
                    run_id,
                    org_id: org_id.clone(),
                    email: email.clone(),
                });

                Self::invite_user_task(run_id, email, org_id.clone(), access_token)
            }
            (
                UserState::OrgCreated { org_id, user_data },
                Screen::OrgDashboard(screen),
                Message::Screen(ScreenMessage::OrgDashboard(
                    msg @ screens::org_dashboard::Message::InviteDoneClicked,
                )),
            ) => {
                if screen.invite_can_done() {
                    screen.update(msg);

                    let access_token = user_data.access_token.clone();
                    self.app.pending_intents.push(Intent::LoadDashboard {
                        org_id: org_id.clone(),
                    });

                    Self::load_dashboard_task(org_id.clone(), access_token)
                } else {
                    Task::none()
                }
            }
            (
                UserState::OrgCreated { org_id, user_data },
                Screen::OrgDashboard(screen),
                Message::Screen(ScreenMessage::OrgDashboard(
                    msg @ screens::org_dashboard::Message::RefreshClicked,
                )),
            ) => {
                screen.update(msg);

                let access_token = user_data.access_token.clone();
                self.app.pending_intents.push(Intent::LoadDashboard {
                    org_id: org_id.clone(),
                });

                Self::load_dashboard_task(org_id.clone(), access_token)
            }
            (
                UserState::OrgCreated { org_id, user_data },
                Screen::OrgDashboard(screen),
                Message::Screen(ScreenMessage::OrgDashboard(
                    msg @ screens::org_dashboard::Message::FetchAuditLogClicked,
                )),
            ) => {
                let page_token = screen.audit_log_next_page.clone();
                screen.update(msg);
                Self::fetch_audit_log_task(
                    org_id.clone(),
                    user_data.access_token.clone(),
                    page_token,
                )
            }
            (
                UserState::OrgCreated { user_data, .. },
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

                Self::revoke_permission_task(
                    folder_id,
                    email,
                    permission_id,
                    user_data.access_token.clone(),
                )
            }
            (
                UserState::OrgJoined { .. },
                Screen::OrgSync(screen),
                Message::Screen(ScreenMessage::OrgSync(
                    msg @ screens::org_sync::Message::LocalFolderChanged(_),
                )),
            ) => {
                screen.update(msg);
                Task::none()
            }
            (
                UserState::OrgJoined { .. } | UserState::OrgSynced { .. },
                Screen::OrgSync(screen),
                Message::Screen(ScreenMessage::OrgSync(
                    msg @ screens::org_sync::Message::ClearLogClicked,
                )),
            ) => {
                screen.update(msg);
                Task::none()
            }
            (
                UserState::OrgJoined { .. } | UserState::OrgSynced { .. },
                Screen::OrgSync(screen),
                Message::Screen(ScreenMessage::OrgSync(
                    msg @ screens::org_sync::Message::SaveMappingClicked,
                )),
            ) => {
                screen.update(msg);

                let input = screen.local_folder_input.trim().to_string();
                if input.is_empty() {
                    screen.status_line = Some("Enter a folder path first.".to_string());
                    return Task::none();
                }

                let path = std::path::PathBuf::from(&input);
                if !path.exists() || !path.is_dir() {
                    screen.status_line =
                        Some("Folder does not exist (or is not a directory).".to_string());
                    return Task::none();
                }

                LocalStorageService::update_object::<OrgState, _>(ObjectType::Org, |org| {
                    org.config.local_folder_path = Some(input.clone());
                });
                screen.mapped_folder = Some(input);

                Task::none()
            }
            (
                _,
                Screen::OrgSync(_),
                Message::Screen(ScreenMessage::OrgSync(
                    screens::org_sync::Message::BrowseFolderClicked,
                )),
            ) => Task::perform(
                async {
                    tokio::task::spawn_blocking(|| {
                        let out = std::process::Command::new("osascript")
                            .args(["-e", "POSIX path of (choose folder)"])
                            .output()
                            .ok()?;
                        out.status
                            .success()
                            .then(|| String::from_utf8(out.stdout).ok())
                            .flatten()
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                    })
                    .await
                    .ok()
                    .flatten()
                },
                |path| {
                    Message::Screen(ScreenMessage::OrgSync(
                        screens::org_sync::Message::FolderSelected(path),
                    ))
                },
            ),
            (
                user_state @ (UserState::OrgJoined { .. } | UserState::OrgSynced { .. }),
                Screen::OrgSync(screen),
                Message::Screen(ScreenMessage::OrgSync(
                    screens::org_sync::Message::FolderSelected(Some(path)),
                )),
            ) => {
                println!("folder selected");
                let path_buf = std::path::PathBuf::from(&path);
                if !path_buf.exists() || !path_buf.is_dir() {
                    return Task::none();
                }

                screen.update(screens::org_sync::Message::FolderSelected(Some(
                    path.clone(),
                )));

                LocalStorageService::update_object::<OrgState, _>(ObjectType::Org, |org| {
                    org.config.local_folder_path = Some(path.clone());
                });

                let _ = std::process::Command::new("open").arg(&path).spawn();

                let watch_task = if let UserState::OrgJoined {
                    root_folder_id,
                    user_data,
                    ..
                } = user_state
                {
                    Self::initial_sync_task(
                        user_data.access_token.clone(),
                        path_buf,
                        root_folder_id.clone(),
                    )
                } else {
                    Task::none()
                };

                let minimize_task = iced::window::latest().then(|maybe_id| {
                    maybe_id
                        .map(|id| iced::window::minimize::<Message>(id, true))
                        .unwrap_or(Task::none())
                });

                Task::batch([watch_task, minimize_task])
            }
            (
                _,
                Screen::OrgSync(_),
                Message::Screen(ScreenMessage::OrgSync(
                    screens::org_sync::Message::FolderSelected(None),
                )),
            ) => Task::none(),
            _ => Task::none(),
        }
    }
}
