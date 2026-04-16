use std::collections::VecDeque;

use iced::alignment::{Horizontal, Vertical};
use iced::widget::table;
use iced::widget::{button, column, container, row, scrollable, text, text_editor, tooltip};
use iced::{Alignment, Border, Color, Element, Length, Theme};

use crate::app::message::ScreenMessage;

#[derive(Debug, Clone)]
pub enum Message {
    RefreshClicked,
    InviteMembersClicked,
    InviteEdit(text_editor::Action),
    InviteSendClicked,
    InviteDoneClicked,
    RemoveAccessClicked {
        email: String,
        folder_id: String,
        permission_id: Option<String>,
    },
    InviteNextEmail,
    InviteFinishEmail,
    RecordInviteInLog {
        run_id: u64,
        email: String,
        status: InviteStatus,
    },
    DashboardRowsLoaded {
        rows: Vec<DashboardRow>,
    },
    StopRemoveAccessAction {
        folder_id: String,
    },
    RemoveAccessRow {
        folder_id: String,
    },
    ShowError {
        error: String,
    },
    AddRow {
        row: DashboardRow,
    },
}

impl Into<ScreenMessage> for Message {
    fn into(self) -> ScreenMessage {
        ScreenMessage::OrgDashboard(self)
    }
}

#[derive(Debug, Clone)]
pub struct DashboardRow {
    pub email: String,
    pub folder_id: String,
    pub active: bool,
    pub permission_id: Option<String>,

    pub removing: bool,
}

#[derive(Debug, Clone)]
pub enum InviteStatus {
    Sent,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct InviteHistoryRow {
    pub run_id: u64,
    pub email: String,
    pub status: InviteStatus,
}

#[derive(Debug)]
pub struct OrgDashboardScreen {
    loading: bool,
    error: Option<String>,
    rows: Vec<DashboardRow>,

    // Invite panel
    pub show_invite_panel: bool,
    invite_editor: text_editor::Content,
    invite_sending: bool,
    invite_active_run_id: Option<u64>,
    invite_next_run_id: u64,
    invite_queue: VecDeque<String>,
    invite_current_email: Option<String>,
    invite_history: Vec<InviteHistoryRow>,
}

impl OrgDashboardScreen {
    pub fn new() -> Self {
        Self {
            loading: true,
            error: None,
            rows: Vec::new(),

            show_invite_panel: false,
            invite_editor: text_editor::Content::new(),
            invite_sending: false,
            invite_active_run_id: None,
            invite_next_run_id: 1,
            invite_queue: VecDeque::new(),
            invite_current_email: None,
            invite_history: Vec::new(),
        }
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::InviteMembersClicked => {
                self.show_invite_panel = !self.show_invite_panel;
            }
            Message::InviteEdit(action) => self.invite_editor.perform(action.clone()),
            Message::RefreshClicked => {
                self.loading = true;
                self.error = None;
            }
            Message::RemoveAccessClicked { folder_id, .. } => {
                self.set_removing(&folder_id, true);
            }
            Message::InviteDoneClicked => {
                self.show_invite_panel = false;
                self.loading = true;
                self.error = None;
            }
            Message::InviteSendClicked => {
                if self.invite_sending {
                    return;
                }
                let emails = parse_emails(&self.invite_editor.text());
                if emails.is_empty() {
                    return;
                }
                let run_id = self.invite_next_run_id;
                self.invite_next_run_id += 1;

                self.invite_active_run_id = Some(run_id);
                self.invite_sending = true;
                self.invite_queue = emails.into_iter().collect();
                self.invite_current_email = self.invite_queue.pop_front();
            }
            Message::InviteNextEmail => {
                self.invite_current_email = self.invite_queue.pop_front();
            }
            Message::InviteFinishEmail => {
                self.invite_current_email = None;
                if self.invite_queue.is_empty() {
                    self.invite_sending = false;
                }
            }
            Message::RecordInviteInLog {
                run_id,
                email,
                status,
            } => {
                self.invite_history.push(InviteHistoryRow {
                    run_id,
                    email,
                    status,
                });
            }
            Message::DashboardRowsLoaded { rows } => {
                self.set_rows(rows);
            }
            Message::StopRemoveAccessAction { folder_id } => {
                self.set_removing(&folder_id, false);
            }
            Message::RemoveAccessRow { folder_id } => {
                if let Some(row) = self.rows.iter_mut().find(|r| r.folder_id == folder_id) {
                    row.permission_id = None;
                }
            }
            Message::ShowError { error } => {
                self.set_error(error);
            }
            Message::AddRow { row } => {
                self.rows.push(row);
            }
        }
    }

    pub fn invite_current_task(&self) -> Option<(u64, String)> {
        let run_id = self.invite_active_run_id?;
        let email = self.invite_current_email.clone()?;

        Some((run_id, email))
    }

    pub fn invite_current_run_stats(&self) -> (usize, usize) {
        let Some(run_id) = self.invite_active_run_id else {
            return (0, 0);
        };

        let mut sent = 0;
        let mut errors = 0;

        for item in self.invite_history.iter().filter(|h| h.run_id == run_id) {
            match &item.status {
                InviteStatus::Sent => sent += 1,
                InviteStatus::Error(_) => errors += 1,
            }
        }

        (sent, errors)
    }

    pub fn invite_can_done(&self) -> bool {
        if self.invite_sending {
            return false;
        }

        let (sent, errors) = self.invite_current_run_stats();
        sent >= 1 && errors == 0
    }

    pub fn invite_done_hint(&self) -> String {
        if self.invite_can_done() {
            return "".to_string();
        }

        if self.invite_sending {
            return "Sending invites…".to_string();
        }

        let (sent, errors) = self.invite_current_run_stats();

        if errors > 0 {
            "Fix invite errors to close.".to_string()
        } else if sent == 0 {
            "Send at least one invite to close.".to_string()
        } else {
            "Complete the invite run to close.".to_string()
        }
    }

    fn set_rows(&mut self, rows: Vec<DashboardRow>) {
        self.rows = rows;
        self.loading = false;
        self.error = None;
    }

    fn set_error(&mut self, error: String) {
        self.loading = false;
        self.error = Some(error);
    }

    fn set_removing(&mut self, folder_id: &str, removing: bool) {
        if let Some(row) = self.rows.iter_mut().find(|r| r.folder_id == folder_id) {
            row.removing = removing;
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let title = text("Dashboard").size(32);
        let subtitle = text("Manage access and see who is active.").size(14);

        let panel_style = |theme: &Theme| {
            let palette = theme.extended_palette();

            container::Style {
                border: Border {
                    color: palette.background.strong.color,
                    width: 1.0,
                    radius: 6.0.into(),
                },
                ..Default::default()
            }
        };

        let invite_toggle = button(if self.show_invite_panel {
            "Hide invites"
        } else {
            "Invite members"
        })
        .padding(12)
        .on_press(Message::InviteMembersClicked);

        let refresh = button(if self.loading {
            "Loading…"
        } else {
            "Refresh"
        })
        .padding(12)
        .on_press(Message::RefreshClicked);

        let status_line: Element<Message> = if let Some(err) = &self.error {
            text(format!("Error: {err}")).size(12).into()
        } else if self.loading {
            text("Loading users…").size(12).into()
        } else {
            text("").into()
        };

        let invite_panel: Element<Message> = if self.show_invite_panel {
            render_invite_panel(self)
        } else {
            column![].into()
        };

        let table_element = render_table(&self.rows);

        let table_panel = container(scrollable(table_element).width(Length::Fill))
            .padding(6)
            .height(Length::Fill)
            .width(Length::Fill)
            .style(panel_style);

        let content = column![
            row![
                column![title, subtitle].spacing(6).width(Length::Fill),
                row![invite_toggle, refresh].spacing(10),
            ]
            .align_y(Alignment::Center)
            .spacing(12)
            .width(Length::Fill),
            invite_panel,
            status_line,
            table_panel,
        ]
        .spacing(12)
        .width(Length::Fill)
        .align_x(Alignment::Center);

        let content = container(content)
            .padding(24)
            .width(Length::Fill)
            .max_width(1100);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Center)
            .into()
    }
}

fn render_invite_panel(screen: &OrgDashboardScreen) -> Element<'_, Message> {
    const PANEL_HEIGHT: f32 = 220.0;

    let panel_style = |theme: &Theme| {
        let palette = theme.extended_palette();

        container::Style {
            border: Border {
                color: palette.background.strong.color,
                width: 1.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        }
    };

    let mut editor_widget = text_editor(&screen.invite_editor)
        .placeholder("alice@example.com\nbob@example.com")
        .height(Length::Fill)
        .style(|theme, status| {
            let mut style = text_editor::default(theme, status);
            style.border = Border {
                width: 0.0,
                color: Color::TRANSPARENT,
                radius: 0.0.into(),
            };
            style
        });

    if !screen.invite_sending {
        editor_widget = editor_widget.on_action(Message::InviteEdit);
    }

    let mut send_button = button(if screen.invite_sending {
        "Sending…"
    } else {
        "Send invites"
    })
    .padding(12);

    if !screen.invite_sending && !parse_emails(&screen.invite_editor.text()).is_empty() {
        send_button = send_button.on_press(Message::InviteSendClicked);
    }

    let done_enabled = screen.invite_can_done();
    let done_hint = screen.invite_done_hint();
    let mut done_button = button("Done").padding(12);

    let done_with_tooltip = if done_enabled {
        done_button = done_button.on_press(Message::InviteDoneClicked);

        tooltip(
            done_button,
            container(text(done_hint)).padding(0),
            tooltip::Position::Top,
        )
    } else {
        tooltip(
            done_button,
            container(text(done_hint).size(11))
                .padding(10)
                .style(container::rounded_box),
            tooltip::Position::Top,
        )
    };

    let current: Element<Message> = if let Some(email) = &screen.invite_current_email {
        text(format!("Currently sending: {email}")).size(12).into()
    } else {
        text("").into()
    };

    let editor_panel = container(editor_widget)
        .padding(6)
        .height(Length::Fixed(PANEL_HEIGHT))
        .width(Length::Fill)
        .style(panel_style);

    let history_list = render_invite_history(&screen.invite_history);

    let history_table = container(
        scrollable(history_list)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .padding(6)
    .height(Length::Fixed(PANEL_HEIGHT))
    .width(Length::Fill)
    .style(panel_style);

    let left = column![
        text("Invitees").size(18),
        editor_panel,
        row![send_button, done_with_tooltip].spacing(10),
        current,
    ]
    .spacing(10)
    .width(Length::Fill);

    let right = column![text("History").size(18), history_table]
        .spacing(10)
        .width(Length::Fill);

    row![
        container(left).width(Length::FillPortion(2)),
        container(right).width(Length::FillPortion(3)),
    ]
    .spacing(18)
    .width(Length::Fill)
    .align_y(Alignment::Start)
    .into()
}

fn render_invite_history(history: &[InviteHistoryRow]) -> Element<'_, Message> {
    if history.is_empty() {
        return column![text("No invites sent yet.").size(12)].into();
    }

    let mut rows: Vec<InviteHistoryRow> = history.to_vec();
    rows.reverse();

    let columns = [
        table::column(text("Attempt").size(14), |row: InviteHistoryRow| {
            text(row.run_id.to_string()).size(12)
        })
        .width(Length::Fixed(90.0))
        .align_x(Horizontal::Left),
        table::column(text("Email").size(14), |row: InviteHistoryRow| {
            text(row.email).size(12)
        })
        .width(Length::Fill)
        .align_x(Horizontal::Left),
        table::column(text("Status").size(14), |row: InviteHistoryRow| {
            let status = match row.status {
                InviteStatus::Sent => "Sent".to_string(),
                InviteStatus::Error(e) => format!("Error: {e}"),
            };
            text(status).size(12)
        })
        .width(Length::Fixed(200.0))
        .align_x(Horizontal::Left),
    ];

    table::Table::new(columns, rows)
        .width(Length::Fill)
        .padding_x(8)
        .padding_y(6)
        .separator(1)
        .into()
}

fn parse_emails(input: &str) -> Vec<String> {
    let mut emails: Vec<String> = input
        .split(|c| c == '\n' || c == ',' || c == ';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    let mut deduped: Vec<String> = Vec::new();
    for email in emails.drain(..) {
        if !deduped.iter().any(|e| e.eq_ignore_ascii_case(&email)) {
            deduped.push(email);
        }
    }

    deduped
}

fn render_table(rows: &[DashboardRow]) -> Element<'_, Message> {
    if rows.is_empty() {
        return column![text("No users found.").size(12)].into();
    }

    let columns = [
        table::column(text("Email").size(14), |row: DashboardRow| {
            text(row.email).size(12)
        })
        .width(Length::Fill)
        .align_x(Horizontal::Left),
        table::column(text("Status").size(14), |row: DashboardRow| {
            let status = if row.permission_id.is_none() {
                "Revoked".to_string()
            } else if row.active {
                "Active".to_string()
            } else {
                "Invited".to_string()
            };

            text(status).size(12)
        })
        .width(Length::Fixed(120.0))
        .align_x(Horizontal::Left),
        table::column(text("Action").size(14), |row: DashboardRow| {
            let mut b = button(if row.removing {
                "Removing…"
            } else {
                "Remove access"
            })
            .padding(8);

            if !row.removing && row.permission_id.is_some() {
                b = b.on_press(Message::RemoveAccessClicked {
                    email: row.email,
                    folder_id: row.folder_id,
                    permission_id: row.permission_id,
                });
            }

            b
        })
        .width(Length::Fixed(160.0))
        .align_x(Horizontal::Left),
    ];

    table::Table::new(columns, rows.to_vec())
        .width(Length::Fill)
        .padding_x(8)
        .padding_y(6)
        .separator(1)
        .into()
}
