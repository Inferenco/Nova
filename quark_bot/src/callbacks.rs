//! Callback query handlers for quark_bot.

use crate::ai::group_vector_store::{
    delete_file_from_group_vector_store, delete_group_vector_store, list_group_files_with_names,
};
use crate::ai::moderation::dto::{ModerationSettings, ModerationState};
use crate::ai::vector_store::{
    delete_file_from_vector_store, delete_vector_store, list_user_files_with_names,
};
use crate::dao::handler::{handle_dao_preference_callback, handle_disable_notifications_callback};
use crate::dependencies::BotDependencies;
use crate::filters::handler::handle_filters_callback;
use crate::scheduled_payments::callbacks::handle_scheduled_payments_callback;
use crate::scheduled_prompts::callbacks::handle_scheduled_prompts_callback;
use crate::sponsor::handler::handle_sponsor_settings_callback;
use crate::user_model_preferences::callbacks::handle_model_preferences_callback;
use crate::utils::{self, send_html_message};
use crate::welcome::handler::handle_welcome_settings_callback;
use anyhow::Result;
use teloxide::sugar::request::RequestReplyExt;
use teloxide::{
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, MaybeInaccessibleMessage, ParseMode},
};

pub async fn handle_callback_query(
    bot: Bot,
    query: teloxide::types::CallbackQuery,
    bot_deps: BotDependencies,
) -> Result<()> {
    log::info!(
        "Received callback query from user {}: {:?}",
        query.from.id.0,
        query.data
    );

    if let Some(data) = &query.data {
        let user_id = query.from.id.0 as i64;

        if data.starts_with("delete_file:") {
            let file_id = data.strip_prefix("delete_file:").unwrap();

            if let Some(vector_store_id) = bot_deps.user_convos.get_vector_store_id(user_id) {
                match delete_file_from_vector_store(
                    user_id,
                    bot_deps.clone(),
                    &vector_store_id,
                    file_id,
                )
                .await
                {
                    Ok(_) => {
                        bot.answer_callback_query(query.id.clone()).await?;

                        match list_user_files_with_names(user_id, bot_deps.clone()) {
                            Ok(files) => {
                                if files.is_empty() {
                                    if let Some(MaybeInaccessibleMessage::Regular(message)) =
                                        &query.message
                                    {
                                        let kb = InlineKeyboardMarkup::new(vec![
                                            vec![InlineKeyboardButton::callback(
                                                "📎 Upload Files",
                                                "upload_files_prompt",
                                            )],
                                            vec![InlineKeyboardButton::callback(
                                                "↩️ Back",
                                                "back_to_user_settings",
                                            )],
                                        ]);
                                        let text = "✅ <b>File deleted successfully!</b>\n\n📁 <i>Your document library is now empty</i>\n\n💡 Use the button below to add new documents";
                                        bot.edit_message_text(message.chat.id, message.id, text)
                                            .parse_mode(ParseMode::Html)
                                            .reply_markup(kb)
                                            .await?;
                                    }
                                } else {
                                    let file_list = files
                                        .iter()
                                        .map(|file| {
                                            let icon = utils::get_file_icon(&file.name);
                                            let clean_name = utils::clean_filename(&file.name);
                                            format!("{}  <b>{}</b>", icon, clean_name)
                                        })
                                        .collect::<Vec<_>>()
                                        .join("\n");
                                    let response = format!(
                                        "🗂️ <b>Your Document Library</b> ({} files)\n\n{}\n\n💡 <i>Tap any button below to manage your files</i>",
                                        files.len(),
                                        file_list
                                    );
                                    let mut keyboard_rows = Vec::new();
                                    for file in &files {
                                        let clean_name = utils::clean_filename(&file.name);
                                        let button_text = if clean_name.len() > 25 {
                                            format!("🗑️ {}", &clean_name[..22].trim_end())
                                        } else {
                                            format!("🗑️ {}", clean_name)
                                        };
                                        let delete_button = InlineKeyboardButton::callback(
                                            button_text,
                                            format!("delete_file:{}", file.id),
                                        );
                                        keyboard_rows.push(vec![delete_button]);
                                    }
                                    if files.len() > 1 {
                                        let clear_all_button = InlineKeyboardButton::callback(
                                            "🗑️ Clear All Files",
                                            "clear_all_files",
                                        );
                                        keyboard_rows.push(vec![clear_all_button]);
                                    }
                                    let keyboard = InlineKeyboardMarkup::new(keyboard_rows);

                                    if let Some(MaybeInaccessibleMessage::Regular(message)) =
                                        &query.message
                                    {
                                        bot.edit_message_text(message.chat.id, message.id, response)
                                        .parse_mode(ParseMode::Html)
                                        .reply_markup(keyboard)
                                        .await?;
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to list files after deletion: {}", e);
                                bot.answer_callback_query(query.id)
                                    .text("❌ Error refreshing file list. Please reopen the Document Library.")
                                    .await?;
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("File deletion failed: {}", e);
                        let error_msg = e.to_string();

                        // Check if it's a vector store not found error
                        if error_msg.contains("document library is no longer available") {
                            bot.answer_callback_query(query.id)
                                .text("📁 Your document library was removed. Use <Upload Files> to create a new one!")
                                .await?;
                        } else {
                            bot.answer_callback_query(query.id)
                                .text(&format!("❌ Failed to delete file. Error: {}", e))
                                .await?;
                        }
                    }
                }
            } else {
                bot.answer_callback_query(query.id)
                    .text("❌ No document library found. Please reopen the Document Library.")
                    .await?;
            }
        } else if data == "clear_all_files" {
            match delete_vector_store(user_id, bot_deps.clone()).await {
                Ok(_) => {
                    bot.answer_callback_query(query.id).await?;
                    if let Some(MaybeInaccessibleMessage::Regular(message)) = &query.message {
                        let kb = InlineKeyboardMarkup::new(vec![
                            vec![InlineKeyboardButton::callback(
                                "📎 Upload Files",
                                "upload_files_prompt",
                            )],
                            vec![InlineKeyboardButton::callback(
                                "↩️ Back",
                                "back_to_user_settings",
                            )],
                        ]);
                        let text = "✅ <b>All files cleared successfully!</b>\n\n📁 <i>Your document library is now empty</i>\n\n💡 Use the button below to add new documents";
                        bot.edit_message_text(message.chat.id, message.id, text)
                            .parse_mode(teloxide::types::ParseMode::Html)
                            .reply_markup(kb)
                            .await?;
                    }
                }
                Err(e) => {
                    log::error!("Failed to clear all files: {}", e);
                    bot.answer_callback_query(query.id)
                        .text(&format!("❌ Failed to clear files. Error: {}", e))
                        .await?;
                }
            }
        } else if data.starts_with("unmute:") {
            // Handle unmute callback - admin only
            let user_id_str = data.strip_prefix("unmute:").unwrap();
            let target_user_id: i64 = user_id_str.parse().unwrap_or(0);

            if let Some(MaybeInaccessibleMessage::Regular(message)) = &query.message {
                // Check if the user clicking the button is an admin
                let admins = bot.get_chat_administrators(message.chat.id).await?;
                let requester_id = query.from.id;
                let is_admin = admins.iter().any(|member| member.user.id == requester_id);

                if !is_admin {
                    bot.answer_callback_query(query.id)
                        .text("❌ Only administrators can use this action")
                        .await?;
                    return Ok(());
                }

                // Create full permissions to unmute the user
                let full_permissions = teloxide::types::ChatPermissions::all();

                match bot
                    .restrict_chat_member(
                        message.chat.id,
                        teloxide::types::UserId(target_user_id as u64),
                        full_permissions,
                    )
                    .await
                {
                    Ok(_) => {
                        // Delete the moderation notification message
                        if let Err(e) = bot.delete_message(message.chat.id, message.id).await {
                            log::warn!("Failed to delete moderation notification: {}", e);
                        }

                        bot.answer_callback_query(query.id)
                            .text("✅ User unmuted successfully")
                            .await?;

                        log::info!("Admin {} unmuted user {}", requester_id, target_user_id);
                    }
                    Err(e) => {
                        log::error!("Failed to unmute user {}: {}", target_user_id, e);
                        bot.answer_callback_query(query.id)
                            .text("❌ Failed to unmute user")
                            .await?;
                    }
                }
            }
        } else if data.starts_with("ban:") {
            // Handle ban callback - admin only
            // Support formats: "ban:<user_id>" and legacy "ban:<user_id>:<message_id>" (offending message is already deleted on flag)
            let parts: Vec<&str> = data.split(':').collect();
            if parts.len() < 2 {
                bot.answer_callback_query(query.id)
                    .text("❌ Invalid ban action")
                    .await?;
                return Ok(());
            }
            let target_user_id: i64 = parts[1].parse().unwrap_or(0);

            if let Some(MaybeInaccessibleMessage::Regular(message)) = &query.message {
                // Check if the user clicking the button is an admin
                let admins = bot.get_chat_administrators(message.chat.id).await?;
                let requester_id = query.from.id;
                let is_admin = admins.iter().any(|member| member.user.id == requester_id);

                if !is_admin {
                    bot.answer_callback_query(query.id)
                        .text("❌ Only administrators can use this action")
                        .await?;
                    return Ok(());
                }

                match bot
                    .ban_chat_member(
                        message.chat.id,
                        teloxide::types::UserId(target_user_id as u64),
                    )
                    .await
                {
                    Ok(_) => {
                        // Delete the moderation notification message itself
                        if let Err(e) = bot.delete_message(message.chat.id, message.id).await {
                            log::warn!(
                                "Failed to delete moderation message {}: {}",
                                message.id.0,
                                e
                            );
                        }

                        bot.answer_callback_query(query.id)
                            .text("✅ User banned successfully")
                            .await?;

                        log::info!("Admin {} banned user {}", requester_id, target_user_id);
                    }
                    Err(e) => {
                        log::error!("Failed to ban user {}: {}", target_user_id, e);
                        bot.answer_callback_query(query.id)
                            .text("❌ Failed to ban user")
                            .await?;
                    }
                }
            }
        } else if data.starts_with("select_chat_model:")
            || data.starts_with("set_temperature:")
            || data.starts_with("set_gpt5_mode:")
            || data.starts_with("set_gpt5_effort:")
            || data.starts_with("set_gpt5_verbosity:")
            || data.starts_with("set_reasoning:")
            || data.starts_with("set_verbosity:")
            || data == "continue_to_verbosity"
            || data == "back_to_model_selection"
            || data == "back_to_reasoning"
        {
            // Handle model preference callbacks
            handle_model_preferences_callback(bot, query, bot_deps.user_model_prefs.clone())
                .await?;
        } else if data == "open_select_model" {
            // Start the same workflow as /selectmodel: show chat model options
            if let Some(message) = &query.message {
                if let MaybeInaccessibleMessage::Regular(m) = message {
                    let keyboard = InlineKeyboardMarkup::new(vec![
                        vec![InlineKeyboardButton::callback(
                            "GPT-5 (💸 Smart & Creative)",
                            "select_chat_model:GPT5",
                        )],
                        vec![InlineKeyboardButton::callback(
                            "GPT-5-Mini (💵 Cheapest & Fastest)",
                            "select_chat_model:GPT5Mini",
                        )],
                        vec![InlineKeyboardButton::callback(
                            "↩️ Back to Settings",
                            "back_to_user_settings",
                        )],
                    ]);
                    let text = "🤖 <b>Select your chat model:</b>\n\nChoose which model to use for regular chat commands (/c):";
                    bot.edit_message_text(m.chat.id, m.id, text)
                    .reply_markup(keyboard)
                    .parse_mode(ParseMode::Html)
                    .await?;
                }
            }
        } else if data == "open_my_settings" {
            // Render user's current settings using existing logic
            if let Some(message) = &query.message {
                if let MaybeInaccessibleMessage::Regular(m) = message {
                    // Build comprehensive settings view: Model, Mode, Verbosity, Token selected
                    let user = query.from.username.clone();
                    let id = query.from.id;
                    if let Some(username) = user {
                        let prefs = bot_deps.user_model_prefs.get_preferences(&username);
                        // Resolve selected token from user prefs; fall back to default
                        let token_label = if let Some(token) = bot_deps
                            .payment
                            .get_payment_token(id.to_string(), &bot_deps)
                            .await
                        {
                            token.label
                        } else {
                            bot_deps.default_payment_prefs.label
                        };

                        let reasoning_text = if prefs.reasoning_enabled { "On" } else { "Off" };
                        let verbosity_text = prefs.verbosity.to_display_string();

                        // Get effective summarization preferences
                        let user_id = id.0 as i64;
                        let user_id_str = user_id.to_string();

                        // Determine if this is a group chat
                        let group_id = if m.chat.is_group() || m.chat.is_supergroup() {
                            Some(m.chat.id.to_string())
                        } else {
                            None
                        };

                        let sum_prefs = bot_deps
                            .summarization_settings
                            .get_effective_prefs(&user_id_str, group_id);
                        let sum_status = if sum_prefs.enabled { "On" } else { "Off" };

                        let text = format!(
                            "⚙️ <b>Your Settings</b>\n\n🤖 Model: {}\n🧠 Reasoning: {}\n🗣️ Verbosity: {}\n💳 Token: <code>{}</code>\n🧾 Summarizer: {}\n📏 Threshold: {} tokens",
                            prefs.chat_model.to_display_string(),
                            reasoning_text,
                            verbosity_text,
                            token_label,
                            sum_status,
                            sum_prefs.token_limit
                        );

                        let keyboard =
                            InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
                                "↩️ Back to Settings",
                                "back_to_user_settings",
                            )]]);

                        bot.edit_message_text(m.chat.id, m.id, text)
                            .parse_mode(ParseMode::Html)
                            .reply_markup(keyboard)
                            .await?;
                    } else {
                        bot.answer_callback_query(query.id)
                            .text("❌ Username required")
                            .await?;
                    }
                }
            }
        } else if data == "open_payment_settings" {
            // Show submenu with the choose token action and the default currency
            if let Some(message) = &query.message {
                if let MaybeInaccessibleMessage::Regular(m) = message {
                    let mut default_currency = bot_deps.default_payment_prefs.label.clone();

                    let prefs = bot_deps
                        .payment
                        .get_payment_token(m.chat.id.to_string(), &bot_deps)
                        .await;

                    if prefs.is_some() {
                        let prefs = prefs.unwrap();
                        default_currency = prefs.label;
                    }

                    let kb = InlineKeyboardMarkup::new(vec![
                        vec![InlineKeyboardButton::callback(
                            "💳 Choose Payment Token",
                            "payment_selected",
                        )],
                        vec![InlineKeyboardButton::callback(
                            "↩️ Back",
                            "back_to_user_settings",
                        )],
                    ]);
                    let text = format!(
                        "💳 <b>Payment Settings</b>\n\nDefault currency: <code>{}</code>",
                        default_currency
                    );
                    bot.edit_message_text(m.chat.id, m.id, text)
                    .parse_mode(ParseMode::Html)
                    .reply_markup(kb)
                    .await?;
                }
            }
        } else if data == "open_document_library" {
            // Open the user's Document Library within /usersettings (DM context)
            let user_id = query.from.id.0 as i64;

            match list_user_files_with_names(user_id, bot_deps.clone()) {
                Ok(files) => {
                    use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

                    let (text, keyboard) = if files.is_empty() {
                        let kb = InlineKeyboardMarkup::new(vec![
                            vec![InlineKeyboardButton::callback(
                                "📎 Upload Files",
                                "upload_files_prompt",
                            )],
                            vec![InlineKeyboardButton::callback(
                                "↩️ Back",
                                "back_to_user_settings",
                            )],
                        ]);
                        (
                            "📁 <b>Your Document Library</b>\n\n<i>No files uploaded yet</i>\n\n💡 Use the button below to upload your first documents.".to_string(),
                            kb,
                        )
                    } else {
                        let file_list = files
                            .iter()
                            .map(|file| {
                                let icon = utils::get_file_icon(&file.name);
                                let clean_name = utils::clean_filename(&file.name);
                                format!("{}  <b>{}</b>", icon, clean_name)
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                        let response = format!(
                            "🗂️ <b>Your Document Library</b> ({} files)\n\n{}\n\n💡 <i>Tap any button below to manage your files</i>",
                            files.len(),
                            file_list
                        );
                        let mut keyboard_rows = Vec::new();
                        for file in &files {
                            let clean_name = utils::clean_filename(&file.name);
                            let button_text = if clean_name.len() > 25 {
                                format!("🗑️ {}", &clean_name[..22].trim_end())
                            } else {
                                format!("🗑️ {}", clean_name)
                            };
                            let delete_button = InlineKeyboardButton::callback(
                                button_text,
                                format!("delete_file:{}", file.id),
                            );
                            keyboard_rows.push(vec![delete_button]);
                        }
                        if files.len() > 1 {
                            let clear_all_button = InlineKeyboardButton::callback(
                                "🗑️ Clear All Files",
                                "clear_all_files",
                            );
                            keyboard_rows.push(vec![clear_all_button]);
                        }
                        // Upload + Back controls
                        keyboard_rows.push(vec![InlineKeyboardButton::callback(
                            "📎 Upload Files",
                            "upload_files_prompt",
                        )]);
                        keyboard_rows.push(vec![InlineKeyboardButton::callback(
                            "↩️ Back",
                            "back_to_user_settings",
                        )]);
                        (response, InlineKeyboardMarkup::new(keyboard_rows))
                    };

                    if let Some(MaybeInaccessibleMessage::Regular(message)) = &query.message {
                         
                        bot.edit_message_text(message.chat.id, message.id, text)
                            .parse_mode(ParseMode::Html)
                            .reply_markup(keyboard)
                            .await?;
                    }
                }
                Err(e) => {
                    log::error!("Failed to open document library: {}", e);
                    bot.answer_callback_query(query.id)
                        .text("❌ Error loading Document Library")
                        .await?;
                }
            }
        } else if data == "upload_files_prompt" {
            if let Some(MaybeInaccessibleMessage::Regular(message)) = &query.message {
                use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};
                let kb = InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
                    "↩️ Back",
                    "open_document_library",
                )]]);
                let text = "📎 Please attach the documents you wish to upload in your next message.\n\n✅ Supported: Documents (.txt, .md, .py, .js, .pdf, .docx, etc.)\n💡 You can send multiple documents in one message!";
                bot.edit_message_text(message.chat.id, message.id, text)
                    .parse_mode(ParseMode::Html)
                    .reply_markup(kb)
                    .await?;
            }
        } else if data == "back_to_user_settings" {
            if let Some(message) = &query.message {
                if let MaybeInaccessibleMessage::Regular(m) = message {
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
                    bot.edit_message_text(m.chat.id, m.id, text)
                        .parse_mode(ParseMode::Html)
                        .reply_markup(kb)
                        .await?;
                }
            }
        } else if data == "user_settings_close" {
            if let Some(message) = &query.message {
                if let MaybeInaccessibleMessage::Regular(m) = message {
                    let _ = bot.edit_message_reply_markup(m.chat.id, m.id).await;
                    bot.answer_callback_query(query.id).text("Closed").await?;
                }
            }
        } else if data == "open_group_document_library" {
            // Open the group's Document Library within /groupsettings (admin only)
            if let Some(message) = &query.message {
                if let MaybeInaccessibleMessage::Regular(m) = message {
                    let is_admin = utils::is_admin(&bot, m.chat.id, query.from.id).await;
                    if !is_admin {
                        bot.answer_callback_query(query.id)
                            .text("❌ Only administrators can manage group documents")
                            .await?;
                        return Ok(());
                    }

                    let group_id = m.chat.id.to_string();

                    // Use reusable function but edit existing message instead of sending new one
                    match list_group_files_with_names(group_id.clone(), bot_deps.clone()) {
                        Ok(files) => {
                            use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

                            let (text, keyboard) = if files.is_empty() {
                                let kb = InlineKeyboardMarkup::new(vec![
                                    vec![InlineKeyboardButton::callback(
                                        "📎 Upload Files",
                                        "group_upload_files_prompt",
                                    )],
                                    vec![InlineKeyboardButton::callback(
                                        "↩️ Back",
                                        "back_to_group_settings",
                                    )],
                                ]);
                                (
                                    "📁 <b>Group Document Library</b>\n\n<i>No files uploaded yet</i>\n\n💡 Use the button below to upload your first documents for /g commands.".to_string(),
                                    kb,
                                )
                            } else {
                                let file_list = files
                                    .iter()
                                    .map(|file| {
                                        let icon = utils::get_file_icon(&file.name);
                                        let clean_name = utils::clean_filename(&file.name);
                                        format!("{}  <b>{}</b>", icon, clean_name)
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n");
                                let response = format!(
                                    "🗂️ <b>Group Document Library</b> ({} files)\n\n{}\n\n💡 <i>Tap any button below to manage your files</i>",
                                    files.len(),
                                    file_list
                                );
                                let mut keyboard_rows = Vec::new();
                                for file in &files {
                                    let clean_name = utils::clean_filename(&file.name);
                                    let button_text = if clean_name.len() > 25 {
                                        format!("🗑️ {}", &clean_name[..22].trim_end())
                                    } else {
                                        format!("🗑️ {}", clean_name)
                                    };
                                    let delete_button = InlineKeyboardButton::callback(
                                        button_text,
                                        format!("group_delete_file:{}", file.id),
                                    );
                                    keyboard_rows.push(vec![delete_button]);
                                }
                                if files.len() > 1 {
                                    let clear_all_button = InlineKeyboardButton::callback(
                                        "🗑️ Clear All Files",
                                        "group_clear_all_files",
                                    );
                                    keyboard_rows.push(vec![clear_all_button]);
                                }
                                // Upload + Back controls
                                keyboard_rows.push(vec![InlineKeyboardButton::callback(
                                    "📎 Upload Files",
                                    "group_upload_files_prompt",
                                )]);
                                keyboard_rows.push(vec![InlineKeyboardButton::callback(
                                    "↩️ Back",
                                    "back_to_group_settings",
                                )]);
                                (response, InlineKeyboardMarkup::new(keyboard_rows))
                            };

                            bot.edit_message_text(m.chat.id, m.id, text)
                                .parse_mode(ParseMode::Html)
                                .reply_markup(keyboard)
                                .await?;
                        }
                        Err(e) => {
                            log::error!("Failed to open group document library: {}", e);
                            bot.answer_callback_query(query.id)
                                .text("❌ Error loading Group Document Library")
                                .await?;
                        }
                    }
                }
            }
        } else if data.starts_with("group_delete_file:") {
            let file_id = data.strip_prefix("group_delete_file:").unwrap();

            if let Some(message) = &query.message {
                if let MaybeInaccessibleMessage::Regular(m) = message {
                    let is_admin = utils::is_admin(&bot, m.chat.id, query.from.id).await;
                    if !is_admin {
                        bot.answer_callback_query(query.id)
                            .text("❌ Only administrators can manage group documents")
                            .await?;
                        return Ok(());
                    }

                    let group_id = m.chat.id.to_string();

                    if let Some(vector_store_id) = bot_deps
                        .group_docs
                        .get_group_vector_store_id(group_id.clone())
                    {
                        match delete_file_from_group_vector_store(
                            group_id.clone(),
                            bot_deps.clone(),
                            &vector_store_id,
                            file_id,
                        )
                        .await
                        {
                            Ok(_) => {
                                bot.answer_callback_query(query.id.clone()).await?;

                                match list_group_files_with_names(
                                    group_id.clone(),
                                    bot_deps.clone(),
                                ) {
                                    Ok(files) => {
                                        if files.is_empty() {
                                            let text = "✅ <b>File deleted successfully!</b>\n\n📁 <i>Your group document library is now empty</i>\n\n💡 Use <b>Upload Files</b> to add new documents";
                                            bot.edit_message_text(m.chat.id, m.id, text)
                                                .parse_mode(ParseMode::Html)
                                                .reply_markup(InlineKeyboardMarkup::new(vec![
                                                    vec![InlineKeyboardButton::callback(
                                                        "📎 Upload Files",
                                                        "group_upload_files_prompt",
                                                    )],
                                                    vec![InlineKeyboardButton::callback(
                                                        "↩️ Back",
                                                        "back_to_group_settings",
                                                    )],
                                                ]))
                                                .await?;
                                        } else {
                                            let file_list = files
                                                .iter()
                                                .map(|file| {
                                                    let icon = utils::get_file_icon(&file.name);
                                                    let clean_name =
                                                        utils::clean_filename(&file.name);
                                                    format!("{}  <b>{}</b>", icon, clean_name)
                                                })
                                                .collect::<Vec<_>>()
                                                .join("\n");
                                            let response = format!(
                                                "🗂️ <b>Group Document Library</b> ({} files)\n\n{}\n\n💡 <i>Tap any button below to manage your files</i>",
                                                files.len(),
                                                file_list
                                            );
                                            let mut keyboard_rows = Vec::new();
                                            for file in &files {
                                                let clean_name = utils::clean_filename(&file.name);
                                                let button_text = if clean_name.len() > 25 {
                                                    format!("🗑️ {}", &clean_name[..22].trim_end())
                                                } else {
                                                    format!("🗑️ {}", clean_name)
                                                };
                                                let delete_button = InlineKeyboardButton::callback(
                                                    button_text,
                                                    format!("group_delete_file:{}", file.id),
                                                );
                                                keyboard_rows.push(vec![delete_button]);
                                            }
                                            if files.len() > 1 {
                                                let clear_all_button =
                                                    InlineKeyboardButton::callback(
                                                        "🗑️ Clear All Files",
                                                        "group_clear_all_files",
                                                    );
                                                keyboard_rows.push(vec![clear_all_button]);
                                            }
                                            keyboard_rows.push(vec![
                                                InlineKeyboardButton::callback(
                                                    "📎 Upload Files",
                                                    "group_upload_files_prompt",
                                                ),
                                            ]);
                                            keyboard_rows.push(vec![
                                                InlineKeyboardButton::callback(
                                                    "↩️ Back",
                                                    "back_to_group_settings",
                                                ),
                                            ]);
                                            let keyboard = InlineKeyboardMarkup::new(keyboard_rows);

                                            bot.edit_message_text(m.chat.id, m.id, response)
                                                .parse_mode(ParseMode::Html)
                                                .reply_markup(keyboard)
                                                .await?;
                                        }
                                    }
                                    Err(e) => {
                                        log::error!("Failed to list files after deletion: {}", e);
                                        bot.answer_callback_query(query.id)
                                            .text("❌ Error refreshing file list. Please reopen the Group Document Library.")
                                            .await?;
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("Group file deletion failed: {}", e);
                                let error_msg = e.to_string();

                                if error_msg
                                    .contains("group document library is no longer available")
                                {
                                    bot.answer_callback_query(query.id)
                                        .text("📁 Your group document library was removed. Use <Upload Files> to create a new one!")
                                        .await?;
                                } else {
                                    bot.answer_callback_query(query.id)
                                        .text(&format!("❌ Failed to delete file. Error: {}", e))
                                        .await?;
                                }
                            }
                        }
                    } else {
                        bot.answer_callback_query(query.id)
                            .text("❌ No group document library found. Please reopen the Group Document Library.")
                            .await?;
                    }
                }
            }
        } else if data == "group_clear_all_files" {
            if let Some(message) = &query.message {
                if let MaybeInaccessibleMessage::Regular(m) = message {
                    let is_admin = utils::is_admin(&bot, m.chat.id, query.from.id).await;
                    if !is_admin {
                        bot.answer_callback_query(query.id)
                            .text("❌ Only administrators can manage group documents")
                            .await?;
                        return Ok(());
                    }

                    let group_id = m.chat.id.to_string();

                    match delete_group_vector_store(group_id.clone(), bot_deps.clone()).await {
                        Ok(_) => {
                            bot.answer_callback_query(query.id).await?;
                            let text = "✅ <b>All files cleared successfully!</b>\n\n🗑️ <i>Your entire group document library has been deleted</i>\n\n💡 Open <b>Group Settings → Document Library</b> and tap <b>Upload Files</b> to start building your library again";
                            bot.edit_message_text(m.chat.id, m.id, text)
                                .parse_mode(teloxide::types::ParseMode::Html)
                                .reply_markup(InlineKeyboardMarkup::new(vec![
                                    vec![InlineKeyboardButton::callback(
                                        "📎 Upload Files",
                                        "group_upload_files_prompt",
                                    )],
                                    vec![InlineKeyboardButton::callback(
                                        "↩️ Back",
                                        "back_to_group_settings",
                                    )],
                                ]))
                                .await?;
                        }
                        Err(e) => {
                            log::error!("Failed to clear all group files: {}", e);
                            bot.answer_callback_query(query.id)
                                .text(&format!("❌ Failed to clear files. Error: {}", e))
                                .await?;
                        }
                    }
                }
            }
        } else if data == "group_upload_files_prompt" {
            if let Some(message) = &query.message {
                if let MaybeInaccessibleMessage::Regular(m) = message {
                    let is_admin = utils::is_admin(&bot, m.chat.id, query.from.id).await;
                    if !is_admin {
                        bot.answer_callback_query(query.id)
                            .text("❌ Only administrators can manage group documents")
                            .await?;
                        return Ok(());
                    }

                    use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};
                    let kb = InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
                        "↩️ Back",
                        "open_group_document_library",
                    )]]);
                    // Set the group as awaiting files
                    let group_id = m.chat.id.to_string();
                    bot_deps
                        .group_file_upload_state
                        .set_awaiting(group_id)
                        .await;

                    let text = "📎 Please attach the documents you wish to upload to the group document library in your next message.\n\n✅ Supported: Documents (.txt, .md, .py, .js, .pdf, .docx, etc.)\n💡 You can send multiple documents in one message!\n\n🔒 Only administrators can upload files to the group library.";
                    bot.edit_message_text(m.chat.id, m.id, text)
                        .parse_mode(ParseMode::Html)
                        .reply_markup(kb)
                        .await?;
                }
            }
        } else if data == "open_group_payment_settings" {
            // Show group payment settings submenu
            if let Some(message) = &query.message {
                if let MaybeInaccessibleMessage::Regular(m) = message {
                    let is_admin = utils::is_admin(&bot, m.chat.id, query.from.id).await;

                    if !is_admin {
                        bot.answer_callback_query(query.id)
                            .text("❌ Only administrators can manage group payment settings")
                            .await?;
                        return Ok(());
                    }

                    let prefs = bot_deps
                        .payment
                        .get_payment_token(m.chat.id.to_string(), &bot_deps)
                        .await;

                    let default_currency = if prefs.is_some() {
                        prefs.unwrap().label
                    } else {
                        bot_deps.default_payment_prefs.label
                    };

                    let kb = InlineKeyboardMarkup::new(vec![
                        vec![InlineKeyboardButton::callback(
                            "💳 Choose Group Payment Token",
                            "payment_selected",
                        )],
                        vec![InlineKeyboardButton::callback(
                            "↩️ Back",
                            "back_to_group_settings",
                        )],
                    ]);
                    let text = format!(
                        "💳 <b>Group Payment Settings</b>\n\nDefault currency: <code>{}</code>",
                        default_currency
                    );
                    bot.edit_message_text(m.chat.id, m.id, text)
                    .parse_mode(ParseMode::Html)
                    .reply_markup(kb)
                    .await?;
                }
            }
        } else if data == "open_dao_preferences" {
            // Open DAO preferences menu
            if let Some(message) = &query.message {
                if let MaybeInaccessibleMessage::Regular(m) = message {
                    // Check if user is admin
                    let admins = bot.get_chat_administrators(m.chat.id).await?;
                    let requester_id = query.from.id;
                    let is_admin = admins.iter().any(|member| member.user.id == requester_id);

                    if !is_admin {
                        bot.answer_callback_query(query.id)
                            .text("❌ Only administrators can manage DAO preferences")
                            .await?;
                        return Ok(());
                    }

                    let group_id = m.chat.id.to_string();
                    let group_id_formatted =
                        format!("{}-{}", group_id, bot_deps.group.account_seed);

                    // Get current DAO admin preferences
                    let dao_admin_preferences = bot_deps
                        .dao
                        .get_dao_admin_preferences(group_id_formatted.clone());
                    let current_prefs = match dao_admin_preferences {
                        Ok(prefs) => prefs,
                        Err(_) => {
                            // Create default preferences if none exist
                            use crate::dao::dto::DaoAdminPreferences;
                            let default_prefs = DaoAdminPreferences {
                                group_id: group_id_formatted.clone(),
                                expiration_time: 7 * 24 * 60 * 60, // 7 days in seconds
                                interval_active_proposal_notifications: 60 * 60, // 1 hour in seconds
                                interval_dao_results_notifications: 3600,
                                default_dao_token: None,
                                vote_duration: Some(24 * 60 * 60), // Default to 24 hours
                            };

                            // Save default preferences
                            if let Err(_e) = bot_deps.dao.set_dao_admin_preferences(
                                group_id_formatted.clone(),
                                default_prefs.clone(),
                            ) {
                                bot.answer_callback_query(query.id)
                                    .text("❌ Error creating default DAO preferences")
                                    .await?;
                                return Ok(());
                            }
                            default_prefs
                        }
                    };

                    // Create keyboard with current values
                    let keyboard = InlineKeyboardMarkup::new(vec![
                        vec![InlineKeyboardButton::callback(
                            format!(
                                "🗑️ Deletion After Conclusion: {}",
                                utils::format_time_duration(current_prefs.expiration_time)
                            ),
                            format!("dao_set_expiration_{}", group_id_formatted),
                        )],
                        vec![InlineKeyboardButton::callback(
                            format!(
                                "🔔 Notification Interval: {}",
                                utils::format_time_duration(
                                    current_prefs.interval_active_proposal_notifications
                                )
                            ),
                            format!("dao_set_notifications_{}", group_id_formatted),
                        )],
                        vec![InlineKeyboardButton::callback(
                            format!(
                                "🔔 Results Notification: {}",
                                utils::format_time_duration(
                                    current_prefs.interval_dao_results_notifications
                                )
                            ),
                            format!("dao_set_results_notifications_{}", group_id_formatted),
                        )],
                        vec![InlineKeyboardButton::callback(
                            format!(
                                "🗳️ Vote Duration: {}",
                                utils::format_time_duration(
                                    current_prefs.vote_duration.unwrap_or(24 * 60 * 60)
                                )
                            ),
                            format!("dao_set_vote_duration_{}", group_id_formatted),
                        )],
                        vec![InlineKeyboardButton::callback(
                            "↩️ Back",
                            "back_to_group_settings",
                        )],
                    ]);

                    bot.edit_message_text(
                        m.chat.id,
                        m.id,
                        "🏛️ <b>DAO Preferences</b>\n\nConfigure group DAO settings:",
                    )
                    .parse_mode(ParseMode::Html)
                    .reply_markup(keyboard)
                    .await?;
                }
            }
        } else if data == "open_migrate_group_id" {
            // Handle group ID migration
            if let Some(message) = &query.message {
                if let MaybeInaccessibleMessage::Regular(m) = message {
                    // Check if user is admin
                    let admins = bot.get_chat_administrators(m.chat.id).await?;
                    let requester_id = query.from.id;
                    let is_admin = admins.iter().any(|member| member.user.id == requester_id);

                    if !is_admin {
                        bot.answer_callback_query(query.id)
                            .text("❌ Only administrators can migrate group ID")
                            .await?;
                        return Ok(());
                    }

                    let group_credentials = bot_deps.group.get_credentials(m.chat.id);
                    if group_credentials.is_none() {
                        bot.answer_callback_query(query.id)
                            .text("❌ Group not found")
                            .await?;
                        return Ok(());
                    }

                    let group_credentials = group_credentials.unwrap();
                    let transaction_response = bot_deps
                        .service
                        .migrate_group_id(group_credentials.jwt)
                        .await;

                    match transaction_response {
                        Ok(_response) => {
                            // Show popup notification
                            bot.answer_callback_query(query.id.clone())
                                .text("✅ Group ID migrated successfully!")
                                .await?;

                            // Return to group settings menu
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
                                    "🔄 Migrate Group ID",
                                    "open_migrate_group_id",
                                )],
                                vec![InlineKeyboardButton::callback(
                                    "↩️ Close",
                                    "group_settings_close",
                                )],
                            ]);

                            bot.edit_message_text(
                                m.chat.id,
                                m.id,
                                "⚙️ <b>Group Settings</b>\n\n• Configure payment token, DAO preferences, moderation, sponsor settings, welcome settings, filters, and group migration.\n\n💡 Only group administrators can access these settings."
                            )
                            .parse_mode(ParseMode::Html)
                            .reply_markup(kb)
                            .await?;
                        }
                        Err(e) => {
                            bot.answer_callback_query(query.id)
                                .text(&format!("❌ Error migrating group ID: {}", e))
                                .await?;
                        }
                    }
                }
            }
        } else if data == "back_to_group_settings" {
            // Return to main group settings menu
            if let Some(message) = &query.message {
                let is_admin = utils::is_admin(&bot, message.chat().id, query.from.id).await;

                if !is_admin {
                    bot.answer_callback_query(query.id)
                        .text("❌ Only administrators can manage group settings")
                        .await?;
                    return Ok(());
                }

                if let teloxide::types::MaybeInaccessibleMessage::Regular(m) = message {
                    // If a moderation wizard is in progress for this admin, delete its prompt and clear state
                    let moderation_state = bot_deps
                        .moderation
                        .get_moderation_state(m.chat.id.to_string());

                    if moderation_state.is_ok() {
                        let state = moderation_state.unwrap();
                        if let Some(mid) = state.message_id {
                            let deleted_message = bot
                                .delete_message(m.chat.id, teloxide::types::MessageId(mid as i32))
                                .await;

                            if deleted_message.is_err() {
                                log::warn!(
                                    "Failed to delete moderation settings message {}: {}",
                                    mid,
                                    deleted_message.err().unwrap()
                                );
                            }
                        }
                        bot_deps
                            .moderation
                            .remove_moderation_state(m.chat.id.to_string())?;
                    }

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
                    let text = "⚙️ <b>Group Settings</b>\n\n• Configure payment token, DAO preferences, moderation, sponsor settings, command settings, filters, and group migration.\n\n💡 Only group administrators can access these settings.";
                    bot.edit_message_text(m.chat.id, m.id, text)
                        .parse_mode(ParseMode::Html)
                        .reply_markup(kb)
                        .await?;
                }
            }
        } else if data == "group_settings_close" {
            if let Some(message) = &query.message {
                if let MaybeInaccessibleMessage::Regular(m) = message {
                    let is_admin = utils::is_admin(&bot, m.chat.id, query.from.id).await;

                    if !is_admin {
                        bot.answer_callback_query(query.id)
                            .text("❌ Only administrators can manage group settings")
                            .await?;
                        return Ok(());
                    }

                    let moderation_state = bot_deps
                        .moderation
                        .get_moderation_state(m.chat.id.to_string());
                    if moderation_state.is_ok() {
                        let state = moderation_state.unwrap();
                        if let Some(mid) = state.message_id {
                            if let Err(e) = bot
                                .delete_message(m.chat.id, teloxide::types::MessageId(mid as i32))
                                .await
                            {
                                log::warn!(
                                    "Failed to delete moderation wizard message {}: {}",
                                    mid,
                                    e
                                );
                            }
                        }
                        bot_deps
                            .moderation
                            .remove_moderation_state(m.chat.id.to_string())?;
                    }
                    bot.edit_message_reply_markup(m.chat.id, m.id).await?;
                    bot.answer_callback_query(query.id).text("Closed").await?;
                }
            }
        } else if data == "dao_preferences_done"
            || data.starts_with("dao_set_expiration_")
            || data.starts_with("dao_set_notifications_")
            || data.starts_with("dao_set_results_notifications_")
            || data.starts_with("dao_set_token_")
            || data.starts_with("dao_set_vote_duration_")
            || data.starts_with("dao_manage_disabled_")
            || data.starts_with("dao_enable_notifications_")
            || data.starts_with("dao_exp_")
            || data.starts_with("dao_notif_")
            || data.starts_with("dao_res_notif_")
            || data.starts_with("dao_vote_duration_")
            || data == "dao_preferences_back"
        {
            // Handle DAO preferences callbacks
            handle_dao_preference_callback(bot, query, bot_deps).await?;
        } else if data == "open_sponsor_settings"
            || data.starts_with("sponsor_set_")
            || data.starts_with("sponsor_interval_")
            || data.starts_with("sponsor_cooldown_")
            || data == "sponsor_enable"
            || data == "sponsor_disable"
            || data == "sponsor_cancel_input"
        {
            // Handle sponsor settings callbacks
            handle_sponsor_settings_callback(bot, query, bot_deps).await?;
        } else if data == "open_command_settings"
            || data == "toggle_chat_commands"
            || data == "command_settings_back"
        {
            crate::command_settings::handler::handle_command_settings_callback(
                bot, query, bot_deps,
            )
            .await?;
        } else if data.starts_with("welcome_verify:") {
            // Handle welcome verification callback
            log::info!("Received welcome verification callback: {}", data);
            let parts: Vec<&str> = data.split(':').collect();
            log::info!("Callback parts: {:?}", parts);

            if parts.len() == 3 {
                let chat_id = parts[1].parse::<i64>().unwrap_or(0);
                let user_id = parts[2].parse::<u64>().unwrap_or(0);
                log::info!("Parsed chat_id: {}, user_id: {}", chat_id, user_id);

                // Accept negative chat IDs (Telegram supergroups use negative IDs)
                if chat_id != 0 && user_id > 0 {
                    let chat_id = teloxide::types::ChatId(chat_id);
                    let user_id = teloxide::types::UserId(user_id);

                    log::info!(
                        "Calling welcome service verification for chat {} user {} (requested by {})",
                        chat_id.0,
                        user_id.0,
                        query.from.id.0
                    );
                    let welcome_service = bot_deps.welcome_service.clone();
                    match welcome_service
                        .handle_verification(&bot, chat_id, user_id, query.from.id)
                        .await
                    {
                        Ok(_) => {
                            log::info!(
                                "Verification successful for user {} in chat {}",
                                user_id.0,
                                chat_id.0
                            );
                            match bot.answer_callback_query(query.id)
                                .text("✅ Verification successful! You can now participate in the group.")
                                .await {
                                Ok(_) => log::info!("Successfully answered callback query for user {}", user_id.0),
                                Err(e) => log::error!("Failed to answer callback query for user {}: {}", user_id.0, e),
                            }
                        }
                        Err(e) => {
                            log::error!(
                                "Welcome verification failed for user {} in chat {}: {}",
                                user_id.0,
                                chat_id.0,
                                e
                            );
                            match bot
                                .answer_callback_query(query.id)
                                .text("❌ Verification failed. Please contact an administrator.")
                                .await
                            {
                                Ok(_) => log::info!(
                                    "Successfully answered callback query for user {}",
                                    user_id.0
                                ),
                                Err(e) => log::error!(
                                    "Failed to answer callback query for user {}: {}",
                                    user_id.0,
                                    e
                                ),
                            }
                        }
                    }
                } else {
                    log::error!(
                        "Invalid chat_id or user_id: chat_id={}, user_id={}",
                        chat_id,
                        user_id
                    );
                }
            } else {
                log::error!(
                    "Invalid callback format: expected 3 parts, got {}",
                    parts.len()
                );
            }
        } else if data == "welcome_settings"
            || data.starts_with("welcome_")
            || data.starts_with("welcome_back_to_")
        {
            // Handle welcome settings callbacks
            handle_welcome_settings_callback(bot, query, bot_deps).await?;
        } else if data == "open_summarization_settings"
            || data == "open_group_summarization_settings"
            || data.starts_with("toggle_summarizer:")
            || data.starts_with("set_summarizer_threshold:")
            || data == "summarization_back_to_usersettings"
            || data == "summarization_back_to_groupsettings"
        {
            // Handle summarization settings callbacks
            crate::summarization_settings::handler::handle_summarization_settings_callback(
                bot,
                query,
                bot_deps.summarization_settings,
            )
            .await?;
        } else if data == "open_moderation_settings" {
            // Open Moderation submenu inside Group Settings
            if let Some(message) = &query.message {
                if let MaybeInaccessibleMessage::Regular(m) = message {
                    // Admin check
                    let is_admin = utils::is_admin(&bot, m.chat.id, query.from.id).await;
                    if !is_admin {
                        bot.answer_callback_query(query.id)
                            .text("❌ Only administrators can manage moderation settings")
                            .await?;
                        return Ok(());
                    }

                    // Read sentinel state

                    let sentinel_on = bot_deps.sentinel.get_sentinel(m.chat.id.to_string());

                    // Read moderation settings for this group
                    let settings = bot_deps
                        .moderation
                        .get_moderation_settings(m.chat.id.to_string())
                        .unwrap_or(ModerationSettings::from((vec![], vec![], 0, 0)));

                    let text = format!(
                        concat!(
                            "🛡️ <b>Moderation Settings</b>\n\n",
                            "Sentinel: <b>{sentinel}</b>\n",
                            "Custom Rules: <b>{allowed}</b> allowed, <b>{disallowed}</b> disallowed\n",
                            "Updated: <i>{updated}</i>\n\n",
                            "Choose an action below:"
                        ),
                        sentinel = if sentinel_on { "ON" } else { "OFF" },
                        allowed = settings.allowed_items.len(),
                        disallowed = settings.disallowed_items.len(),
                        updated = settings.updated_at_unix_ms.to_string(),
                    );

                    let toggle_label = if sentinel_on {
                        "🔕 Turn OFF Sentinel"
                    } else {
                        "🛡️ Turn ON Sentinel"
                    };
                    let toggle_cb = if sentinel_on {
                        "mod_toggle_sentinel_off"
                    } else {
                        "mod_toggle_sentinel_on"
                    };
                    let kb = InlineKeyboardMarkup::new(vec![
                        vec![InlineKeyboardButton::callback(toggle_label, toggle_cb)],
                        vec![InlineKeyboardButton::callback(
                            "📝 Start Moderation Wizard",
                            "mod_settings_start",
                        )],
                        vec![InlineKeyboardButton::callback(
                            "🧹 Reset Custom Rules",
                            "mod_reset",
                        )],
                        vec![InlineKeyboardButton::callback(
                            "✅ Show Allowed Rules",
                            "mod_show_allowed",
                        )],
                        vec![InlineKeyboardButton::callback(
                            "⛔ Show Disallowed Rules",
                            "mod_show_disallowed",
                        )],
                        vec![InlineKeyboardButton::callback(
                            "📜 Show Default Rules",
                            "mod_show_defaults",
                        )],
                        vec![InlineKeyboardButton::callback(
                            "↩️ Back",
                            "back_to_group_settings",
                        )],
                    ]);
                    bot.edit_message_text(m.chat.id, m.id, text)
                        .parse_mode(teloxide::types::ParseMode::Html)
                        .reply_markup(kb)
                        .await?;
                }
            }
        } else if data == "mod_toggle_sentinel_on" || data == "mod_toggle_sentinel_off" {
            // Toggle sentinel ON/OFF
            if let Some(message) = &query.message {
                if let MaybeInaccessibleMessage::Regular(m) = message {
                    let is_admin = utils::is_admin(&bot, m.chat.id, query.from.id).await;
                    if !is_admin {
                        bot.answer_callback_query(query.id)
                            .text("❌ Only administrators can manage moderation settings")
                            .await?;
                        return Ok(());
                    }
                    if data == "mod_toggle_sentinel_on" {
                        bot_deps.sentinel.set_sentinel(m.chat.id.to_string(), true);
                        bot.answer_callback_query(query.id)
                            .text("🛡️ Sentinel is now ON")
                            .await?;
                    } else {
                        bot_deps.sentinel.set_sentinel(m.chat.id.to_string(), false);
                        bot.answer_callback_query(query.id)
                            .text("🔕 Sentinel is now OFF")
                            .await?;
                    }
                    // Refresh submenu
                    // Reuse the same rendering path by simulating the branch
                    // (duplicate minimal logic for clarity)
                    let sentinel_on = bot_deps.sentinel.get_sentinel(m.chat.id.to_string());
                    let settings = bot_deps
                        .moderation
                        .get_moderation_settings(m.chat.id.to_string())
                        .unwrap_or(ModerationSettings::from((vec![], vec![], 0, 0)));

                    let text = format!(
                        concat!(
                            "🛡️ <b>Moderation Settings</b>\n\n",
                            "Sentinel: <b>{sentinel}</b>\n",
                            "Custom Rules: <b>{allowed}</b> allowed, <b>{disallowed}</b> disallowed\n",
                            "Updated: <i>{updated}</i>\n\n",
                            "Choose an action below:"
                        ),
                        sentinel = if sentinel_on { "ON" } else { "OFF" },
                        allowed = settings.allowed_items.len(),
                        disallowed = settings.disallowed_items.len(),
                        updated = settings.updated_at_unix_ms.to_string(),
                    );
                    let toggle_label = if sentinel_on {
                        "🔕 Turn OFF Sentinel"
                    } else {
                        "🛡️ Turn ON Sentinel"
                    };
                    let toggle_cb = if sentinel_on {
                        "mod_toggle_sentinel_off"
                    } else {
                        "mod_toggle_sentinel_on"
                    };
                    let kb = InlineKeyboardMarkup::new(vec![
                        vec![InlineKeyboardButton::callback(toggle_label, toggle_cb)],
                        vec![InlineKeyboardButton::callback(
                            "📝 Start Moderation Wizard",
                            "mod_settings_start",
                        )],
                        vec![InlineKeyboardButton::callback(
                            "🧹 Reset Custom Rules",
                            "mod_reset",
                        )],
                        vec![InlineKeyboardButton::callback(
                            "✅ Show Allowed Rules",
                            "mod_show_allowed",
                        )],
                        vec![InlineKeyboardButton::callback(
                            "⛔ Show Disallowed Rules",
                            "mod_show_disallowed",
                        )],
                        vec![InlineKeyboardButton::callback(
                            "📜 Show Default Rules",
                            "mod_show_defaults",
                        )],
                        vec![InlineKeyboardButton::callback(
                            "↩️ Back",
                            "back_to_group_settings",
                        )],
                    ]);
                    bot.edit_message_text(m.chat.id, m.id, text)
                        .parse_mode(ParseMode::Html)
                        .reply_markup(kb)
                        .await?;
                }
            }
        } else if data == "mod_settings_start" {
            // Initialize moderation wizard for the requesting admin
            if let Some(message) = &query.message {
                if let teloxide::types::MaybeInaccessibleMessage::Regular(m) = message {
                    let is_admin = utils::is_admin(&bot, m.chat.id, query.from.id).await;
                    if !is_admin {
                        bot.answer_callback_query(query.id)
                            .text("❌ Only administrators can manage moderation settings")
                            .await?;
                        return Ok(());
                    }

                    let mut state = ModerationState::from((
                        "AwaitingAllowed".to_string(),
                        None,
                        None,
                        query.from.id.0 as i64,
                    ));
                    // Prompt Step 1/2 in chat
                    let sent = bot
                        .send_message(
                            m.chat.id,
                            "🛡️ <b>Moderation Settings — Step 1/2</b>\n\n<b>Send ALLOWED items</b> for this group.\n\n<b>Be specific</b>: include concrete phrases and examples.\n\n<b>Cancel anytime</b>: Tap <b>Back</b> or <b>Close</b> in the Moderation menu — this prompt will be removed.\n\n<b>Warning</b>: Allowed items can reduce moderation strictness; we've included a <b>copy & paste</b> template below to safely allow discussion of your token.\n\n<b>Format</b>:\n- Send them in a <b>single message</b>\n- Separate each item with <code>;</code>\n\n<b>Example</b>:\n<b>discussion of APT token and ecosystem; official project links and documentation; community updates and announcements</b>\n\n<b>Quick template (copy/paste) to allow your own token</b>:\n<code>discussion of [YOUR_TOKEN] and ecosystem; official project links and documentation; community updates and announcements</code>\n\n<i>Note:</i> Default rules still protect against scams, phishing, and inappropriate content.\n\nWhen ready, send your list now — or use the button below to skip.",
                        )
                        .parse_mode(ParseMode::Html)
                        .reply_markup(InlineKeyboardMarkup::new(vec![
                            vec![InlineKeyboardButton::callback(
                                "⏭️ Skip Allowed",
                                "mod_skip_allowed",
                            )],
                        ]))
                        .reply_to(m.id)
                        .await?;

                    state.message_id = Some(sent.id.0 as i64);
                    bot_deps
                        .moderation
                        .set_moderation_state(m.chat.id.to_string(), state)?;
                    bot.answer_callback_query(query.id)
                        .text("📝 Wizard started")
                        .await?;
                }
            }
        } else if data == "mod_skip_allowed" {
            // Skip Step 1 (Allowed) and move to Step 2
            if let Some(message) = &query.message {
                if let teloxide::types::MaybeInaccessibleMessage::Regular(m) = message {
                    // Verify there's an active wizard and the caller is the owner
                    if let Ok(mut state) = bot_deps
                        .moderation
                        .get_moderation_state(m.chat.id.to_string())
                    {
                        // Ensure only the admin who started the wizard can skip
                        if let Some(owner) = state.started_by_user_id {
                            if owner != query.from.id.0 as i64 {
                                bot.answer_callback_query(query.id)
                                    .text("❌ Only the admin who started the wizard can skip this step")
                                    .await?;
                                return Ok(());
                            }
                        }

                        if state.step != "AwaitingAllowed" {
                            bot.answer_callback_query(query.id)
                                .text("❌ Wizard is not waiting for Allowed items")
                                .await?;
                            return Ok(());
                        }

                        // Remove previous prompt if present
                        if let Some(mid) = state.message_id {
                            let _ = bot
                                .delete_message(m.chat.id, teloxide::types::MessageId(mid as i32))
                                .await;
                        }

                        // Advance to Step 2 (Disallowed)
                        state.allowed_items = Some(vec![]);
                        state.step = "AwaitingDisallowed".to_string();

                        let sent = bot
                            .send_message(
                                m.chat.id,
                                "🛡️ <b>Moderation Settings — Step 2/2</b>\n\n<b>Now send DISALLOWED items</b> for this group.\n\n<b>Be specific</b>: include concrete phrases, patterns, and examples you want flagged.\n\n<b>Cancel anytime</b>: Tap <b>Back</b> or <b>Close</b> in the Moderation menu — this prompt will be removed.\n\n<b>Format</b>:\n- Send them in a <b>single message</b>\n- Separate each item with <code>;</code>\n\n<b>Examples (community standards)</b>:\n<code>harassment, insults, or personal attacks; hate speech or slurs (racism, homophobia, etc.); doxxing or sharing private information; NSFW/explicit content; graphic violence/gore; off-topic spam or mass mentions; repeated flooding/emoji spam; political or religious debates (off-topic); promotion of unrelated/non-affiliated projects; misinformation/FUD targeting members</code>\n\n<i>Notes:</i> \n- Avoid duplicating default scam rules (phishing links, wallet approvals, DM requests, giveaways) — those are already enforced by Default Rules.\n- <b>Group Disallowed</b> > <b>Group Allowed</b> > <b>Default Rules</b> (strict priority).\n- If any Group Disallowed item matches, the message will be flagged.\n\nWhen ready, send your list now — or use the button below to skip.",
                            )
                            .parse_mode(ParseMode::Html)
                            .reply_markup(InlineKeyboardMarkup::new(vec![
                                vec![InlineKeyboardButton::callback(
                                    "⏭️ Skip Disallowed",
                                    "mod_skip_disallowed",
                                )],
                            ]))
                            .reply_to(m.id)
                            .await?;

                        state.message_id = Some(sent.id.0 as i64);
                        bot_deps
                            .moderation
                            .set_moderation_state(m.chat.id.to_string(), state)?;

                        bot.answer_callback_query(query.id)
                            .text("⏭️ Skipped Allowed. Now send DISALLOWED items.")
                            .await?;
                    } else {
                        bot.answer_callback_query(query.id)
                            .text("❌ No active moderation wizard")
                            .await?;
                    }
                }
            }
        } else if data == "mod_skip_disallowed" {
            // Finish wizard with empty Disallowed
            if let Some(message) = &query.message {
                if let teloxide::types::MaybeInaccessibleMessage::Regular(m) = message {
                    if let Ok(state) = bot_deps
                        .moderation
                        .get_moderation_state(m.chat.id.to_string())
                    {
                        if state.step != "AwaitingDisallowed" {
                            bot.answer_callback_query(query.id)
                                .text("❌ Wizard is not waiting for Disallowed items")
                                .await?;
                            return Ok(());
                        }
                        if let Some(owner) = state.started_by_user_id {
                            if owner != query.from.id.0 as i64 {
                                bot.answer_callback_query(query.id)
                                    .text("❌ Only the admin who started the wizard can skip this step")
                                    .await?;
                                return Ok(());
                            }
                        }

                        // Remove current prompt if present
                        if let Some(mid) = state.message_id {
                            let _ = bot
                                .delete_message(m.chat.id, teloxide::types::MessageId(mid as i32))
                                .await;
                        }

                        let allowed = state.allowed_items.unwrap_or_default();
                        let disallowed: Vec<String> = vec![];

                        let settings = ModerationSettings::from((
                            allowed.clone(),
                            disallowed.clone(),
                            query.from.id.0 as i64,
                            chrono::Utc::now().timestamp_millis(),
                        ));
                        bot_deps
                            .moderation
                            .set_or_update_moderation_settings(m.chat.id.to_string(), settings)?;
                        bot_deps
                            .moderation
                            .remove_moderation_state(m.chat.id.to_string())?;

                        let allowed_list = if allowed.is_empty() {
                            "<i>(none)</i>".to_string()
                        } else {
                            allowed
                                .iter()
                                .map(|x| format!("• {}", teloxide::utils::html::escape(x)))
                                .collect::<Vec<_>>()
                                .join("\n")
                        };
                        let disallowed_list = "<i>(none)</i>".to_string();

                        let mut summary = format!(
                            "✅ <b>Custom moderation rules saved.</b>\n\n<b>Allowed ({})</b>:\n{}\n\n<b>Disallowed ({})</b>:\n{}",
                            allowed.len(),
                            allowed_list,
                            0,
                            disallowed_list,
                        );
                        if allowed.is_empty() {
                            summary.push_str("\n\n<i>No custom rules recorded. Default moderation rules remain fully in effect.</i>");
                        }
                        send_html_message(*m.clone(), bot.clone(), summary).await?;
                        bot.answer_callback_query(query.id)
                            .text("⏭️ Skipped Disallowed and saved.")
                            .await?;
                    } else {
                        bot.answer_callback_query(query.id)
                            .text("❌ No active moderation wizard")
                            .await?;
                    }
                }
            }
        } else if data == "mod_reset" {
            // Reset custom rules for this group and clear wizard for this admin
            if let Some(message) = &query.message {
                if let teloxide::types::MaybeInaccessibleMessage::Regular(m) = message {
                    let is_admin = utils::is_admin(&bot, m.chat.id, query.from.id).await;
                    if !is_admin {
                        bot.answer_callback_query(query.id)
                            .text("❌ Only administrators can manage moderation settings")
                            .await?;
                        return Ok(());
                    }
                    bot_deps
                        .moderation
                        .remove_moderation_state(m.chat.id.to_string())?;
                    bot_deps.moderation.set_or_update_moderation_settings(
                        m.chat.id.to_string(),
                        ModerationSettings::from((vec![], vec![], 0, 0)),
                    )?;
                    bot.answer_callback_query(query.id)
                        .text("🧹 Custom rules reset")
                        .await?;
                    // Re-open moderation settings view
                    let sentinel_on = bot_deps.sentinel.get_sentinel(m.chat.id.to_string());
                    let text = format!(
                        concat!(
                            "🛡️ <b>Moderation Settings</b>\n\n",
                            "Sentinel: <b>{sentinel}</b>\n",
                            "Custom Rules: <b>0</b> allowed, <b>0</b> disallowed\n",
                            "Updated: <i>(none)</i>\n\n",
                            "Choose an action below:"
                        ),
                        sentinel = if sentinel_on { "ON" } else { "OFF" },
                    );
                    let toggle_label = if sentinel_on {
                        "🔕 Turn OFF Sentinel"
                    } else {
                        "🛡️ Turn ON Sentinel"
                    };
                    let toggle_cb = if sentinel_on {
                        "mod_toggle_sentinel_off"
                    } else {
                        "mod_toggle_sentinel_on"
                    };
                    let kb = InlineKeyboardMarkup::new(vec![
                        vec![InlineKeyboardButton::callback(toggle_label, toggle_cb)],
                        vec![InlineKeyboardButton::callback(
                            "📝 Start Moderation Wizard",
                            "mod_settings_start",
                        )],
                        vec![InlineKeyboardButton::callback(
                            "🧹 Reset Custom Rules",
                            "mod_reset",
                        )],
                        vec![InlineKeyboardButton::callback(
                            "✅ Show Allowed Rules",
                            "mod_show_allowed",
                        )],
                        vec![InlineKeyboardButton::callback(
                            "⛔ Show Disallowed Rules",
                            "mod_show_disallowed",
                        )],
                        vec![InlineKeyboardButton::callback(
                            "📜 Show Default Rules",
                            "mod_show_defaults",
                        )],
                        vec![InlineKeyboardButton::callback(
                            "🎯 Sponsor Settings",
                            "open_sponsor_settings",
                        )],
                        vec![InlineKeyboardButton::callback(
                            "↩️ Back",
                            "back_to_group_settings",
                        )],
                    ]);
                    bot.edit_message_text(m.chat.id, m.id, text)
                        .parse_mode(ParseMode::Html)
                        .reply_markup(kb)
                        .await?;
                }
            }
        } else if data == "mod_show_defaults" {
            // Show default moderation rules
            if let Some(message) = &query.message {
                if let teloxide::types::MaybeInaccessibleMessage::Regular(m) = message {
                    let rules = r#"
<b>🛡️ Moderation Rules</b>

To avoid being muted or banned, please follow these rules:

<b>1. No Promotion or Selling</b>
- Do not offer services, products, access, or benefits
- Do not position yourself as an authority/leader to gain trust
- Do not promise exclusive opportunities or deals
- No commercial solicitation of any kind

<b>2. No Private Communication Invites</b>
- Do not request to move conversation to DM/private
- Do not offer to send details privately
- Do not ask for personal contact information
- Do not attempt to bypass public group discussion

<b>Examples (not exhaustive):</b>
- "I can offer you whitelist access"
- "DM me for details"
- "React and I'll message you"
- "I'm a [title] and can help you"
- "Send me your wallet address"
- "Contact me privately"
- "I'll send you the link"

If you have questions, ask an admin before posting.
"#;
                    send_html_message(*m.clone(), bot.clone(), rules.to_string()).await?;
                    bot.answer_callback_query(query.id)
                        .text("📜 Default rules sent")
                        .await?;
                }
            }
        } else if data == "mod_show_allowed" || data == "mod_show_disallowed" {
            // Show current Allowed or Disallowed custom rules
            if let Some(message) = &query.message {
                if let teloxide::types::MaybeInaccessibleMessage::Regular(m) = message {
                    let settings = bot_deps
                        .moderation
                        .get_moderation_settings(m.chat.id.to_string())
                        .unwrap_or(ModerationSettings::from((vec![], vec![], 0, 0)));

                    let (title, items) = if data == "mod_show_allowed" {
                        ("✅ <b>Allowed Rules</b>", settings.allowed_items)
                    } else {
                        ("⛔ <b>Disallowed Rules</b>", settings.disallowed_items)
                    };

                    let body = if items.is_empty() {
                        "<i>No custom rules set.</i>".to_string()
                    } else {
                        items
                            .iter()
                            .map(|x| format!("• {}", x))
                            .collect::<Vec<_>>()
                            .join("\n")
                    };

                    send_html_message(
                        *m.clone(),
                        bot.clone(),
                        format!("{title}\n\n{body}", title = title, body = body),
                    )
                    .await?;
                    bot.answer_callback_query(query.id).text("✅ Sent").await?;
                }
            }
        } else if data == "disable_notifications" {
            // Handle disable notifications callback
            handle_disable_notifications_callback(bot, query, bot_deps).await?;
        } else if data == "voting_help" {
            // Handle voting help callback
            bot.answer_callback_query(query.id)
                .text("📱 Mini App: Opens voting interface inside Telegram\n🌐 Browser: Opens voting page in external browser\n\nBoth options work the same way!")
                .show_alert(true)
                .await?;
        } else if data.starts_with("pay_accept:") || data.starts_with("pay_reject:") {
            // Handle payment confirmation callbacks
            handle_payment_callback(bot, query, bot_deps).await?;
        } else if data.starts_with("sched_") {
            // Handle scheduled prompts wizard and management callbacks
            handle_scheduled_prompts_callback(bot, query, bot_deps).await?;
        } else if data.starts_with("schedpay_") {
            // Handle scheduled payments wizard and management callbacks
            handle_scheduled_payments_callback(bot, query, bot_deps).await?;
        } else if data.starts_with("filters_") {
            // Handle filters callbacks
            handle_filters_callback(bot, query, bot_deps).await?;
        } else if data == "open_payment_settings"
            || data == "open_group_payment_settings"
            || data == "payment_selected"
            || data.starts_with("pay_tokpage:")
            || data.starts_with("pay_selid-")
        {
            // Handle all payment-related callbacks
            crate::payment::handler::handle_payment(bot, query, bot_deps).await?;
        } else {
            bot.answer_callback_query(query.id)
                .text("❌ Unknown action")
                .await?;
        }
    }

    Ok(())
}

pub async fn handle_payment_callback(
    bot: Bot,
    query: teloxide::types::CallbackQuery,
    bot_deps: BotDependencies,
) -> Result<()> {
    let data = query.data.as_ref().unwrap();

    // Parse callback data: pay_accept:user_id:group_id:transaction_id or pay_reject:user_id:group_id:transaction_id
    let parts: Vec<&str> = data.split(':').collect();
    if parts.len() != 4 {
        bot.answer_callback_query(query.id)
            .text("❌ Invalid callback data")
            .await?;
        return Ok(());
    }

    let action = parts[0];
    let user_id = parts[1].parse::<i64>();
    let group_id_i64 = parts[2].parse::<i64>();
    let transaction_id = parts[3].to_string();

    if user_id.is_err() || group_id_i64.is_err() {
        bot.answer_callback_query(query.id)
            .text("❌ Invalid user or group ID")
            .await?;
        return Ok(());
    }

    let user_id = user_id.unwrap();
    let group_id_i64 = group_id_i64.unwrap();

    // SECURITY CHECK: Verify that the user clicking the button is authorized
    let callback_user_id = query.from.id.0 as i64;

    // Only the original requester can confirm/cancel transactions (both individual and group context)
    if callback_user_id != user_id {
        bot.answer_callback_query(query.id)
            .text("❌ Only the user who requested this transaction can confirm or cancel it")
            .await?;
        return Ok(());
    }

    // Convert group_id back to Option<i64> format used by pending_transactions
    let group_id_opt = if group_id_i64 == 0 {
        None
    } else {
        Some(group_id_i64)
    };

    // Get the pending transaction
    let pending_transaction = bot_deps
        .pending_transactions
        .get_pending_transaction(user_id, group_id_opt);

    if pending_transaction.is_none() {
        bot.answer_callback_query(query.id)
            .text("❌ No pending transaction found")
            .await?;

        // Edit the message to remove buttons
        if let Some(message) = &query.message {
            if let MaybeInaccessibleMessage::Regular(msg) = message {
                if let Err(e) = bot.edit_message_reply_markup(msg.chat.id, msg.id).await {
                    log::warn!("Failed to clear reply markup: {}", e);
                }
            }
        }
        return Ok(());
    }

    let pending_transaction = pending_transaction.unwrap();

    // TRANSACTION ID VALIDATION: Verify that the callback transaction ID matches the stored transaction
    if pending_transaction.transaction_id != transaction_id {
        bot.answer_callback_query(query.id)
            .text("❌ Transaction ID mismatch - invalid callback")
            .await?;

        log::warn!(
            "Transaction ID mismatch: callback={}, stored={}",
            transaction_id,
            pending_transaction.transaction_id
        );
        return Ok(());
    }

    // Check if transaction has expired
    if crate::pending_transactions::handler::PendingTransactions::is_expired(&pending_transaction) {
        // Remove expired transaction
        if let Err(e) = bot_deps
            .pending_transactions
            .delete_pending_transaction(user_id, group_id_opt)
        {
            log::warn!("Failed to delete expired pending transaction: {}", e);
        }

        bot.answer_callback_query(query.id)
            .text("❌ Transaction has expired (1 minute timeout)")
            .await?;

        // Edit the message to show expiration
        if let Some(message) = &query.message {
            if let MaybeInaccessibleMessage::Regular(msg) = message {
                let recipients_text = if pending_transaction.original_usernames.len() == 1 {
                    format!("@{}", pending_transaction.original_usernames[0])
                } else {
                    pending_transaction
                        .original_usernames
                        .iter()
                        .map(|username| format!("@{}", username))
                        .collect::<Vec<_>>()
                        .join(", ")
                };

                let expired_message = format!(
                    "⏰ <b>Transaction expired</b>\n\n💰 {:.2} {} to {} was not sent.\n\n<i>Transactions expire after 1 minute for security.</i>",
                    pending_transaction.per_user_amount
                        * pending_transaction.original_usernames.len() as f64,
                    pending_transaction.symbol,
                    recipients_text
                );

                bot.edit_message_text(msg.chat.id, msg.id, expired_message)
                    .parse_mode(ParseMode::Html)
                    .await?;
            }
        }
        return Ok(());
    }

    match action {
        "pay_accept" => {
            // Execute the transaction
            let pay_request =
                crate::pending_transactions::handler::PendingTransactions::to_pay_users_request(
                    &pending_transaction,
                );

            let result = if pending_transaction.is_group_transfer {
                bot_deps
                    .service
                    .pay_members(pending_transaction.jwt_token, pay_request)
                    .await
            } else {
                bot_deps
                    .service
                    .pay_users(pending_transaction.jwt_token, pay_request)
                    .await
            };

            match result {
                Ok(response) => {
                    // Delete the pending transaction ONLY after successful payment
                    if let Err(e) = bot_deps
                        .pending_transactions
                        .delete_pending_transaction(user_id, group_id_opt)
                    {
                        log::warn!("Failed to delete pending transaction after payment: {}", e);
                    }

                    let network = std::env::var("APTOS_NETWORK")
                        .unwrap_or("mainnet".to_string())
                        .to_lowercase();

                    let recipients_text = if pending_transaction.original_usernames.len() == 1 {
                        format!("@{}", pending_transaction.original_usernames[0])
                    } else {
                        pending_transaction
                            .original_usernames
                            .iter()
                            .map(|username| format!("@{}", username))
                            .collect::<Vec<_>>()
                            .join(", ")
                    };

                    let success_message = format!(
                        "✅ <b>Payment sent successfully!</b>\n\n💰 {:.2} {} sent to {} ({:.2} each)\n\n🔗 <a href=\"https://explorer.aptoslabs.com/txn/{}?network={}\">View transaction</a>",
                        pending_transaction.per_user_amount
                            * pending_transaction.original_usernames.len() as f64,
                        pending_transaction.symbol,
                        recipients_text,
                        pending_transaction.per_user_amount,
                        response.hash,
                        network
                    );

                    // Edit the original message
                    if let Some(message) = &query.message {
                        if let MaybeInaccessibleMessage::Regular(msg) = message {
                            bot.edit_message_text(msg.chat.id, msg.id, success_message)
                                .parse_mode(ParseMode::Html)
                                .await?;
                        }
                    }

                    bot.answer_callback_query(query.id)
                        .text("✅ Payment executed successfully!")
                        .await?;
                }
                Err(e) => {
                    let error_message = format!("❌ <b>Payment failed</b>\n\n{}", e);

                    // Edit the original message
                    if let Some(message) = &query.message {
                        if let MaybeInaccessibleMessage::Regular(msg) = message {
                            bot.edit_message_text(msg.chat.id, msg.id, error_message)
                                .parse_mode(ParseMode::Html)
                                .await?;
                        }
                    }

                    bot.answer_callback_query(query.id)
                        .text("❌ Payment failed")
                        .await?;
                }
            }
        }
        "pay_reject" => {
            // Delete the pending transaction
            if let Err(e) = bot_deps
                .pending_transactions
                .delete_pending_transaction(user_id, group_id_opt)
            {
                log::warn!("Failed to delete pending transaction on cancel: {}", e);
            }

            let recipients_text = if pending_transaction.original_usernames.len() == 1 {
                format!("@{}", pending_transaction.original_usernames[0])
            } else {
                pending_transaction
                    .original_usernames
                    .iter()
                    .map(|username| format!("@{}", username))
                    .collect::<Vec<_>>()
                    .join(", ")
            };

            let cancel_message = format!(
                "❌ <b>Payment cancelled</b>\n\n💰 {:.2} {} to {} was not sent.",
                pending_transaction.per_user_amount
                    * pending_transaction.original_usernames.len() as f64,
                pending_transaction.symbol,
                recipients_text
            );

            // Edit the original message
            if let Some(message) = &query.message {
                if let MaybeInaccessibleMessage::Regular(msg) = message {
                    bot.edit_message_text(msg.chat.id, msg.id, cancel_message)
                        .parse_mode(ParseMode::Html)
                        .await?;
                }
            }

            bot.answer_callback_query(query.id)
                .text("❌ Payment cancelled")
                .await?;
        }
        _ => {
            bot.answer_callback_query(query.id)
                .text("❌ Unknown action")
                .await?;
        }
    }

    Ok(())
}
