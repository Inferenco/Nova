use crate::utils::{escape_for_markdown_v2, unescape_markdown};
use crate::welcome::dto::WelcomeSettings;

pub fn get_default_welcome_message(
    username_markup: &str,
    group_name: &str,
    timeout_minutes: u64,
) -> String {
    let escaped_group_name = escape_for_markdown_v2(group_name);
    let escaped_timeout = escape_for_markdown_v2(&timeout_minutes.to_string());

    format!(
        "👋 Welcome to {}, {}\\!\n\n🔒 Please verify you're human by clicking the button below within {} minutes\\.\n\n⚠️ You'll be automatically removed if you don't verify in time\\.",
        escaped_group_name, username_markup, escaped_timeout
    )
}

pub fn get_custom_welcome_message(
    settings: &WelcomeSettings,
    username_markup: &str,
    group_name: &str,
) -> String {
    if let Some(ref custom_msg) = settings.custom_message {
        let mut message = custom_msg.clone();

        // First, unescape markdown characters that Telegram escaped
        message = unescape_markdown(&message);

        // Prepare dynamic, safely escaped replacements
        // username_markup is already MarkdownV2 link entity like
        // [@username](tg://user?id=123) or [First Name](tg://user?id=123)
        let escaped_group_name = escape_for_markdown_v2(group_name);
        let timeout_minutes = (settings.verification_timeout / 60).to_string();
        let escaped_timeout = escape_for_markdown_v2(&timeout_minutes);

        // Use letter-only marker tokens that survive escaping untouched
        let username_token = "QKUSERNAME9F2D";
        let group_token = "QKGROUP9F2D";
        let timeout_token = "QKTIMEOUT9F2D";

        // Replace placeholders with tokens
        message = message.replace("{username}", username_token);
        message = message.replace("{group_name}", group_token);
        message = message.replace("{timeout}", timeout_token);

        // Escape the full message content safely for MarkdownV2
        let mut escaped_message = escape_for_markdown_v2(&message);

        // Swap tokens back to the desired values
        escaped_message = escaped_message.replace(username_token, username_markup);
        escaped_message = escaped_message.replace(group_token, &escaped_group_name);
        escaped_message = escaped_message.replace(timeout_token, &escaped_timeout);

        escaped_message
    } else {
        get_default_welcome_message(
            username_markup,
            group_name,
            settings.verification_timeout / 60,
        )
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
