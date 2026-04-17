use iced::alignment::Horizontal;
use iced::alignment::Vertical::{self};
use iced::widget::{column, container, row, text};
use iced::{Alignment, Border, Color, Length, Padding};
use iced::{Element, widget::button};

use crate::app::message::ScreenMessage;
use crate::ui_error::UiError;

#[derive(Default, Debug)]
pub struct SignInScreen {
    busy: bool,
    pub error: Option<UiError>,
}

impl SignInScreen {
    pub fn view(&self) -> Element<'_, Message> {
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
            .style(|_theme| container::Style {
                border: Border {
                    color: Color::from_rgb(1.0, 0.0, 0.0),
                    width: 1.0,
                    radius: 4.0.into(),
                },
                background: Some(Color::from_rgb(1.0, 0.9, 0.9).into()),
                ..Default::default()
            })
            .into()
        } else {
            container(text("")).into()
        };

        let content = container(
            column![title, sign_in, error_block]
                .spacing(16)
                .align_x(Alignment::Center),
        )
        .padding(24)
        .width(Length::Shrink);

        // Use center with padding to offset upward
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding {
                top: 0.0,
                right: 0.0,
                bottom: 100.0,
                left: 0.0,
            }) // [top, right, bottom, left] - adds bottom padding to shift up
            .align_x(Horizontal::Center)
            .align_y(Vertical::Center)
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

impl From<Message> for ScreenMessage {
    fn from(val: Message) -> Self {
        ScreenMessage::Login(val)
    }
}
