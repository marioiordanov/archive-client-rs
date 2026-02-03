use iced::alignment::{Horizontal, Vertical};
use iced::widget::{button, column, container, row, scrollable, text};
use iced::widget::table;
use iced::{Alignment, Border, Element, Length, Theme};

use crate::app::message::ScreenMessage;

#[derive(Debug, Clone)]
pub enum Message {
    RefreshClicked,
    RemoveAccessClicked {
        email: String,
        folder_id: String,
        permission_id: Option<String>,
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

#[derive(Debug)]
pub struct OrgDashboardScreen {
    pub org_id: String,

    pub loading: bool,
    pub error: Option<String>,
    pub rows: Vec<DashboardRow>,
}

impl OrgDashboardScreen {
    pub fn new(org_id: String) -> Self {
        Self {
            org_id,
            loading: true,
            error: None,
            rows: Vec::new(),
        }
    }

    pub fn set_rows(&mut self, rows: Vec<DashboardRow>) {
        self.rows = rows;
        self.loading = false;
        self.error = None;
    }

    pub fn set_error(&mut self, error: String) {
        self.loading = false;
        self.error = Some(error);
    }

    pub fn set_removing(&mut self, folder_id: &str, removing: bool) {
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

        let refresh = button(if self.loading { "Loading…" } else { "Refresh" })
            .padding(12)
            .on_press(Message::RefreshClicked);

        let status_line: Element<Message> = if let Some(err) = &self.error {
            text(format!("Error: {err}")).size(12).into()
        } else if self.loading {
            text("Loading users…").size(12).into()
        } else {
            text("").into()
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
                refresh,
            ]
            .align_y(Alignment::Center)
            .spacing(12)
            .width(Length::Fill),
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
