use std::collections::VecDeque;

use iced::alignment::{Horizontal, Vertical};
use iced::widget::{button, column, container, row, scrollable, text, text_editor};
use iced::{Alignment, Element, Length};

use crate::app::message::ScreenMessage;

#[derive(Debug, Clone)]
pub enum Message {
    EditorAction(text_editor::Action),
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
            Message::EditorAction(action) => {
                if !self.sending {
                    self.editor.perform(action);
                }
            }
            Message::SendInvitesClicked | Message::ContinueClicked => {}
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

        let mut editor_widget = text_editor(&self.editor)
            .placeholder("alice@example.com\nbob@example.com")
            .height(Length::Fixed(140.0));

        // If on_action is not set, the text editor is disabled.
        if !self.sending {
            editor_widget = editor_widget.on_action(Message::EditorAction);
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

        let continue_enabled = self.can_continue();
        let mut continue_button = button("Continue").padding(12);
        if continue_enabled {
            continue_button = continue_button.on_press(Message::ContinueClicked);
        }

        let hint: Element<Message> = if let Some(hint) = self.continue_hint() {
            text(hint).size(12).into()
        } else {
            text("").into()
        };

        let current: Element<Message> = if let Some(email) = &self.current_email {
            text(format!("Currently sending: {email}"))
                .size(12)
                .into()
        } else {
            text("").into()
        };

        let history_list = render_history(&self.history);

        let content = column![
            title,
            subtitle,
            editor_widget,
            row![send_button].spacing(10),
            current,
            text("History").size(18),
            container(scrollable(history_list)).max_height(260).width(Length::Fill),
            row![continue_button].spacing(10),
            hint,
        ]
        .spacing(12)
        .align_x(Alignment::Center);

        let content = container(content)
            .padding(24)
            .width(Length::Shrink)
            .max_width(600);

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

    let mut col = column![].spacing(6);

    // Render newest-first, grouped by run id.
    let mut items: Vec<InviteHistoryRow> = history.to_vec();
    items.reverse();

    let mut last_run: Option<u64> = None;
    for item in items {
        if last_run != Some(item.run_id) {
            last_run = Some(item.run_id);
            col = col.push(text(format!("Attempt #{}", item.run_id)).size(14));
        }

        let status = match item.status {
            InviteStatus::Sent => "Sent".to_string(),
            InviteStatus::Error(e) => format!("Error: {e}"),
        };

        col = col.push(
            row![text(item.email).size(12), text(status).size(12)]
                .spacing(10)
                .width(Length::Fill),
        );
    }

    col.into()
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
