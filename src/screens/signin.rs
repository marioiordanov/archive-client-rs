use iced::widget::{column, container, row, text};
use iced::{Alignment, Length, Task};
use iced::{Element, widget::button};

use crate::app::message::ScreenMessage;
use crate::ui_error::UiError;

#[derive(Default, Debug)]
pub struct SignInScreen {
    busy: bool,
    pub error: Option<UiError>,
}

impl SignInScreen {
    pub fn view(&self) -> Element<Message> {
        let title = text("Sign in").size(32);

        let sign_in = {
            let mut b = button(if self.busy {
                "Opening browser…"
            } else {
                "Sign in with Google"
            });

            if !self.busy {
                b = b.on_press(Message::SignInClicked);
            }

            b
        };

        let error_block: Element<Message> = if let Some(err) = &self.error {
            let dismiss = button("Dismiss").on_press(Message::ClearError);

            container(
                column![
                    text(&err.title).size(18),
                    if let Some(detail) = &err.detail {
                        text(detail)
                    } else {
                        text("")
                    },
                    row![dismiss].spacing(10),
                ]
                .spacing(8),
            )
            .padding(12)
            .width(Length::Fill)
            .into()
        } else {
            container(text("")).into()
        };

        container(
            column![title, sign_in, error_block]
                .spacing(16)
                .align_x(Alignment::Center),
        )
        .padding(24)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    pub fn update(&mut self, msg: Message) {
        match msg {
            Message::SignInClicked => {
                self.error = None;
                self.busy = true;
            }
            Message::ClearError => {
                self.error = None;
                self.busy = false;
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum Message {
    SignInClicked,
    ClearError,
}

impl Into<ScreenMessage> for Message {
    fn into(self) -> ScreenMessage {
        ScreenMessage::Login(self)
    }
}
