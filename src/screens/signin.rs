use iced::Task;
use iced::widget::column;
use iced::{Element, widget::button};

use crate::app::message::ScreenMessage;
use crate::ui_error::UiError;

#[derive(Default, Debug)]
pub(crate) struct SignInScreen {
    busy: bool,
    error: Option<UiError>,
}

impl SignInScreen {
    pub fn view(&self) -> Element<Message> {
        column![button("Sign in").on_press(Message::SignInClicked)]
            .spacing(16)
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
