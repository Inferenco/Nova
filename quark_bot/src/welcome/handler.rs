use anyhow::Result;
use teloxide::{
    prelude::*,
    types::{CallbackQuery, InlineKeyboardButton, InlineKeyboardMarkup, Message, ParseMode},
    utils::render::RenderMessageTextHelper,
};

use crate::{
    dependencies::BotDependencies,
    utils::{self, send_html_message, send_message},
    welcome::{helpers::format_timeout_display, welcome_service::WelcomeService},
};

pub async fn handle_welcome_settings_callback(
    bot: Bot,
    query: CallbackQuery,
    bot_deps: BotDependencies,
) -> Result<()> {
    let data = query.data.as_ref().unwrap();
    let msg = match &query.message {
        Some(teloxide::types::MaybeInaccessibleMessage::Regular(message)) => message,
        _ => return Ok(()),
    };

    let admins = bot.get_chat_administrators(msg.chat.id).await?;
    let is_admin = admins
        .iter()
        .any(|admin| admin.user.id.to_string() == query.from.id.to_string());

    if !is_admin {
        bot.answer_callback_query(query.id)
            .text("❌ Only group admins can manage welcome settings.")
            .await?;
        return Ok(());
    }

    let welcome_service = bot_deps.welcome_service.clone();

    match data.as_str() {
        "welcome_settings" => {
            show_welcome_settings_menu(bot.clone(), msg, welcome_service).await?;
        }
        "welcome_toggle" => {
            toggle_welcome_feature(bot.clone(), msg, welcome_service).await?;
        }
        "welcome_custom_message" => {
            show_custom_message_menu(bot.clone(), msg, welcome_service).await?;
        }
        "welcome_timeout" => {
            show_timeout_menu(bot.clone(), msg, welcome_service).await?;
        }
        "welcome_stats" => {
            show_welcome_stats(bot.clone(), msg, welcome_service).await?;
        }
        "welcome_reset_stats" => {
            reset_welcome_stats(bot.clone(), msg, welcome_service).await?;
        }
        "welcome_reset_message" => {
            reset_custom_message(bot.clone(), msg, welcome_service).await?;
        }
        "welcome_set_custom_message" => {
            start_custom_message_input(bot.clone(), msg, welcome_service).await?;
        }
        _ if data.starts_with("welcome_timeout_set_") => {
            let timeout = data.strip_prefix("welcome_timeout_set_").unwrap();
            if let Ok(timeout_seconds) = timeout.parse::<u64>() {
                set_welcome_timeout(bot.clone(), msg, welcome_service, timeout_seconds).await?;
            }
        }
        _ if data.starts_with("welcome_back_to_") => {
            let target = data.strip_prefix("welcome_back_to_").unwrap();
            match target {
                "main" => show_welcome_settings_menu(bot.clone(), msg, welcome_service).await?,
                "groupsettings" => show_main_group_settings(bot.clone(), msg).await?,
                _ => {}
            }
        }
        _ => {}
    }

    // Answer callback query for all welcome callbacks to prevent retries
    if data.starts_with("welcome_") {
        bot.answer_callback_query(query.id).await?;
    }
    Ok(())
}

async fn show_welcome_settings_menu(
    bot: Bot,
    msg: &Message,
    welcome_service: WelcomeService,
) -> Result<()> {
    let settings = welcome_service.get_settings(msg.chat.id);
    let stats = welcome_service.get_stats(msg.chat.id);

    let status_text = if settings.enabled {
        "🟢 Enabled"
    } else {
        "🔴 Disabled"
    };
    let timeout_text = format_timeout_display(settings.verification_timeout);

    let text = format!(
        "👋 <b>Welcome Settings</b>\n\n\
        📊 Status: {}\n\
        ⏰ Verification Timeout: {}\n\
        📈 Success Rate: {:.1}%\n\
        ✅ Total Verifications: {}\n\
        ❌ Failed Verifications: {}\n\n\
        Configure anti-spam protection for new group members.",
        status_text,
        timeout_text,
        stats.success_rate,
        stats.total_verifications,
        stats.failed_verifications
    );

    let keyboard = InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback(
            if settings.enabled {
                "🔴 Disable Welcome"
            } else {
                "🟢 Enable Welcome"
            },
            "welcome_toggle",
        )],
        vec![InlineKeyboardButton::callback(
            "✏️ Custom Message",
            "welcome_custom_message",
        )],
        vec![InlineKeyboardButton::callback(
            "⏰ Set Timeout",
            "welcome_timeout",
        )],
        vec![InlineKeyboardButton::callback(
            "📊 View Statistics",
            "welcome_stats",
        )],
        vec![InlineKeyboardButton::callback(
            "🔄 Reset Statistics",
            "welcome_reset_stats",
        )],
        vec![InlineKeyboardButton::callback(
            "↩️ Back to Group Settings",
            "welcome_back_to_groupsettings",
        )],
    ]);

    let text = crate::utils::sanitize_telegram_html(&text);
    match bot
        .edit_message_text(msg.chat.id, msg.id, text)
        .parse_mode(ParseMode::Html)
        .reply_markup(keyboard)
        .await
    {
        Ok(_) => log::info!("Welcome settings menu updated successfully"),
        Err(e) => {
            if e.to_string().contains("message is not modified") {
                log::info!("Welcome settings menu unchanged, skipping update");
            } else {
                return Err(anyhow::anyhow!("Failed to edit message: {}", e));
            }
        }
    }

    Ok(())
}

async fn toggle_welcome_feature(
    bot: Bot,
    msg: &Message,
    welcome_service: WelcomeService,
) -> Result<()> {
    let mut settings = welcome_service.get_settings(msg.chat.id);
    settings.enabled = !settings.enabled;
    settings.last_updated = chrono::Utc::now().timestamp();

    welcome_service.save_settings(msg.chat.id, settings.clone())?;

    // Always refresh the menu to show the new state
    show_welcome_settings_menu(bot, msg, welcome_service).await?;

    Ok(())
}

async fn show_custom_message_menu(
    bot: Bot,
    msg: &Message,
    welcome_service: WelcomeService,
) -> Result<()> {
    let settings = welcome_service.get_settings(msg.chat.id);
    let current_message = if let Some(ref msg) = settings.custom_message {
        msg
    } else {
        "Use default welcome message"
    };

    let text = format!(
        "✏️ <b>Custom Welcome Message</b>\n\n\
            Current message:\n\
            <code>{}</code>\n\n\
            💡 <i>You can use Markdown formatting (bold, code, etc.) or just plain text. Both work perfectly!</i>\n\n\
            Available placeholders:\n\
            • {{username}} - @username (creates clickable mention)\n\
            • {{group_name}} - Group name\n\
            • {{timeout}} - Verification timeout in minutes\n\n\
            <b>Examples:</b>\n\
            • <code>Hello {{username}}! Welcome to {{group_name}}! 👋</code>\n\
            • <code>**Bold welcome** to {{group_name}}, {{username}}!</code>\n\
            • <code>Use `code` for inline formatting</code>\n\n\
            To set a custom message, reply to this message with your text.\n\
            To use the default message, click 'Reset to Default'.",
        teloxide::utils::html::escape(current_message)
    );

    let keyboard = InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback(
            "✏️ Set Custom Message",
            "welcome_set_custom_message",
        )],
        vec![InlineKeyboardButton::callback(
            "🔄 Reset to Default",
            "welcome_reset_message",
        )],
        vec![InlineKeyboardButton::callback(
            "↩️ Back",
            "welcome_back_to_main",
        )],
    ]);

    let text = crate::utils::sanitize_telegram_html(&text);
    bot.edit_message_text(msg.chat.id, msg.id, text)
        .parse_mode(ParseMode::Html)
        .reply_markup(keyboard)
        .await?;

    Ok(())
}

async fn show_timeout_menu(bot: Bot, msg: &Message, welcome_service: WelcomeService) -> Result<()> {
    let settings = welcome_service.get_settings(msg.chat.id);
    let current_timeout = settings.verification_timeout;

    let text = format!(
        "⏰ <b>Verification Timeout</b>\n\n\
        Current timeout: {}\n\n\
        Select a new timeout value:",
        format_timeout_display(current_timeout)
    );

    let keyboard = InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback("30s", "welcome_timeout_set_30"),
            InlineKeyboardButton::callback("1m", "welcome_timeout_set_60"),
            InlineKeyboardButton::callback("2m", "welcome_timeout_set_120"),
        ],
        vec![
            InlineKeyboardButton::callback("3m", "welcome_timeout_set_180"),
            InlineKeyboardButton::callback("4m", "welcome_timeout_set_240"),
            InlineKeyboardButton::callback("5m", "welcome_timeout_set_300"),
        ],
        vec![InlineKeyboardButton::callback(
            "↩️ Back",
            "welcome_back_to_main",
        )],
    ]);

    let text = crate::utils::sanitize_telegram_html(&text);
    bot.edit_message_text(msg.chat.id, msg.id, text)
        .parse_mode(ParseMode::Html)
        .reply_markup(keyboard)
        .await?;

    Ok(())
}

async fn set_welcome_timeout(
    bot: Bot,
    msg: &Message,
    welcome_service: WelcomeService,
    timeout_seconds: u64,
) -> Result<()> {
    let mut settings = welcome_service.get_settings(msg.chat.id);
    settings.verification_timeout = timeout_seconds;
    settings.last_updated = chrono::Utc::now().timestamp();

    welcome_service.save_settings(msg.chat.id, settings)?;

    // Refresh the menu
    show_welcome_settings_menu(bot, msg, welcome_service).await?;

    Ok(())
}

async fn show_welcome_stats(
    bot: Bot,
    msg: &Message,
    welcome_service: WelcomeService,
) -> Result<()> {
    let stats = welcome_service.get_stats(msg.chat.id);
    let settings = welcome_service.get_settings(msg.chat.id);

    let last_verification = if let Some(timestamp) = stats.last_verification {
        let dt = chrono::DateTime::from_timestamp(timestamp, 0).unwrap_or_default();
        dt.format("%Y-%m-%d %H:%M:%S UTC").to_string()
    } else {
        "Never".to_string()
    };

    let text = format!(
        "📊 <b>Welcome Statistics</b>\n\n\
        📈 Total Verifications: {}\n\
        ✅ Successful: {}\n\
        ❌ Failed: {}\n\
        📊 Success Rate: {:.1}%\n\
        🕐 Last Verification: {}\n\
        ⏰ Current Timeout: {}\n\
        🕐 Last Updated: {}\n\n\
        These statistics help you monitor the effectiveness of your anti-spam protection.",
        stats.total_verifications,
        stats.successful_verifications,
        stats.failed_verifications,
        stats.success_rate,
        last_verification,
        format_timeout_display(settings.verification_timeout),
        chrono::DateTime::from_timestamp(settings.last_updated, 0)
            .unwrap_or_default()
            .format("%Y-%m-%d %H:%M:%S UTC")
    );

    let keyboard = InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback(
            "🔄 Reset Statistics",
            "welcome_reset_stats",
        )],
        vec![InlineKeyboardButton::callback(
            "↩️ Back",
            "welcome_back_to_main",
        )],
    ]);

    let text = crate::utils::sanitize_telegram_html(&text);
    bot.edit_message_text(msg.chat.id, msg.id, text)
        .parse_mode(ParseMode::Html)
        .reply_markup(keyboard)
        .await?;

    Ok(())
}

async fn reset_welcome_stats(
    bot: Bot,
    msg: &Message,
    welcome_service: WelcomeService,
) -> Result<()> {
    // Reset stats by clearing the stats tree for this chat
    welcome_service.reset_stats(msg.chat.id)?;

    // Refresh the stats view
    show_welcome_stats(bot, msg, welcome_service).await?;

    Ok(())
}

async fn reset_custom_message(
    bot: Bot,
    msg: &Message,
    welcome_service: WelcomeService,
) -> Result<()> {
    let mut settings = welcome_service.get_settings(msg.chat.id);
    settings.custom_message = None;
    settings.last_updated = chrono::Utc::now().timestamp();

    welcome_service.save_settings(msg.chat.id, settings)?;

    // Refresh the custom message menu
    show_custom_message_menu(bot, msg, welcome_service).await?;

    Ok(())
}

async fn start_custom_message_input(
    bot: Bot,
    msg: &Message,
    welcome_service: WelcomeService,
) -> Result<()> {
    let text = "✏️ <b>Custom Welcome Message</b>\n\n\
        Please reply to this message with your custom welcome message.\n\n\
        💡 <i>You can use Markdown formatting (bold, code, etc.) or just plain text. Both work perfectly!</i>\n\n\
        Available placeholders:\n\
        • {username} - @username (creates clickable mention)\n\
        • {group_name} - Group name\n\
        • {timeout} - Verification timeout in minutes\n\n\
        <b>Examples:</b>\n\
        • <code>Hello {username}! Welcome to {group_name}! 👋</code>\n\
        • <code>**Bold welcome** to {group_name}, {username}!</code>\n\
        • <code>Use `code` for inline formatting</code>\n\n\
        <i>Send /cancel to cancel or just send your message.</i>";

    let keyboard = InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
        "↩️ Back",
        "welcome_back_to_main",
    )]]);

    let text = crate::utils::sanitize_telegram_html(text);
    match bot
        .edit_message_text(msg.chat.id, msg.id, text)
        .parse_mode(ParseMode::Html)
        .reply_markup(keyboard)
        .await
    {
        Ok(_) => log::info!("Successfully updated custom message input screen"),
        Err(e) => log::error!("Failed to update custom message input screen: {}", e),
    }

    // Store the state that we're waiting for custom message input
    match welcome_service.store_input_state(msg.chat.id).await {
        Ok(_) => log::info!("Successfully stored welcome input state"),
        Err(e) => log::error!("Failed to store welcome input state: {}", e),
    }

    Ok(())
}

async fn show_main_group_settings(bot: Bot, msg: &Message) -> Result<()> {
    let keyboard = InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback(
            "💳 Payment Settings",
            "open_group_payment_settings",
        )],
        vec![InlineKeyboardButton::callback(
            "🏛️ DAO Preferences",
            "open_dao_preferences",
        )],
        vec![InlineKeyboardButton::callback(
            "🛡️ Moderation",
            "open_moderation_settings",
        )],
        vec![InlineKeyboardButton::callback(
            "🎯 Sponsor Settings",
            "open_sponsor_settings",
        )],
        vec![InlineKeyboardButton::callback(
            "👋 Welcome Settings",
            "welcome_settings",
        )],
        vec![InlineKeyboardButton::callback("🔍 Filters", "filters_main")],
        vec![InlineKeyboardButton::callback(
            "📁 Group Document Library",
            "open_group_document_library",
        )],
        vec![InlineKeyboardButton::callback(
            "⚙️ Command Settings",
            "open_command_settings",
        )],
        vec![InlineKeyboardButton::callback(
            "📋 Summarization Settings",
            "open_group_summarization_settings",
        )],
        vec![InlineKeyboardButton::callback(
            "🔄 Migrate Group ID",
            "open_migrate_group_id",
        )],
        vec![InlineKeyboardButton::callback(
            "↩️ Close",
            "group_settings_close",
        )],
    ]);

    let text = crate::utils::sanitize_telegram_html(
        "⚙️ <b>Group Settings</b>\n\n• Configure payment token, DAO preferences, moderation, sponsor settings, welcome protection, command settings, filters, summarization settings, and group migration.\n\n💡 Only group administrators can access these settings.",
    );
    bot.edit_message_text(
        msg.chat.id,
        msg.id,
        text,
    )
    .parse_mode(ParseMode::Html)
    .reply_markup(keyboard)
    .await?;

    Ok(())
}

pub async fn handle_welcome_message(
    bot: Bot,
    bot_deps: BotDependencies,
    msg: &Message,
    user_id: String,
    group_id: String,
) -> Result<bool> {
    let group_id = group_id.parse::<i64>();

    if group_id.is_err() {
        log::error!("Invalid group ID: {}", group_id.err().unwrap());
        return Err(anyhow::anyhow!("Invalid group ID"));
    }

    let group_id = ChatId(group_id.unwrap());

    let user_id = user_id.parse::<u64>();

    if user_id.is_err() {
        log::error!("Invalid user ID: {}", user_id.err().unwrap());
        return Ok(false);
    }

    let user_id = UserId(user_id.unwrap());

    if let Some(_input_state) = bot_deps.welcome_service.get_input_state(group_id) {
        log::info!("Found welcome input state for group: {}", group_id);
        // Only process if the user is an admin
        let is_admin = utils::is_admin(&bot, group_id, user_id).await;
        if !is_admin {
            // Non-admin users typing during welcome setup - ignore silently
            return Ok(false);
        }

        if let Some(text) = msg.text() {
            let text = text.trim();
            if !text.is_empty() {
                if text == "/cancel" {
                    // Cancel the custom message input
                    bot_deps.welcome_service.clear_input_state(group_id)?;
                    send_message(
                        msg.clone(),
                        bot,
                        "❌ Custom message input cancelled.".to_string(),
                    )
                    .await?;
                    return Ok(true);
                }

                // Try to preserve markdown formatting, with fallback to plain text
                let message_text = msg
                    .markdown_text()
                    .map(|s| s.to_string())
                    .or_else(|| msg.markdown_caption().map(|s| s.to_string()))
                    .or_else(|| msg.text().map(|s| s.to_string()))
                    .or_else(|| msg.caption().map(|s| s.to_string()))
                    .unwrap_or_default()
                    .trim()
                    .to_string();

                // Update the welcome settings with custom message
                let mut settings = bot_deps.welcome_service.get_settings(msg.chat.id);
                settings.custom_message = Some(message_text);
                settings.last_updated = chrono::Utc::now().timestamp();

                if let Err(e) = bot_deps
                    .welcome_service
                    .save_settings(msg.chat.id, settings)
                {
                    send_message(
                        msg.clone(),
                        bot,
                        format!("❌ Failed to save custom message: {}", e),
                    )
                    .await?;
                    return Ok(true);
                }

                // Clear the input state
                bot_deps.welcome_service.clear_input_state(group_id)?;

                // Send success message
                send_html_message(
                    msg.clone(),
                    bot,
                    "✅ <b>Custom welcome message updated successfully!</b>\n\n\
                    New members will now see your custom message with placeholders replaced and markdown formatting preserved!".to_string(),
                )
                .await?;

                return Ok(true);
            } else {
                // Empty text, ask for valid input
                send_message(
                    msg.clone(),
                    bot,
                    "❌ Please enter a valid welcome message. Use /cancel to cancel.".to_string(),
                )
                .await?;
                return Ok(true);
            }
        }
    }

    Ok(false)
}
