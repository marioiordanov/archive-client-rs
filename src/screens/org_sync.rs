use iced::alignment::{Horizontal, Vertical};
use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Alignment, Element, Length};

use crate::app::message::{ScreenMessage};
use crate::app::handlers::external_commands::FileWithRevision;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Message {
    LocalFolderChanged(String),
    SaveMappingClicked,
    ClearLogClicked,
    BrowseFolderClicked,
    FolderSelected(Option<String>),
    DismissRevisions,
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

    pub status_line: Option<String>,
    pub upload_log: Vec<String>,

    pub revisions_panel: Option<Vec<FileWithRevision>>,
}

impl OrgSyncScreen {
    pub fn new(mapped_folder: Option<String>) -> Self {
        Self {
            local_folder_input: mapped_folder.clone().unwrap_or_default(),
            mapped_folder,
            status_line: None,
            upload_log: Vec::new(),
            revisions_panel: None,
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
            Message::ClearLogClicked => {
                self.upload_log.clear();
            }
            Message::FolderSelected(Some(path)) => {
                self.local_folder_input = path.clone();
                self.mapped_folder = Some(path);
            }
            Message::BrowseFolderClicked | Message::FolderSelected(None) => {}
            Message::DismissRevisions => {
                self.revisions_panel = None;
            }
        }
    }

    pub fn set_revisions(&mut self, revisions: Vec<FileWithRevision>) {
        self.revisions_panel = Some(revisions);
    }

    pub fn push_log(&mut self, line: impl Into<String>) {
        self.upload_log.push(line.into());
        if self.upload_log.len() > 250 {
            let overflow = self.upload_log.len() - 250;
            self.upload_log.drain(0..overflow);
        }
    }

    pub fn view(&self, org_name: &str) -> Element<'_, Message> {
        let main: Element<Message> = if let Some(folder) = &self.mapped_folder {
            let log_items = self
                .upload_log
                .iter()
                .rev()
                .take(200)
                .fold(column![].spacing(4), |col, line| {
                    col.push(text(line).size(12))
                });

            let log_header = row![
                text("Activity Log").size(13).width(Length::Fill),
                button("Clear")
                    .padding([4, 10])
                    .on_press(Message::ClearLogClicked),
            ]
            .align_y(Alignment::Center);

            column![
                text("You are all set!").size(28),
                text(format!("Tracking: {folder}")).size(13),
                text(format!("Org: {org_name}")).size(13),
                log_header,
                container(scrollable(log_items).height(Length::Fill))
                    .padding(8)
                    .width(Length::Fill)
                    .height(Length::Fill),
            ]
            .spacing(12)
            .padding(24)
            .width(Length::Fill)
            .into()
        } else {
            container(
                column![
                    text("Which folder would you like to track?").size(18),
                    button("Browse…")
                        .padding([10, 24])
                        .on_press(Message::BrowseFolderClicked),
                ]
                .spacing(20)
                .align_x(Alignment::Center),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Center)
            .into()
        };

        let content: Element<Message> = if let Some(revisions) = &self.revisions_panel {
            let revision_rows = revisions.iter().fold(column![].spacing(6), |col, r| {
                let size_label = r
                    .revision.size
                    .as_deref()
                    .map(|s| format!("  ({s} bytes)"))
                    .unwrap_or_default();
                col.push(text(format!("{}{}", r.revision.modified_time.format("%c"), size_label)).size(12))
            });

            let panel = column![
                row![
                    text("Revisions").size(15).width(Length::Fill),
                    button("Close")
                        .padding([4, 10])
                        .on_press(Message::DismissRevisions),
                ]
                .align_y(Alignment::Center),
                container(scrollable(revision_rows).height(Length::Fill))
                    .padding(8)
                    .width(Length::Fill)
                    .height(Length::Fill),
            ]
            .spacing(8)
            .padding(16)
            .width(300)
            .height(Length::Fill);

            row![main, panel].into()
        } else {
            main
        };

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
