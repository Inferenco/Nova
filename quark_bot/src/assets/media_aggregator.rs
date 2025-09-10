use crate::ai::handler::AI;
use crate::credentials::handler::Auth;
use crate::dependencies::BotDependencies;
use crate::user_model_preferences::handler::UserModelPreferences;
use dashmap::DashMap;
use open_ai_rust_responses_by_sshift::types::ReasoningParams;

use std::sync::Arc;
use std::time::Duration;
use teloxide::net::Download;
use teloxide::prelude::*;
use teloxide::types::ChatAction;

pub struct MediaGroupAggregator {
    // Key: media_group_id
    // Value: (Vec of messages in the group, debounce task handle)
    groups: DashMap<String, (Vec<Message>, tokio::task::JoinHandle<()>)>,
    bot: Bot,
    ai: AI,
    auth: Auth,
    user_model_prefs: UserModelPreferences,
}

impl MediaGroupAggregator {
    pub fn new(bot: Bot, ai: AI, auth: Auth, user_model_prefs: UserModelPreferences) -> Self {
        Self {
            groups: DashMap::new(),
            bot,
            ai,
            auth,
            user_model_prefs,
        }
    }

    pub async fn add_message(self: Arc<Self>, msg: Message, bot_deps: BotDependencies) {
        let media_group_id = if let Some(id) = msg.media_group_id() {
            id.to_string()
        } else {
            return;
        };

        let mut entry = self
            .groups
            .entry(media_group_id.clone())
            .or_insert_with(|| (Vec::new(), tokio::spawn(async {})));

        // A new message has arrived for the group, so cancel the previous debounce task.
        entry.value().1.abort();

        // Add the new message to the group.
        entry.value_mut().0.push(msg);

        // Clone the Arc to move it into the new task.
        let aggregator_clone = self.clone();

        // Start a new debounce task.
        let handle = tokio::spawn(async move {
            // Wait for a short period to see if more messages arrive for this group.
            tokio::time::sleep(Duration::from_millis(2000)).await;

            // The timer has elapsed, so we can now process the group.
            if let Some((_, (messages, _))) = aggregator_clone.groups.remove(&media_group_id) {
                aggregator_clone
                    .process_media_group(messages, bot_deps)
                    .await;
            }
        });

        // Store the new task's handle.
        entry.value_mut().1 = handle;
    }

    async fn process_media_group(&self, messages: Vec<Message>, bot_deps: BotDependencies) {
        if messages.is_empty() {
            return;
        }

        // Find the message with caption (the command)
        let command_msg = messages.iter().find(|msg| msg.caption().is_some());

        if let Some(cmd_msg) = command_msg {
            // Determine prompt & command type
            let text = cmd_msg.caption().unwrap_or("");
            let is_group_command = text.trim_start().starts_with("/g ");
            let is_private_command = text.trim_start().starts_with("/c ");

            if !is_private_command && !is_group_command {
                return;
            }

            let group_id = if is_group_command && !cmd_msg.chat.is_private() {
                Some(cmd_msg.chat.id.to_string())
            } else {
                None
            };

            // --- Start typing indicator ---
            let bot_clone = self.bot.clone();
            let chat_id = cmd_msg.chat.id;
            let typing_indicator_handle = tokio::spawn(async move {
                loop {
                    if let Err(e) = bot_clone
                        .send_chat_action(chat_id, ChatAction::Typing)
                        .await
                    {
                        log::warn!("Failed to send typing action: {}", e);
                        break;
                    }
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            });

            // --- Auth & prefs ---
            let user = if let Some(u) = cmd_msg.from.as_ref() {
                u
            } else {
                typing_indicator_handle.abort();
                return;
            };

            let username = if let Some(u) = &user.username {
                u
            } else {
                typing_indicator_handle.abort();
                if let Err(e) = self
                    .bot
                    .send_message(chat_id, "❌ Unable to verify permissions.")
                    .await
                {
                    log::warn!("Failed to send permission error: {}", e);
                }
                return;
            };

            if self.auth.get_credentials(username).is_none() {
                typing_indicator_handle.abort();
                if let Err(e) = self
                    .bot
                    .send_message(chat_id, "❌ Please login first.")
                    .await
                {
                    log::warn!("Failed to send login required message: {}", e);
                }
                return;
            }

            // Load model prefs and compute request params (unified)
            let prefs = self.user_model_prefs.get_preferences(username);
            let model = prefs.chat_model.to_openai_model();
            let reasoning_params: Option<ReasoningParams> = None;

            // --- Gather photos: take largest variant from each message ---
            let mut image_paths: Vec<(String, String)> = Vec::new();
            for m in &messages {
                if let Some(photos) = m.photo() {
                    if let Some(photo) = photos.last() {
                        let file_id = &photo.file.id;
                        match self.bot.get_file(file_id.clone()).await {
                            Ok(file_info) => {
                                let extension = file_info
                                    .path
                                    .split('.')
                                    .last()
                                    .unwrap_or("jpg")
                                    .to_string();
                                let tmp_path = format!(
                                    "/tmp/{}_{}.{}",
                                    user.id.0, photo.file.unique_id, extension
                                );
                                if let Ok(mut f) = tokio::fs::File::create(&tmp_path).await {
                                    if self
                                        .bot
                                        .download_file(&file_info.path, &mut f)
                                        .await
                                        .is_ok()
                                    {
                                        image_paths.push((tmp_path, extension));
                                    }
                                }
                            }
                            Err(e) => log::error!("Failed to fetch file info: {}", e),
                        }
                    }
                }
            }

            // Upload images to GCS
            let uploaded_urls = match self.ai.upload_user_images(image_paths).await {
                Ok(urls) => urls,
                Err(e) => {
                    typing_indicator_handle.abort();
                    if let Err(e2) = self
                        .bot
                        .send_message(chat_id, "Failed to upload images.")
                        .await
                    {
                        log::warn!("Failed to send upload error message: {}", e2);
                    }
                    log::error!("upload_user_images failed: {}", e);
                    return;
                }
            };

            // Generate response
            let response_result = self
                .ai
                .generate_response(
                    self.bot.clone(),
                    cmd_msg.clone(),
                    text,
                    None,
                    uploaded_urls,
                    model,
                    4000,
                    reasoning_params,
                    bot_deps.clone(),
                    group_id.clone(),
                )
                .await;

            typing_indicator_handle.abort();

            match response_result {
                Ok(ai_response) => {
                    if let Some(image_data) = ai_response.image_data {
                        let photo = teloxide::types::InputFile::memory(image_data);
                        // Remove <pre> blocks from caption to avoid unbalanced HTML when truncated
                        let re = regex::Regex::new(r"(?s)<pre[^>]*>(.*?)</pre>").unwrap();
                        let mut pre_blocks: Vec<String> = Vec::new();
                        let text_without_pre = re
                            .replace_all(&ai_response.text, |caps: &regex::Captures| {
                                pre_blocks.push(
                                    caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string(),
                                );
                                "".to_string()
                            })
                            .to_string();
                        let caption = if text_without_pre.len() > 1024 {
                            &text_without_pre[..1024]
                        } else {
                            &text_without_pre
                        };
                        if let Err(e) = self
                            .bot
                            .send_photo(chat_id, photo)
                            .caption(caption)
                            .parse_mode(teloxide::types::ParseMode::Html)
                            .await
                        {
                            log::warn!("Failed to send photo with caption: {}", e);
                        }
                        // Send any extracted <pre> blocks safely in full
                        for pre in pre_blocks {
                            let escaped = teloxide::utils::html::escape(&pre);
                            let msg = format!("<pre>{}</pre>", escaped);
                            if let Err(e) = self
                                .bot
                                .send_message(chat_id, msg)
                                .parse_mode(teloxide::types::ParseMode::Html)
                                .await
                            {
                                log::warn!("Failed to send pre block: {}", e);
                            }
                        }
                        if text_without_pre.len() > 1024 {
                            if let Err(e) = self
                                .bot
                                .send_message(chat_id, &text_without_pre[1024..])
                                .parse_mode(teloxide::types::ParseMode::Html)
                                .await
                            {
                                log::warn!("Failed to send overflow message: {}", e);
                            }
                        }
                    } else {
                        if let Err(e) = self
                            .bot
                            .send_message(chat_id, ai_response.text)
                            .parse_mode(teloxide::types::ParseMode::Html)
                            .await
                        {
                            log::warn!("Failed to send AI response: {}", e);
                        }
                    }
                }
                Err(e) => {
                    log::error!("AI generate_response failed: {}", e);
                    let _ = self
                        .bot
                        .send_message(chat_id, "Sorry, I couldn't process your request.")
                        .await;
                }
            }
        } else {
            log::warn!("Media group processed but no caption found with command");
        }
    }
}
