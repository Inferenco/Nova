use super::SummarizationSettings;
use super::helpers::{build_summarization_keyboard_with_context, format_summarization_status};
use anyhow::Result;
use teloxide::prelude::*;
use teloxide::types::ParseMode;

pub async fn handle_summarization_settings_callback(
    bot: Bot,
    query: CallbackQuery,
    summarization_settings: SummarizationSettings,
) -> Result<()> {
    let data = query.data.as_ref().unwrap();
    let user_id = query.from.id.0 as i64;
    let user_id_str = user_id.to_string();

    // Determine if this is a group chat
    let group_id = if let Some(message) = &query.message {
        if let teloxide::types::MaybeInaccessibleMessage::Regular(msg) = message {
            if msg.chat.is_group() || msg.chat.is_supergroup() {
                Some(msg.chat.id.to_string())
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    if data == "open_summarization_settings" || data == "open_group_summarization_settings" {
        show_summarization_settings_menu(bot, query, summarization_settings).await?;
    } else if data.starts_with("toggle_summarizer:") {
        let enabled = data == "toggle_summarizer:on";
        if let Err(e) = summarization_settings.set_enabled(&user_id_str, group_id.clone(), enabled)
        {
            log::error!(
                "Failed to set summarizer enabled for user {}: {}",
                user_id,
                e
            );
        }
        show_summarization_settings_menu(bot, query, summarization_settings).await?;
    } else if data.starts_with("set_summarizer_threshold:") {
        if let Some(threshold_str) = data.strip_prefix("set_summarizer_threshold:") {
            if let Ok(threshold) = threshold_str.parse::<u32>() {
                // Validate against allowed presets
                if [16000, 18000, 20000, 24000, 26000].contains(&threshold) {
                    if let Err(e) = summarization_settings.set_token_limit(
                        &user_id_str,
                        group_id.clone(),
                        threshold,
                    ) {
                        log::error!("Failed to set token limit for user {}: {}", user_id, e);
                    }
                }
            }
        }
        show_summarization_settings_menu(bot, query, summarization_settings).await?;
    } else if data == "summarization_back_to_usersettings" {
        show_user_settings_menu(bot, query).await?;
    } else if data == "summarization_back_to_groupsettings" {
        show_group_settings_menu(bot, query).await?;
    }

    Ok(())
}

async fn show_summarization_settings_menu(
    bot: Bot,
    query: CallbackQuery,
    summarization_settings: SummarizationSettings,
) -> Result<()> {
    let user_id = query.from.id.0 as i64;
    let user_id_str = user_id.to_string();

    // Determine if this is a group chat
    let group_id = if let Some(message) = &query.message {
        if let teloxide::types::MaybeInaccessibleMessage::Regular(msg) = message {
            if msg.chat.is_group() || msg.chat.is_supergroup() {
                Some(msg.chat.id.to_string())
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let is_group_context = group_id.is_some();
    let effective_prefs = summarization_settings.get_effective_prefs(&user_id_str, group_id);

    let status_text = format_summarization_status(&effective_prefs);
    let keyboard = build_summarization_keyboard_with_context(&effective_prefs, is_group_context);

    if let Some(teloxide::types::MaybeInaccessibleMessage::Regular(message)) = &query.message {
        bot.edit_message_text(message.chat.id, message.id, status_text)
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard)
            .await?;
    }

    bot.answer_callback_query(query.id)
        .text("Settings updated!")
        .await?;

    Ok(())
}

async fn show_user_settings_menu(bot: Bot, query: CallbackQuery) -> Result<()> {
    use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

    if let Some(teloxide::types::MaybeInaccessibleMessage::Regular(message)) = &query.message {
        let kb = InlineKeyboardMarkup::new(vec![
            vec![InlineKeyboardButton::callback(
                "🧠 Select Model",
                "open_select_model",
            )],
            vec![InlineKeyboardButton::callback(
                "💳 Payment Settings",
                "open_payment_settings",
            )],
            vec![InlineKeyboardButton::callback(
                "📁 Document Library",
                "open_document_library",
            )],
            vec![InlineKeyboardButton::callback(
                "📋 View My Settings",
                "open_my_settings",
            )],
            vec![InlineKeyboardButton::callback(
                "🧾 Summarization Settings",
                "open_summarization_settings",
            )],
            vec![InlineKeyboardButton::callback(
                "↩️ Close",
                "user_settings_close",
            )],
        ]);

        let text = "⚙️ <b>User Settings</b>\n\n• Manage your model, view current settings, and configure payment.\n\n💡 If no payment token is selected, the on-chain default will be used.";
        bot.edit_message_text(message.chat.id, message.id, text)
        .parse_mode(ParseMode::Html)
        .reply_markup(kb)
        .await?;
    }

    bot.answer_callback_query(query.id).await?;
    Ok(())
}

async fn show_group_settings_menu(bot: Bot, query: CallbackQuery) -> Result<()> {
    use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

    if let Some(teloxide::types::MaybeInaccessibleMessage::Regular(message)) = &query.message {
        let kb = InlineKeyboardMarkup::new(vec![
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

        let text = "⚙️ <b>Group Settings</b>\n\n• Configure payment token, DAO preferences, moderation, sponsor settings, command settings, filters, summarization settings, and group migration.\n\n💡 Only group administrators can access these settings.";
        bot.edit_message_text(message.chat.id, message.id, text)
        .parse_mode(ParseMode::Html)
        .reply_markup(kb)
        .await?;
    }

    bot.answer_callback_query(query.id).await?;
    Ok(())
}
