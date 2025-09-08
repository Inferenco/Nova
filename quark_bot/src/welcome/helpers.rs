use crate::utils::{escape_for_markdown_v2, unescape_markdown};
use crate::welcome::dto::WelcomeSettings;

pub fn get_default_welcome_message(
    username: &str,
    group_name: &str,
    timeout_minutes: u64,
) -> String {
    format!(
        "👋 Welcome to {}, {}\\!\\n\\n🔒 Please verify you're human by clicking the button below within {} minutes\\.\\n\\n⚠️ You'll be automatically removed if you don't verify in time\\.",
        escape_for_markdown_v2(group_name),
        escape_for_markdown_v2(&format!("@{}", username)),
        timeout_minutes
    )
}

pub fn get_custom_welcome_message(
    settings: &WelcomeSettings,
    username: &str,
    group_name: &str,
) -> String {
    if let Some(ref custom_msg) = settings.custom_message {
        let mut message = custom_msg.clone();

        // First, unescape markdown characters that Telegram escaped
        message = unescape_markdown(&message);

        // Escape the entire message template for MarkdownV2
        message = escape_for_markdown_v2(&message);

        // Escape dynamic content for MarkdownV2 before replacement
        let escaped_username = escape_for_markdown_v2(&format!("@{}", username));
        let escaped_group_name = escape_for_markdown_v2(group_name);
        let timeout_minutes = (settings.verification_timeout / 60).to_string();
        let escaped_timeout = escape_for_markdown_v2(&timeout_minutes);

        // Replace placeholders with escaped content
        message = message.replace("\\{username\\}", &escaped_username);
        message = message.replace("\\{group_name\\}", &escaped_group_name);
        message = message.replace("\\{timeout\\}", &escaped_timeout);

        message
    } else {
        get_default_welcome_message(username, group_name, settings.verification_timeout / 60)
    }
}

pub fn format_timeout_display(seconds: u64) -> String {
    if seconds < 60 {
        format!("{} seconds", seconds)
    } else if seconds < 3600 {
        format!("{} minutes", seconds / 60)
    } else {
        format!("{} hours", seconds / 3600)
    }
}

pub fn is_verification_expired(timestamp: i64) -> bool {
    chrono::Utc::now().timestamp() > timestamp
}

pub fn get_verification_expiry_time(timeout_seconds: u64) -> i64 {
    chrono::Utc::now().timestamp() + timeout_seconds as i64
}
