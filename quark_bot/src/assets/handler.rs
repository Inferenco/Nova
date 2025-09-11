//! File upload and processing logic for quark_bot.

use crate::ai::group_vector_store::{
    list_group_files_with_names, upload_files_to_group_vector_store,
};
use crate::ai::vector_store::{list_user_files_with_names, upload_files_to_vector_store};
use crate::dependencies::BotDependencies;
use crate::utils::{self, KeyboardMarkupType, send_markdown_message_with_keyboard, send_message};
use anyhow::Result as AnyResult;
use std::time::Duration;
use teloxide::net::Download;
use teloxide::prelude::*;
use teloxide::types::{ChatAction, InlineKeyboardButton, InlineKeyboardMarkup, ParseMode};
use tokio::time::sleep;

pub async fn handle_file_upload(
    bot: Bot,
    msg: Message,
    bot_deps: BotDependencies,
) -> AnyResult<()> {
    use tokio::fs::File;
    let user_id = msg.from.as_ref().map(|u| u.id.0).unwrap_or(0) as i64;
    let chat_id = msg.chat.id;
    let mut file_paths = Vec::new();
    // Handle document
    if let Some(document) = msg.document() {
        let file_id = &document.file.id;
        let file_info = bot.get_file(file_id.clone()).await?;
        let filename = document
            .file_name
            .clone()
            .unwrap_or_else(|| "document.bin".to_string());
        let file_path = format!("/tmp/{}_{}", user_id, filename);
        let mut file = File::create(&file_path)
            .await
            .map_err(|e| teloxide::RequestError::from(std::sync::Arc::new(e)))?;
        bot.download_file(&file_info.path, &mut file)
            .await
            .map_err(|e| teloxide::RequestError::from(e))?;
        file_paths.push(file_path);
    }
    // Only documents are supported for vector stores
    // Photos, videos, and audio cannot be processed for semantic search
    if !file_paths.is_empty() {
        // --- Typing Indicator Task ---
        let bot_clone = bot.clone();
        let typing_indicator_handle = tokio::spawn(async move {
            loop {
                if let Err(e) = bot_clone
                    .send_chat_action(chat_id, ChatAction::Typing)
                    .await
                {
                    log::warn!("Failed to send typing action: {}", e);
                    break;
                }
                sleep(Duration::from_secs(5)).await;
            }
        });

        let upload_result =
            upload_files_to_vector_store(user_id, bot_deps.clone(), file_paths.clone()).await;

        // Stop the typing indicator task
        typing_indicator_handle.abort();

        match upload_result {
            Ok(vector_store_id) => {
                let file_count = file_paths.len();
                let files_text = if file_count == 1 { "file" } else { "files" };

                // Show success message
                send_message(
                    msg.clone(),
                    bot.clone(),
                    format!(
                        "✅ {} {} uploaded and indexed! Your vector store ID: {}",
                        file_count, files_text, vector_store_id
                    ),
                )
                .await?;

                // Show updated user document library
                show_user_document_library(bot, chat_id, user_id, bot_deps).await?;
            }
            Err(e) => {
                send_message(msg, bot, format!("[Upload error]: {}", e).to_string()).await?;
            }
        }
    } else {
        send_message(msg, bot, "❌ No supported files found in your message. Please attach documents (.txt, .md, .py, .js, .pdf, .docx, etc.) that can be processed for semantic search.".to_string()).await?;
    }
    Ok(())
}

/// Display the user document library interface as a new message
pub async fn show_user_document_library(
    bot: Bot,
    chat_id: ChatId,
    user_id: i64,
    bot_deps: BotDependencies,
) -> AnyResult<()> {
    match list_user_files_with_names(user_id, bot_deps) {
        Ok(files) => {
            let (text, keyboard) = if files.is_empty() {
                let kb = InlineKeyboardMarkup::new(vec![
                    vec![InlineKeyboardButton::callback(
                        "📎 Upload Files",
                        "upload_files_prompt",
                    )],
                    vec![InlineKeyboardButton::callback(
                        "↩️ Back to User Settings",
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
                    let clear_all_button =
                        InlineKeyboardButton::callback("🗑️ Clear All Files", "clear_all_files");
                    keyboard_rows.push(vec![clear_all_button]);
                }
                // Upload + Back controls
                keyboard_rows.push(vec![InlineKeyboardButton::callback(
                    "📎 Upload Files",
                    "upload_files_prompt",
                )]);
                keyboard_rows.push(vec![InlineKeyboardButton::callback(
                    "↩️ Back to User Settings",
                    "back_to_user_settings",
                )]);
                (response, InlineKeyboardMarkup::new(keyboard_rows))
            };

            bot.send_message(chat_id, text)
                .parse_mode(ParseMode::Html)
                .reply_markup(keyboard)
                .await?;
        }
        Err(e) => {
            log::error!("Failed to show user document library: {}", e);
            bot.send_message(chat_id, "❌ Error loading Document Library")
                .await?;
        }
    }
    Ok(())
}

pub async fn handle_group_file_upload(
    bot: Bot,
    msg: Message,
    bot_deps: BotDependencies,
) -> AnyResult<()> {
    use tokio::fs::File;

    // Ensure this is a group chat
    if msg.chat.is_private() {
        send_message(
            msg,
            bot,
            "❌ Group document uploads can only be used in group chats.".to_string(),
        )
        .await?;
        return Ok(());
    }

    let group_id = msg.chat.id.to_string();

    // Check if user is admin
    let user = msg.from.as_ref();
    if user.is_none() {
        send_message(msg, bot, "❌ Unable to verify permissions.".to_string()).await?;
        return Ok(());
    }

    let user = user.unwrap();
    let is_admin = utils::is_admin(&bot, msg.chat.id, user.id).await;
    if !is_admin {
        send_message(
            msg,
            bot,
            "❌ Only group administrators can upload files to the group document library."
                .to_string(),
        )
        .await?;
        return Ok(());
    }

    let chat_id = msg.chat.id;
    let mut file_paths = Vec::new();

    // Handle document
    if let Some(document) = msg.document() {
        let file_id = &document.file.id;
        let file_info = bot.get_file(file_id.clone()).await?;
        let filename = document
            .file_name
            .clone()
            .unwrap_or_else(|| "document.bin".to_string());
        let file_path = format!("/tmp/group_{}_{}", group_id, filename);
        let mut file = File::create(&file_path)
            .await
            .map_err(|e| teloxide::RequestError::from(std::sync::Arc::new(e)))?;
        bot.download_file(&file_info.path, &mut file)
            .await
            .map_err(|e| teloxide::RequestError::from(e))?;
        file_paths.push(file_path);
    }

    // Only documents are supported for vector stores
    // Photos, videos, and audio cannot be processed for semantic search

    if !file_paths.is_empty() {
        // --- Typing Indicator Task ---
        let bot_clone = bot.clone();
        let typing_indicator_handle = tokio::spawn(async move {
            loop {
                if let Err(e) = bot_clone
                    .send_chat_action(chat_id, ChatAction::Typing)
                    .await
                {
                    log::warn!("Failed to send typing action: {}", e);
                    break;
                }
                sleep(Duration::from_secs(5)).await;
            }
        });

        let upload_result = upload_files_to_group_vector_store(
            group_id.clone(),
            bot_deps.clone(),
            file_paths.clone(),
        )
        .await;

        // Stop the typing indicator task
        typing_indicator_handle.abort();

        // Clear the awaiting state after upload attempt
        bot_deps
            .group_file_upload_state
            .clear_awaiting(group_id.clone())
            .await;

        match upload_result {
            Ok(vector_store_id) => {
                let file_count = file_paths.len();
                let files_text = if file_count == 1 { "file" } else { "files" };

                // Show success message
                send_message(
                    msg.clone(),
                    bot.clone(),
                    format!(
                        "✅ {} {} uploaded to group document library! Vector store ID: {}",
                        file_count, files_text, vector_store_id
                    ),
                )
                .await?;

                // Show updated group document library
                show_group_document_library(msg, bot, group_id, bot_deps).await?;
            }
            Err(e) => {
                send_message(msg, bot, format!("[Group upload error]: {}", e).to_string()).await?;
            }
        }
    } else {
        send_message(msg, bot, "❌ No supported files found in your message. Please attach documents (.txt, .md, .py, .js, .pdf, .docx, etc.) that can be processed for semantic search.".to_string()).await?;
    }
    Ok(())
}

/// Display the group document library interface as a new message
pub async fn show_group_document_library(
    msg: Message,
    bot: Bot,
    group_id: String,
    bot_deps: BotDependencies,
) -> AnyResult<()> {
    match list_group_files_with_names(group_id.clone(), bot_deps) {
        Ok(files) => {
            let (text, keyboard) = if files.is_empty() {
                let kb = InlineKeyboardMarkup::new(vec![
                    vec![InlineKeyboardButton::callback(
                        "📎 Upload Files",
                        "group_upload_files_prompt",
                    )],
                    vec![InlineKeyboardButton::callback(
                        "↩️ Back to Group Settings",
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
                    "↩️ Back to Group Settings",
                    "back_to_group_settings",
                )]);
                (response, InlineKeyboardMarkup::new(keyboard_rows))
            };

            send_markdown_message_with_keyboard(
                bot,
                msg,
                KeyboardMarkupType::InlineKeyboardType(keyboard),
                &text,
            )
            .await?;
        }
        Err(e) => {
            log::error!("Failed to show group document library: {}", e);
            send_message(
                msg,
                bot,
                "❌ Error loading Group Document Library".to_string(),
            )
            .await?;
        }
    }
    Ok(())
}
