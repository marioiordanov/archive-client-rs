use crate::messages::Message;
use iced::widget::column;
use iced::{Element, widget::button};

pub(crate) struct SignInScreen {}

impl SignInScreen {
    pub fn new() -> Self {
        Self {}
    }

    pub fn view(&self) -> Element<'_, Message> {
        column![button("Sign in").on_press(Message::SignInPressed)]
            .spacing(16)
            .into()
    }
}
