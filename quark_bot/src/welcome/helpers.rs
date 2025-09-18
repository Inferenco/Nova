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
        let mut message = unescape_markdown(custom_msg);

        // Remove inline code wrappers around placeholders so replacements render correctly
        for placeholder in ["{username}", "{group_name}", "{timeout}"] {
            let code_wrapped = format!("`{}`", placeholder);
            if message.contains(&code_wrapped) {
                message = message.replace(&code_wrapped, placeholder);
            }
        }

        // username_markup is already MarkdownV2 link entity like
        // [@username](tg://user?id=123) or [First Name](tg://user?id=123)
        let escaped_group_name = escape_for_markdown_v2(group_name);
        let timeout_minutes = (settings.verification_timeout / 60).to_string();
        let escaped_timeout = escape_for_markdown_v2(&timeout_minutes);

        message = message.replace("{username}", username_markup);
        message = message.replace("{group_name}", &escaped_group_name);
        message = message.replace("{timeout}", &escaped_timeout);

        message
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::welcome::dto::WelcomeSettings;

    fn build_settings(message: &str, timeout_seconds: u64) -> WelcomeSettings {
        WelcomeSettings {
            enabled: true,
            custom_message: Some(message.to_string()),
            verification_timeout: timeout_seconds,
            ..Default::default()
        }
    }

    #[test]
    fn custom_message_preserves_markdown_formatting() {
        let template = "Welcome to *{group_name}*, {username}!\n\n1. Connect\n• Enjoy [demo](https://example.com)";
        let settings = build_settings(template, 600);

        let result = get_custom_welcome_message(
            &settings,
            "[@nova](tg://user?id=42)",
            "Inferenco Inner Circle",
        );

        let expected = "Welcome to *Inferenco Inner Circle*, [@nova](tg://user?id=42)!\n\n1. Connect\n• Enjoy [demo](https://example.com)";
        assert_eq!(result, expected);
    }

    #[test]
    fn custom_message_escapes_dynamic_data_only() {
        let template = "Hi {username}! Welcome to {group_name}. Timeout: {timeout}";
        let settings = build_settings(template, 180);

        let result = get_custom_welcome_message(
            &settings,
            "[@nova](tg://user?id=42)",
            "Group (v2)!",
        );

        let expected = r"Hi [@nova](tg://user?id=42)! Welcome to Group \(v2\)\!. Timeout: 3";
        assert_eq!(result, expected);
    }

    #[test]
    fn custom_message_handles_admin_template_with_escaped_chars() {
        let template = "Welcome to *\\{group_name\\}*, `\\{username\\}`\\!\n\n*Nova — Your smart community manager and personal assistant on Telegram*\nTransparent platform with pay-per-use pricing and reliable service\\.\n\n*How Nova works*\n1\\. Connect with Nova — start a Telegram chat and fund your account with Aptos tokens\\.\n\n2\\. Use AI & community tools — inference, content generation, moderation and automation\\.\n\n3\\. Transparent billing — costs recorded on-chain so you pay only for what you consume\\.\n\n*Key features*\n• 🤖 Automatic bot answers & 🎁 sponsorships — instant help and sponsored access to AI tools\\.\n\nWatch a short demo here: [Nova demo](https://youtu.be/ta0Hx42MHas?si=0rk6l2g2HWS5k2TQ)";
        let settings = build_settings(template, 600);

        let result = get_custom_welcome_message(
            &settings,
            "[@username](tg://user?id=123)",
            "Inferenco Inner Circle",
        );

        assert!(result.contains(r"\!"));
        assert!(result.contains(r"\."));
        assert!(result.contains("[@username](tg://user?id=123)"));
        assert!(!result.contains('`'));
    }

    #[test]
    fn custom_message_unwraps_inline_code_placeholders() {
        let template = "`{username}` joins `{group_name}` in `{timeout}` minutes";
        let settings = build_settings(template, 600);

        let result = get_custom_welcome_message(
            &settings,
            "[@nova](tg://user?id=42)",
            "Awesome Group",
        );

        assert_eq!(
            result,
            "[@nova](tg://user?id=42) joins Awesome Group in 10 minutes"
        );
    }
}
