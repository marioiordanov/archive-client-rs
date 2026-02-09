use iced::alignment::Horizontal;
use iced::alignment::Vertical;
use iced::widget::{button, column, container, scrollable, text};
use iced::{Alignment, Element, Length};

use crate::app::{message::ScreenMessage, state::OrgInvitation};

#[derive(Default, Debug)]
pub struct OrgSelectionScreen {
    pub invitations: Vec<OrgInvitation>,
    pub loading: bool,
}

impl OrgSelectionScreen {
    pub fn new() -> Self {
        Self {
            invitations: Vec::new(),
            loading: true,
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let title = text("Organization Setup").size(32);

        let mut content_column = column![title].spacing(20).align_x(Alignment::Center);

        if self.loading {
            content_column = content_column.push(text("Loading invitations...").size(16));
        } else if !self.invitations.is_empty() {
            content_column = content_column.push(text("You have been invited to:").size(18));

            let invitation_list =
                self.invitations
                    .iter()
                    .fold(column![].spacing(10), |col, invitation| {
                        col.push(
                            button(
                                column![
                                    text(&invitation.org_name).size(16),
                                    text(format!("Invited by: {}", invitation.invited_by)).size(12),
                                ]
                                .spacing(4),
                            )
                            .padding(12)
                            .width(Length::Fill)
                            .on_press(Message::JoinOrgClicked {
                                org_id: invitation.org_id.clone(),
                                org_name: invitation.org_name.clone(),
                            }),
                        )
                    });

            content_column = content_column
                .push(
                    container(scrollable(invitation_list))
                        .width(Length::Fill)
                        .max_height(300),
                )
                .push(text("— or —").size(14));
        }

        let create_org_button = button("Create New Organization")
            .padding(16)
            .on_press(Message::CreateOrgClicked);

        content_column = content_column.push(create_org_button);

        let content = container(content_column)
            .padding(24)
            .width(Length::Shrink)
            .max_width(500);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(iced::Padding {
                top: 0.0,
                right: 0.0,
                bottom: 80.0,
                left: 0.0,
            })
            .align_x(Horizontal::Center)
            .align_y(Vertical::Center)
            .into()
    }

    pub fn update(&mut self, msg: Message) {
        match msg {
            Message::CreateOrgClicked => {
                self.invitations = vec![];
                self.loading = true;
            }
            Message::JoinOrgClicked { .. } => {
                self.loading = true;
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum Message {
    CreateOrgClicked,
    JoinOrgClicked { org_id: String, org_name: String },
}

impl Into<ScreenMessage> for Message {
    fn into(self) -> ScreenMessage {
        ScreenMessage::OrgSelection(self)
    }
}
