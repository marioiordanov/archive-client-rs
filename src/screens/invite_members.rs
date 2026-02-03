use std::collections::VecDeque;

use iced::alignment::{Horizontal, Vertical};
use iced::widget::table;
use iced::widget::{button, column, container, row, scrollable, text, text_editor, tooltip};
use iced::{Alignment, Border, Color, Element, Length, Theme};

use crate::app::message::ScreenMessage;

#[derive(Debug, Clone)]
pub enum Message {
    Edit(text_editor::Action),
    SendInvitesClicked,
    ContinueClicked,
}

impl Into<ScreenMessage> for Message {
    fn into(self) -> ScreenMessage {
        ScreenMessage::InviteMembers(self)
    }
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
pub struct InviteMembersScreen {
    pub org_id: String,

    pub editor: text_editor::Content,

    pub sending: bool,
    pub active_run_id: Option<u64>,
    pub next_run_id: u64,

    pub queue: VecDeque<String>,
    pub current_email: Option<String>,

    pub history: Vec<InviteHistoryRow>,
}

impl InviteMembersScreen {
    pub fn new(org_id: String) -> Self {
        Self {
            org_id,
            editor: text_editor::Content::new(),
            sending: false,
            active_run_id: None,
            next_run_id: 1,
            queue: VecDeque::new(),
            current_email: None,
            history: Vec::new(),
        }
    }

    pub fn update(&mut self, msg: Message) {
        match msg {
            Message::SendInvitesClicked | Message::ContinueClicked => {}
            Message::Edit(action) => self.editor.perform(action),
        }
    }

    pub fn begin_run(&mut self) -> Option<u64> {
        if self.sending {
            return None;
        }

        let emails = parse_emails(&self.editor.text());
        if emails.is_empty() {
            return None;
        }

        let run_id = self.next_run_id;
        self.next_run_id += 1;

        self.active_run_id = Some(run_id);
        self.sending = true;
        self.queue = emails.into_iter().collect();
        self.current_email = None;

        Some(run_id)
    }

    pub fn pop_next_email(&mut self) -> Option<String> {
        let email = self.queue.pop_front()?;
        self.current_email = Some(email.clone());
        Some(email)
    }

    pub fn finish_current_email(&mut self) {
        self.current_email = None;

        if self.queue.is_empty() {
            self.sending = false;
        }
    }

    pub fn push_history(&mut self, run_id: u64, email: String, status: InviteStatus) {
        self.history.push(InviteHistoryRow {
            run_id,
            email,
            status,
        });
    }

    pub fn current_run_stats(&self) -> (usize, usize) {
        let Some(run_id) = self.active_run_id else {
            return (0, 0);
        };

        let mut sent = 0;
        let mut errors = 0;

        for item in self.history.iter().filter(|h| h.run_id == run_id) {
            match &item.status {
                InviteStatus::Sent => sent += 1,
                InviteStatus::Error(_) => errors += 1,
            }
        }

        (sent, errors)
    }

    pub fn can_continue(&self) -> bool {
        if self.sending {
            return false;
        }

        let (sent, errors) = self.current_run_stats();
        sent >= 1 && errors == 0
    }

    pub fn continue_hint(&self) -> Option<String> {
        if self.can_continue() {
            return None;
        }

        if self.sending {
            return Some("Sending invites…".to_string());
        }

        let (sent, errors) = self.current_run_stats();

        if errors > 0 {
            Some("Fix invite errors to continue.".to_string())
        } else if sent == 0 {
            Some("Send at least one invite to continue.".to_string())
        } else {
            Some("Complete the invite run to continue.".to_string())
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let title = text("Invite members").size(32);
        let subtitle = text("Enter one email per line, or comma-separated.").size(14);

        const PANEL_HEIGHT: f32 = 260.0;

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

        let mut editor_widget = text_editor(&self.editor)
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

        // If on_action is not set, the text editor is disabled.
        if !self.sending {
            editor_widget = editor_widget.on_action(Message::Edit);
        }

        let mut send_button = button(if self.sending {
            "Sending…"
        } else {
            "Send invites"
        })
        .padding(12);

        if !self.sending && !parse_emails(&self.editor.text()).is_empty() {
            send_button = send_button.on_press(Message::SendInvitesClicked);
        }

        let mut continue_button = button("Continue").padding(12);

        let continue_enabled = self.can_continue();
        let hint_text = self.continue_hint().unwrap_or_default();

        let continue_with_tooltip = if continue_enabled {
            continue_button = continue_button.on_press(Message::ContinueClicked);

            tooltip(
                continue_button,
                container(text(hint_text)).padding(0),
                tooltip::Position::Top,
            )
        } else {
            tooltip(
                continue_button,
                container(text(hint_text).size(11))
                    .padding(10)
                    .style(container::rounded_box),
                tooltip::Position::Top,
            )
        };

        let current: Element<Message> = if let Some(email) = &self.current_email {
            text(format!("Currently sending: {email}")).size(12).into()
        } else {
            text("").into()
        };

        let history_list = render_history(&self.history);

        let editor_panel = container(editor_widget)
            .padding(6)
            .height(Length::Fixed(PANEL_HEIGHT))
            .width(Length::Fill)
            .style(panel_style);

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
            row![send_button, continue_with_tooltip].spacing(10),
            current,
        ]
        .spacing(12)
        .width(Length::Fill);

        let right = column![text("History").size(18), history_table,]
            .spacing(12)
            .width(Length::Fill);

        let main = row![
            container(left).width(Length::FillPortion(2)),
            container(right).width(Length::FillPortion(3)),
        ]
        .spacing(18)
        .width(Length::Fill)
        .align_y(Alignment::Start);

        let content = column![
            title.width(Length::Fill).align_x(Horizontal::Left),
            subtitle.width(Length::Fill).align_x(Horizontal::Left),
            main,
            //row![continue_with_tooltip].spacing(10),
        ]
        .spacing(12)
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

fn render_history(history: &[InviteHistoryRow]) -> Element<'_, Message> {
    if history.is_empty() {
        return column![text("No invites sent yet.").size(12)].into();
    }

    // Render newest-first.
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

    // Cheap de-dupe while preserving order
    let mut deduped: Vec<String> = Vec::new();
    for email in emails.drain(..) {
        if !deduped.iter().any(|e| e.eq_ignore_ascii_case(&email)) {
            deduped.push(email);
        }
    }

    deduped
}
