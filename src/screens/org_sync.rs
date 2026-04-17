use iced::alignment::{Horizontal, Vertical};
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length};

use crate::app::message::ScreenMessage;

#[derive(Debug, Clone)]
pub enum Message {
    LocalFolderChanged(String),
    SaveMappingClicked,
    StartWatchingClicked,
    StopWatchingClicked,
    ClearLogClicked,
}

impl From<Message> for ScreenMessage {
    fn from(val: Message) -> Self {
        ScreenMessage::OrgSync(val)
    }
}

#[derive(Debug, Default)]
pub struct OrgSyncScreen {
    pub local_folder_input: String,

    pub mapped_folder: Option<String>,
    pub watching: bool,

    pub status_line: Option<String>,
    pub upload_log: Vec<String>,
}

impl OrgSyncScreen {
    pub fn new(mapped_folder: Option<String>) -> Self {
        Self {
            local_folder_input: mapped_folder.clone().unwrap_or_default(),
            mapped_folder,
            watching: false,
            status_line: None,
            upload_log: Vec::new(),
        }
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::LocalFolderChanged(value) => {
                self.local_folder_input = value;
            }
            Message::SaveMappingClicked => {
                // handled by app; keep UI responsive
                self.status_line = Some("Mapping saved.".to_string());
            }
            Message::StartWatchingClicked => {
                self.watching = true;
                self.status_line = Some("Watching for changes…".to_string());
            }
            Message::StopWatchingClicked => {
                self.watching = false;
                self.status_line = Some("Watcher stopped.".to_string());
            }
            Message::ClearLogClicked => {
                self.upload_log.clear();
            }
        }
    }

    pub fn push_log(&mut self, line: impl Into<String>) {
        self.upload_log.push(line.into());
        if self.upload_log.len() > 250 {
            let overflow = self.upload_log.len() - 250;
            self.upload_log.drain(0..overflow);
        }
    }

    pub fn view(&self, org_name: &str) -> Element<'_, Message> {
        let title = text("Folder sync").size(32);
        let subtitle = text(format!("Org: {org_name}")).size(14);

        let mapping_row = row![
            text("Local folder:").size(14),
            text_input("/path/to/folder", &self.local_folder_input)
                .on_input(Message::LocalFolderChanged)
                .width(Length::Fill),
            button("Save")
                .padding(10)
                .on_press(Message::SaveMappingClicked),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .width(Length::Fill);

        let watch_button = if self.watching {
            button("Stop watching")
                .padding(12)
                .on_press(Message::StopWatchingClicked)
        } else {
            button("Start watching")
                .padding(12)
                .on_press(Message::StartWatchingClicked)
        };

        let clear_log = button("Clear log")
            .padding(12)
            .on_press(Message::ClearLogClicked);

        let status_line: Element<Message> = match &self.status_line {
            Some(s) => text(s).size(12).into(),
            None => text("").into(),
        };

        let log_items = self
            .upload_log
            .iter()
            .rev()
            .take(200)
            .fold(column![].spacing(6), |col, line| {
                col.push(text(line).size(12))
            });

        let log_panel = container(scrollable(log_items).height(Length::Fill))
            .padding(12)
            .width(Length::Fill)
            .height(Length::Fill);

        let content = column![
            title,
            subtitle,
            mapping_row,
            row![watch_button, clear_log].spacing(10),
            status_line,
            log_panel,
        ]
        .spacing(12)
        .width(Length::Fill)
        .align_x(Alignment::Center);

        let content = container(content)
            .padding(24)
            .width(Length::Fill)
            .max_width(900);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Center)
            .into()
    }
}
