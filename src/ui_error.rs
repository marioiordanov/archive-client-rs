#[derive(Debug, Clone)]
pub struct UiError {
    pub title: String,
    pub detail: Option<String>,
    pub kind: UiErrorKind,
}

#[derive(Debug, Clone, Copy)]
pub enum UiErrorKind {
    Info,
    Warning,
    Error,
}
