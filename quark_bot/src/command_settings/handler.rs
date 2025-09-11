use anyhow::Result;
use teloxide::{
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, ParseMode},
};

use crate::dependencies::BotDependencies;
use crate::utils;

pub async fn handle_command_settings_callback(
    bot: Bot,
    query: teloxide::types::CallbackQuery,
    bot_deps: BotDependencies,
) -> Result<()> {
    if let Some(data) = &query.data {
        let user_id = query.from.id;

        if let Some(message) = &query.message {
            if let teloxide::types::MaybeInaccessibleMessage::Regular(m) = message {
                let is_admin = utils::is_admin(&bot, m.chat.id, user_id).await;

                if !is_admin {
                    bot.answer_callback_query(query.id)
                        .text("❌ Only administrators can manage command settings")
                        .await?;
                    return Ok(());
                }

                match data.as_str() {
                    "open_command_settings" => {
                        show_command_settings_menu(&bot, &query, &bot_deps, m.chat.id).await?;
                    }
                    "toggle_chat_commands" => {
                        toggle_chat_commands(&bot, &query, &bot_deps, m.chat.id).await?;
                    }
                    "command_settings_back" => {
                        show_group_settings_menu(&bot, &query, m.chat.id).await?;
                    }
                    _ => {
                        bot.answer_callback_query(query.id)
                            .text("Unknown command settings action")
                            .await?;
                    }
                }
            }
        }
    }

    Ok(())
}

async fn show_command_settings_menu(
    bot: &Bot,
    query: &teloxide::types::CallbackQuery,
    bot_deps: &BotDependencies,
    chat_id: teloxide::types::ChatId,
) -> Result<()> {
    let group_id = chat_id.to_string();
    let settings = bot_deps.command_settings.get_command_settings(group_id);

    let chat_status = if settings.chat_commands_enabled {
        "✅ Enabled"
    } else {
        "❌ Disabled"
    };

    let chat_action = if settings.chat_commands_enabled {
        "❌ Disable Chat Commands"
    } else {
        "✅ Enable Chat Commands"
    };

    let keyboard = InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback(
            chat_action,
            "toggle_chat_commands",
        )],
        vec![InlineKeyboardButton::callback(
            "↩️ Back to Settings",
            "command_settings_back",
        )],
    ]);

    let text = format!(
        "⚙️ <b>Command Settings</b>\n\nManage which commands are available in this group.\n\n<b>Chat Commands (/c, /chat):</b> {}\n\n💡 <i>When disabled, the /c and /chat commands will not work in this group.</i>",
        chat_status
    );
    let text = crate::utils::sanitize_telegram_html(&text);

    if let Some(teloxide::types::MaybeInaccessibleMessage::Regular(message)) = &query.message {
        bot.edit_message_text(message.chat.id, message.id, text)
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard)
            .await?;
    }

    bot.answer_callback_query(query.id.clone()).await?;
    Ok(())
}

async fn toggle_chat_commands(
    bot: &Bot,
    query: &teloxide::types::CallbackQuery,
    bot_deps: &BotDependencies,
    chat_id: teloxide::types::ChatId,
) -> Result<()> {
    let group_id = chat_id.to_string();
    let mut settings = bot_deps
        .command_settings
        .get_command_settings(group_id.clone());

    settings.chat_commands_enabled = !settings.chat_commands_enabled;
    settings.group_id = group_id.clone();

    match bot_deps
        .command_settings
        .set_command_settings(group_id, settings.clone())
    {
        Ok(_) => {
            let status_text = if settings.chat_commands_enabled {
                "✅ Chat commands have been enabled"
            } else {
                "❌ Chat commands have been disabled"
            };

            show_command_settings_menu(bot, query, bot_deps, chat_id).await?;
            bot.answer_callback_query(query.id.clone())
                .text(status_text)
                .await?;
        }
        Err(e) => {
            log::error!("Failed to update command settings: {}", e);
            bot.answer_callback_query(query.id.clone())
                .text("❌ Failed to update settings")
                .await?;
        }
    }

    Ok(())
}

async fn show_group_settings_menu(
    bot: &Bot,
    query: &teloxide::types::CallbackQuery,
    _chat_id: teloxide::types::ChatId,
) -> Result<()> {
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

    let text = crate::utils::sanitize_telegram_html("⚙️ <b>Group Settings</b>\n\n• Configure payment token, DAO preferences, moderation, sponsor settings, command settings, filters, summarization settings, and group migration.\n\n💡 Only group administrators can access these settings.");

    if let Some(teloxide::types::MaybeInaccessibleMessage::Regular(message)) = &query.message {
        bot.edit_message_text(message.chat.id, message.id, text)
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard)
            .await?;
    }

    bot.answer_callback_query(query.id.clone()).await?;
    Ok(())
}
