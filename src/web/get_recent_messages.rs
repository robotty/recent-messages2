use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct GetRecentMessagesQueryOptions {
    // aliases are used to keep compatibility with the API from version 1.
    #[serde(alias = "hideModerationMessages")]
    pub hide_moderation_messages: bool,
    #[serde(alias = "hideModeratedMessages")]
    pub hide_moderated_messages: bool,
    #[serde(alias = "clearchatToNotice")]
    pub clearchat_to_notice: bool,
    pub limit: Option<usize>,
}

impl Default for GetRecentMessagesQueryOptions {
    fn default() -> Self {
        GetRecentMessagesQueryOptions {
            hide_moderation_messages: false,
            hide_moderated_messages: false,
            clearchat_to_notice: false,
            limit: None,
        }
    }
}

#[derive(Debug, Serialize)]
struct GetRecentMessagesResponse {
    messages: Vec<String>,
    error: Option<&'static str>,
    error_code: Option<&'static str>,
}
