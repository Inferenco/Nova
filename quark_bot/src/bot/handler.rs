//! Command handlers for quark_bot Telegram bot.
use crate::{
    ai::{
        moderation::handler::handle_message_moderation, sentinel::handler::handle_message_sentinel,
    },
    assets::handler::{handle_file_upload, handle_group_file_upload},
    bot::hooks::{fund_account_hook, pay_users_hook, withdraw_funds_hook},
    credentials::dto::CredentialsPayload,
    dao::handler::handle_message_dao,
    dependencies::BotDependencies,
    filters::handler::{handle_message_filters, process_message_for_filters},
    group::dto::GroupCredentials,
    scheduled_payments::handler::handle_message_scheduled_payments,
    scheduled_prompts::handler::handle_message_scheduled_prompts,
    sponsor::handler::handle_sponsor_message,
    user_model_preferences::dto::ModelPreferences,
    utils::{
        self, KeyboardMarkupType, create_purchase_request, send_html_message,
        send_markdown_message_with_keyboard, send_message,
    },
    welcome::handler::handle_welcome_message,
};
use anyhow::Result as AnyResult;
use aptos_rust_sdk_types::api_types::view::ViewRequest;
use serde_json::value;

use crate::{
    ai::moderation::ModerationOverrides,
    user_model_preferences::handler::initialize_user_preferences,
};

use open_ai_rust_responses_by_sshift::Model;
use quark_core::helpers::{bot_commands::Command, dto::CreateGroupRequest};
use regex;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::env;
use std::time::Duration;
use teloxide::types::{
    ChatAction, InlineKeyboardButton, InlineKeyboardMarkup, InputFile, WebAppInfo,
};
use teloxide::types::{KeyboardMarkup, ParseMode};
use teloxide::{net::Download, utils::command::BotCommands};
use teloxide::{
    prelude::*,
    types::{ButtonRequest, KeyboardButton},
};
use tokio::fs::File;
use tokio::time::sleep;

const TELEGRAM_MESSAGE_LIMIT: usize = 4096;

/// Split a Telegram-HTML message into chunks without cutting inside tags/entities.
fn split_message(text: &str) -> Vec<String> {
    if text.len() <= TELEGRAM_MESSAGE_LIMIT {
        return vec![text.to_string()];
    }

    // Track whether a tag requires closing
    fn is_closing_required(tag: &str) -> bool {
        matches!(
            tag,
            "b" | "strong"
                | "i"
                | "em"
                | "u"
                | "ins"
                | "s"
                | "strike"
                | "del"
                | "code"
                | "pre"
                | "a"
                | "tg-spoiler"
                | "span"
                | "blockquote"
        )
    }

    let mut chunks: Vec<String> = Vec::new();
    let mut buf = String::new();
    let mut last_safe_break: Option<usize> = None; // index in buf safe to split
    let mut inside_tag = false;
    let mut inside_entity = false;
    let mut tag_buf = String::new();
    let mut open_stack: Vec<String> = Vec::new();
    let mut tag_start_in_buf: usize = 0; // start index of current tag
    let mut last_anchor_start: Option<usize> = None; // avoid splitting inside <a>

    let push_chunk = |buf: &mut String, chunks: &mut Vec<String>| {
        if !buf.trim().is_empty() {
            chunks.push(buf.trim().to_string());
        }
        buf.clear();
    };

    for ch in text.chars() {
        match ch {
            '<' => {
                inside_tag = true;
                tag_buf.clear();
                tag_start_in_buf = buf.len();
                buf.push(ch);
            }
            '>' => {
                buf.push(ch);
                if inside_tag {
                    // parse tag name
                    let tag_content = tag_buf.trim();
                    let is_end = tag_content.starts_with('/')
                        || tag_content.starts_with("/ ")
                        || tag_content.starts_with(" /");
                    let name = tag_content
                        .trim_start_matches('/')
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .to_lowercase();
                    if !name.is_empty() && is_closing_required(&name) {
                        if is_end {
                            if let Some(pos) = open_stack.iter().rposition(|t| t == &name) {
                                open_stack.remove(pos);
                            }
                            if name == "a" {
                                last_anchor_start = None;
                            }
                        } else {
                            open_stack.push(name.clone());
                            if name == "a" {
                                last_anchor_start = Some(tag_start_in_buf);
                            }
                        }
                    }
                }
                inside_tag = false;
                if !inside_entity && open_stack.is_empty() {
                    last_safe_break = Some(buf.len());
                }
            }
            '&' => {
                inside_entity = true;
                buf.push(ch);
            }
            ';' => {
                buf.push(ch);
                if inside_entity {
                    inside_entity = false;
                    if !inside_tag && open_stack.is_empty() {
                        last_safe_break = Some(buf.len());
                    }
                }
            }
            _ => {
                if inside_tag {
                    tag_buf.push(ch);
                }
                buf.push(ch);
                if (ch == ' ' || ch == '\n' || ch == '\t')
                    && !inside_tag
                    && !inside_entity
                    && open_stack.is_empty()
                {
                    last_safe_break = Some(buf.len());
                }
            }
        }

        if buf.len() >= TELEGRAM_MESSAGE_LIMIT {
            if let Some(idx) = last_safe_break {
                let remainder = buf.split_off(idx);
                let chunk = buf.trim().to_string();
                if !chunk.is_empty() {
                    chunks.push(chunk);
                }
                buf = remainder;
            } else if last_anchor_start.is_some() {
                // Split before the anchor started to avoid cutting inside <a>
                let pos = last_anchor_start.unwrap();
                if pos > 0 {
                    let remainder = buf.split_off(pos);
                    let chunk = buf.trim().to_string();
                    if !chunk.is_empty() {
                        chunks.push(chunk);
                    }
                    buf = remainder;
                } else {
                    // Anchor starts at 0; fall back to pushing the whole buffer to make progress
                    push_chunk(&mut buf, &mut chunks);
                }
            } else if open_stack.iter().any(|t| t == "pre" || t == "code") {
                // Close pre/code at boundary and reopen in next chunk
                let closable: Vec<&str> = open_stack
                    .iter()
                    .map(|s| s.as_str())
                    .filter(|t| *t == "pre" || *t == "code")
                    .collect();
                for t in closable.iter().rev() {
                    buf.push_str(&format!("</{}>", t));
                }
                let reopen = closable
                    .iter()
                    .map(|t| format!("<{}>", t))
                    .collect::<Vec<_>>()
                    .join("");
                let chunk = buf.trim().to_string();
                if !chunk.is_empty() {
                    chunks.push(chunk);
                }
                buf.clear();
                buf.push_str(&reopen);
            } else {
                // Last resort: push whatever we have (should be rare)
                push_chunk(&mut buf, &mut chunks);
            }
            last_safe_break = None;
        }
    }

    if !buf.trim().is_empty() {
        chunks.push(buf.trim().to_string());
    }

    chunks
}

/// Extract all <pre>...</pre> blocks and return the text without them, plus the list of pre contents
fn split_off_pre_blocks(text: &str) -> (String, Vec<String>) {
    let re = regex::Regex::new(r"(?s)<pre[^>]*>(.*?)</pre>").unwrap();
    let mut pre_blocks: Vec<String> = Vec::new();
    let without_pre = re
        .replace_all(text, |caps: &regex::Captures| {
            pre_blocks.push(caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string());
            "".to_string()
        })
        .to_string();
    (without_pre, pre_blocks)
}

/// Send a long <pre> block safely by chunking and wrapping each chunk in <pre> tags
async fn send_pre_block(bot: &Bot, chat_id: ChatId, title: &str, content: &str) -> AnyResult<()> {
    // Escape HTML special chars inside the <pre> block
    let escaped = teloxide::utils::html::escape(content);
    let prefix = format!("{}\n<pre>", title);
    let suffix = "</pre>";
    // Leave some headroom for prefix/suffix
    let max_payload = TELEGRAM_MESSAGE_LIMIT.saturating_sub(prefix.len() + suffix.len() + 16);
    let mut current = String::new();
    for ch in escaped.chars() {
        if current.chars().count() + 1 > max_payload {
            let msg = format!("{}{}{}", prefix, current, suffix);
            match bot
                .send_message(chat_id, msg)
                .parse_mode(ParseMode::Html)
                .await
            {
                Ok(_) => {}
                Err(e) => {
                    let err_text = e.to_string();
                    log::error!("Error sending <pre> chunk: {}", err_text);
                    if err_text.contains("can't parse entities")
                        || err_text.contains("Unsupported start tag")
                    {
                        // Fallback: send as plain text to preserve content
                        let plain = format!("{}\n{}", title, current);
                        let _ = bot.send_message(chat_id, plain).await;
                        return Ok(());
                    }
                    return Err(e.into());
                }
            }
            current.clear();
        }
        current.push(ch);
    }
    if !current.is_empty() {
        let msg = format!("{}{}{}", prefix, current, suffix);
        match bot
            .send_message(chat_id, msg)
            .parse_mode(ParseMode::Html)
            .await
        {
            Ok(_) => {}
            Err(e) => {
                let err_text = e.to_string();
                log::error!("Error sending final <pre> chunk: {}", err_text);
                if err_text.contains("can't parse entities")
                    || err_text.contains("Unsupported start tag")
                {
                    // Fallback: send remainder as plain text
                    let plain = format!("{}\n{}", title, current);
                    let _ = bot.send_message(chat_id, plain).await;
                    return Ok(());
                }
                return Err(e.into());
            }
        }
    }
    Ok(())
}

/// Send a potentially long message, splitting it into multiple messages if necessary
async fn send_long_message(msg: Message, bot: &Bot, text: &str) -> AnyResult<()> {
    // Convert markdown (including ``` code fences) to Telegram-compatible HTML
    let html_text = utils::markdown_to_html(text);
    // Normalize image anchor to point to the public GCS URL when present
    let html_text = utils::normalize_image_url_anchor(&html_text);
    // Sanitize to Telegram-safe HTML before splitting/sending
    let html_text = utils::sanitize_telegram_html(&html_text);
    let chunks = split_message(&html_text);

    for (i, chunk) in chunks.iter().enumerate() {
        if i > 0 {
            // Small delay between messages to avoid rate limiting
            sleep(Duration::from_millis(100)).await;
        }

        match send_html_message(msg.clone(), bot.clone(), chunk.to_string()).await {
            Ok(_) => {}
            Err(e) => {
                let err_text = e.to_string();
                log::error!("Error sending message chunk: {}", err_text);
                if err_text.contains("can't parse entities")
                    || err_text.contains("Unsupported start tag")
                {
                    // Fallback: send this chunk as plain text to preserve content
                    send_message(msg.clone(), bot.clone(), chunk.to_string()).await?;
                    return Ok(());
                }
                return Err(e.into());
            }
        }
    }

    Ok(())
}

pub async fn handle_aptos_connect(bot: Bot, msg: Message) -> AnyResult<()> {
    if !msg.chat.is_private() {
        send_message(
            msg.clone(),
            bot.clone(),
            "❌ This command can only be used in a private chat with the bot.".to_string(),
        )
        .await?;
    }

    let aptos_connect_url = "https://aptosconnect.app";

    let url = Url::parse(&aptos_connect_url).expect("Invalid URL");
    let web_app_info = WebAppInfo { url };

    let aptos_connect_button = InlineKeyboardButton::web_app("Open Aptos Connect", web_app_info);

    send_markdown_message_with_keyboard(
        bot,
        msg,
        KeyboardMarkupType::InlineKeyboardType(InlineKeyboardMarkup::new(vec![vec![
            aptos_connect_button,
        ]])),
        "Click the button below to login to your Nova account",
    )
    .await?;

    return Ok(());
}

pub async fn handle_login_user(bot: Bot, msg: Message) -> AnyResult<()> {
    let private_msg = msg.clone();
    if !private_msg.chat.is_private() {
        send_message(
            private_msg,
            bot,
            "❌ This command can only be used in a private chat with the bot.".to_string(),
        )
        .await?;
        return Ok(());
    }

    let user = msg.from.as_ref();

    if user.is_none() {
        send_message(
            msg.clone(),
            bot,
            "❌ Unable to verify permissions.".to_string(),
        )
        .await?;
        return Ok(());
    }

    let user_id = user.unwrap().id;

    let app_url = env::var("APP_URL").expect("APP_URL must be set");
    let url_to_build = format!("{}/login?userId={}", app_url, user_id);

    let url = Url::parse(&url_to_build).expect("Invalid URL");

    let web_app_info = WebAppInfo { url };

    let request = ButtonRequest::WebApp(web_app_info);

    let login_button = KeyboardButton::new("Login to your Nova account");

    let login_button = login_button.request(request);

    let login_markup = KeyboardMarkup::new(vec![vec![login_button]]);

    send_markdown_message_with_keyboard(
        bot,
        msg,
        KeyboardMarkupType::KeyboardType(login_markup),
        "Click the button below to login to your Nova account",
    )
    .await?;

    return Ok(());
}

pub async fn handle_login_group(
    bot: Bot,
    msg: Message,
    bot_deps: BotDependencies,
) -> AnyResult<()> {
    // Ensure this command is used in a group chat
    if msg.chat.is_private() {
        send_message(
            msg.clone(),
            bot,
            "❌ This command must be used in a group chat.".to_string(),
        )
        .await?;
        return Ok(());
    }

    let account_seed = bot_deps.group.account_seed.clone();

    // Allow only group administrators to invoke
    let admins = bot.get_chat_administrators(msg.chat.id).await?;
    let requester_id = msg.from.as_ref().map(|u| u.id);
    let group_id = msg.chat.id;

    let group_id_formatted = format!("{}-{}", msg.chat.id, account_seed);

    let payload: GroupCredentials;

    if let Some(uid) = requester_id {
        let is_admin = admins.iter().any(|member| member.user.id == uid);
        if !is_admin {
            send_message(
                msg.clone(),
                bot,
                "❌ Only group administrators can use this command.".to_string(),
            )
            .await?;
            return Ok(());
        }
    } else {
        // Cannot identify sender; deny action
        send_message(
            msg.clone(),
            bot,
            "❌ Unable to verify permissions.".to_string(),
        )
        .await?;
        return Ok(());
    }

    let group_exists = bot_deps
        .group
        .group_exists(group_id, bot_deps.panora.clone())
        .await;

    if !group_exists {
        let group_result = bot_deps
            .service
            .create_group(CreateGroupRequest {
                group_id: group_id_formatted.clone(),
            })
            .await;

        if group_result.is_err() {
            send_message(msg, bot, "❌ Unable to create group.".to_string()).await?;
            return Ok(());
        }
    }

    let jwt = bot_deps.group.generate_new_jwt(group_id);

    if !jwt {
        send_message(msg, bot, "❌ Unable to generate JWT.".to_string()).await?;
        return Ok(());
    }

    let payload_response = bot_deps.group.get_credentials(group_id);

    if payload_response.is_none() {
        send_message(msg, bot, "❌ Unable to get credentials.".to_string()).await?;
        return Ok(());
    }

    payload = payload_response.unwrap();

    let updated_credentials =
        check_group_resource_account_address(&bot, payload, msg.clone(), &bot_deps).await;

    if updated_credentials.is_err() {
        send_message(msg, bot, "❌ Unable to save credentials.".to_string()).await?;
        return Ok(());
    }

    send_html_message(msg, bot, format!("🔑 <b>Group Login Successful!</b>\n\n<i>You can now use the group's Nova account to interact with the bot.</i>\n\n💡 <i>Use /groupwalletaddress to get the group's wallet address and /groupbalance to get the group's balance of a token.</i>").to_string()).await?;
    Ok(())
}

pub async fn handle_help(bot: Bot, msg: Message) -> AnyResult<()> {
    send_message(msg, bot, Command::descriptions().to_string()).await?;
    Ok(())
}

pub async fn handle_prices(bot: Bot, msg: Message) -> AnyResult<()> {
    let pricing_info = crate::ai::actions::execute_prices(&serde_json::json!({})).await;
    send_html_message(msg, bot, pricing_info).await?;
    Ok(())
}

pub async fn handle_chat(
    bot: Bot,
    msg: Message,
    prompt: String,
    group_id: Option<String>,
    is_sponsor: bool,
    bot_deps: BotDependencies,
) -> AnyResult<()> {
    // Store group_id for later use to avoid move issues
    let group_id_for_hook = group_id.clone();

    // --- Start Typing Indicator Immediately ---
    let bot_clone = bot.clone();
    let profile = env::var("PROFILE").unwrap_or("prod".to_string());
    let typing_indicator_handle = tokio::spawn(async move {
        loop {
            let typing = bot_clone.send_chat_action(msg.chat.id, ChatAction::Typing);

            let type_result = if let Some(thread_id) = msg.thread_id {
                typing.message_thread_id(thread_id).await
            } else {
                typing.await
            };

            if type_result.is_err() {
                log::warn!(
                    "Failed to send typing action: {}",
                    type_result.err().unwrap()
                );
                break;
            }
            sleep(Duration::from_secs(5)).await;
        }
    });

    let user = msg.from.as_ref();

    if user.is_none() {
        typing_indicator_handle.abort();
        send_message(msg, bot, "❌ Unable to verify permissions.".to_string()).await?;
        return Ok(());
    }

    let user_id = user.unwrap().id.to_string();
    let username = user.unwrap().username.as_ref();

    if username.is_none() {
        typing_indicator_handle.abort();
        send_message(msg, bot, "❌ Unable to verify permissions.".to_string()).await?;
        return Ok(());
    }

    let username = username.unwrap();

    let credentials = bot_deps.auth.get_credentials(&username);
    if credentials.is_none() && !is_sponsor {
        typing_indicator_handle.abort();
        send_message(msg, bot, "❌ Unable to verify permissions.".to_string()).await?;
        return Ok(());
    }

    let group_credentials = bot_deps.group.get_credentials(msg.chat.id);

    // Load user's chat model preferences
    let preferences = if group_id.is_some() {
        ModelPreferences::default()
    } else {
        bot_deps.user_model_prefs.get_preferences(username)
    };

    let chat_model = preferences.chat_model.to_openai_model();

    let _temperature: Option<f32> = None;

    // --- Vision Support: Check for replied-to images ---
    let mut image_url_from_reply: Option<String> = None;
    // --- Context Support: Check for replied-to message text ---
    let mut replied_message_context: Option<String> = None;
    // --- Image Support: Process replied message images ---
    let mut replied_message_image_paths: Vec<(String, String)> = Vec::new();
    if let Some(reply) = msg.reply_to_message() {
        // Extract text content from replied message (following /mod pattern)
        let reply_text_content = reply.text().or_else(|| reply.caption()).unwrap_or_default();

        if !reply_text_content.is_empty() {
            if let Some(from) = reply.from.as_ref() {
                let username = from
                    .username
                    .as_ref()
                    .map(|u| format!("@{}", u))
                    .unwrap_or_else(|| from.first_name.clone());
                replied_message_context =
                    Some(format!("User {} said: {}", username, reply_text_content));
            } else {
                replied_message_context = Some(format!("Previous message: {}", reply_text_content));
            }
        }

        // Process images from replied message – only take the largest resolution (last PhotoSize)
        if let Some(photos) = reply.photo() {
            if let Some(photo) = photos.last() {
                let file_id = &photo.file.id;
                let file_info = bot.get_file(file_id.clone()).await?;
                let extension = file_info
                    .path
                    .split('.')
                    .last()
                    .unwrap_or("jpg")
                    .to_string();
                let temp_path = format!(
                    "/tmp/reply_{}_{}.{}",
                    user_id, photo.file.unique_id, extension
                );
                let mut file = File::create(&temp_path)
                    .await
                    .map_err(|e| teloxide::RequestError::from(std::sync::Arc::new(e)))?;
                bot.download_file(&file_info.path, &mut file)
                    .await
                    .map_err(|e| teloxide::RequestError::from(e))?;
                replied_message_image_paths.push((temp_path, extension));
            }
        }

        if let Some(from) = reply.from.as_ref() {
            if from.is_bot {
                let reply_text = reply.text().or_else(|| reply.caption());
                if let Some(text) = reply_text {
                    // A simple regex to find the GCS URL
                    if let Ok(re) = regex::Regex::new(
                        r"https://storage\.googleapis\.com/sshift-gpt-bucket/[^\s]+",
                    ) {
                        if let Some(mat) = re.find(text) {
                            image_url_from_reply = Some(mat.as_str().to_string());
                        }
                    }
                }
            }
        }
    }

    // --- Download user-attached images ---
    let mut user_uploaded_image_paths: Vec<(String, String)> = Vec::new();
    if let Some(photos) = msg.photo() {
        // Telegram orders PhotoSize from smallest to largest; take the last (largest)
        if let Some(photo) = photos.last() {
            let file_id = &photo.file.id;
            let file_info = bot.get_file(file_id.clone()).await?;
            let extension = file_info
                .path
                .split('.')
                .last()
                .unwrap_or("jpg")
                .to_string();
            let temp_path = format!("/tmp/{}_{}.{}", user_id, photo.file.unique_id, extension);
            let mut file = File::create(&temp_path)
                .await
                .map_err(|e| teloxide::RequestError::from(std::sync::Arc::new(e)))?;
            bot.download_file(&file_info.path, &mut file)
                .await
                .map_err(|e| teloxide::RequestError::from(e))?;
            user_uploaded_image_paths.push((temp_path, extension));
        }
    }

    // --- Upload replied message images to GCS ---
    let replied_message_image_urls = if !replied_message_image_paths.is_empty() {
        match bot_deps
            .ai
            .upload_user_images(replied_message_image_paths)
            .await
        {
            Ok(urls) => urls,
            Err(e) => {
                log::error!("Failed to upload replied message images: {}", e);
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    // --- Upload user images to GCS ---
    let user_uploaded_image_urls = match bot_deps
        .ai
        .upload_user_images(user_uploaded_image_paths)
        .await
    {
        Ok(urls) => urls,
        Err(e) => {
            log::error!("Failed to upload user images: {}", e);
            typing_indicator_handle.abort();
            send_message(
                msg,
                bot,
                "Sorry, I couldn't upload your image. Please try again.".to_string(),
            )
            .await?;
            return Ok(());
        }
    };

    // --- Combine all image URLs ---
    let mut all_image_urls = user_uploaded_image_urls;
    all_image_urls.extend(replied_message_image_urls);

    // Prepare the final prompt with context if available
    let final_prompt = if let Some(context) = replied_message_context {
        format!("{}\n\nUser asks: {}", context, prompt)
    } else {
        prompt
    };

    // Asynchronously generate the response
    let response_result = bot_deps
        .ai
        .generate_response(
            bot.clone(),
            msg.clone(),
            &final_prompt,
            image_url_from_reply,
            all_image_urls,
            chat_model,
            4000,
            None,
            bot_deps.clone(),
            group_id.clone(),
        )
        .await;

    typing_indicator_handle.abort();

    match response_result {
        Ok(ai_response) => {
            let (web_search, file_search, image_gen, _) = ai_response.get_tool_usage_counts();

            let jwt = if group_id.is_some() {
                let group_credentials = group_credentials;

                if group_credentials.is_some() {
                    group_credentials.unwrap().jwt
                } else {
                    credentials.unwrap().jwt
                }
            } else {
                credentials.unwrap().jwt
            };

            if profile != "dev" {
                let response = create_purchase_request(
                    file_search,
                    web_search,
                    image_gen,
                    ai_response.total_tokens,
                    ai_response.model,
                    &jwt,
                    group_id,
                    Some(user_id),
                    bot_deps.clone(),
                )
                .await;

                if response.is_err() {
                    log::error!(
                        "Error purchasing tokens: {}",
                        response.as_ref().err().unwrap()
                    );

                    if response.as_ref().err().unwrap().to_string().contains("401")
                        || response.as_ref().err().unwrap().to_string().contains("403")
                    {
                        send_message(
                            msg,
                            bot,
                            "Your login has expired. Please login again.".to_string(),
                        )
                        .await?;
                    } else {
                        send_message(
                            msg,
                            bot,
                            "Sorry, I encountered an error while processing your chat request."
                                .to_string(),
                        )
                        .await?;
                    }

                    return Ok(());
                }
            }

            if let Some(image_data) = ai_response.image_data {
                let photo = InputFile::memory(image_data);
                // Strip <pre> blocks from caption to avoid unbalanced HTML when truncated
                let (text_without_pre, pre_blocks) = split_off_pre_blocks(&ai_response.text);
                let caption = if text_without_pre.len() > 1024 {
                    &text_without_pre[..1024]
                } else {
                    &text_without_pre
                };
                let caption = utils::sanitize_telegram_html(caption);
                bot.send_photo(msg.chat.id, photo)
                    .caption(caption)
                    .parse_mode(ParseMode::Html)
                    .await?;
                // Send any extracted <pre> blocks safely in full
                for pre in pre_blocks {
                    send_pre_block(&bot, msg.chat.id, "", &pre).await?;
                }
                // If the text_without_pre is longer than 1024, send the remainder
                if text_without_pre.len() > 1024 {
                    send_long_message(msg, &bot, &text_without_pre[1024..]).await?;
                }
            } else if let Some(ref tool_calls) = ai_response.tool_calls {
                if tool_calls
                    .iter()
                    .any(|tool_call| tool_call.name == "withdraw_funds")
                {
                    withdraw_funds_hook(bot, msg, ai_response.text).await?;
                } else if tool_calls
                    .iter()
                    .any(|tool_call| tool_call.name == "fund_account")
                {
                    fund_account_hook(bot, msg, ai_response.text).await?;
                } else if tool_calls
                    .iter()
                    .any(|tool_call| tool_call.name == "get_pay_users")
                {
                    // Get transaction_id from the pending transaction
                    let user_id = if let Some(user) = &msg.from {
                        user.id.0 as i64
                    } else {
                        log::warn!("Unable to get user ID for pay_users_hook");
                        send_long_message(msg.clone(), &bot, &ai_response.text).await?;
                        return Ok(());
                    };

                    let group_id_i64 = group_id_for_hook
                        .as_ref()
                        .and_then(|gid| gid.parse::<i64>().ok());

                    if let Some(pending_transaction) = bot_deps
                        .pending_transactions
                        .get_pending_transaction(user_id, group_id_i64)
                    {
                        pay_users_hook(
                            bot,
                            msg,
                            ai_response.text,
                            group_id_for_hook,
                            pending_transaction.transaction_id,
                            bot_deps.clone(),
                        )
                        .await?;
                    } else {
                        log::warn!(
                            "No pending transaction found for user {} in group {:?}",
                            user_id,
                            group_id_i64
                        );
                        send_long_message(msg.clone(), &bot, &ai_response.text).await?;
                    }
                } else {
                    send_long_message(msg.clone(), &bot, &ai_response.text).await?;
                }
            } else {
                send_long_message(msg, &bot, &ai_response.text).await?;
            }

            // Log tool calls if any
            if let Some(tool_calls) = &ai_response.tool_calls {
                if !tool_calls.is_empty() {
                    log::info!("Tool calls executed: {:?}", tool_calls);
                }
            }
        }
        Err(e) => {
            send_html_message(
                msg,
                bot,
                format!("An error occurred while processing your request: {}", e).to_string(),
            )
            .await?;
        }
    }

    Ok(())
}

pub async fn handle_new_chat(bot: Bot, msg: Message, bot_deps: BotDependencies) -> AnyResult<()> {
    let user_id = msg.from.as_ref().map(|u| u.id.0).unwrap_or(0) as i64;
    let user_id_str = user_id.to_string();

    // Determine if this is a group chat
    let group_id = if msg.chat.is_group() || msg.chat.is_supergroup() {
        Some(msg.chat.id.to_string())
    } else {
        None
    };

    // Clear conversation thread
    let convos_result = bot_deps.user_convos.clear_response_id(user_id);

    // Clear stored conversation summary
    let summary_result = bot_deps.summarizer.clear_summary(&user_id_str, group_id);

    match (convos_result, summary_result) {
        (Ok(_), Ok(_)) => {
            send_html_message(msg, bot, "🆕 <b>New conversation started!</b>\n\n✨ Your previous chat history has been cleared. Your next /chat command will start a fresh conversation thread.\n\n💡 <i>Your uploaded files and settings remain intact</i>".to_string()).await?;
        }
        (Ok(_), Err(e)) => {
            log::warn!("Failed to clear summary for user {}: {}", user_id, e);
            send_html_message(msg, bot, "🆕 <b>New conversation started!</b>\n\n✨ Your previous chat history has been cleared. Your next /chat command will start a fresh conversation thread.\n\n⚠️ <i>Note: Some conversation context may still be present</i>".to_string()).await?;
        }
        (Err(e), _) => {
            send_html_message(
                msg,
                bot,
                format!(
                    "❌ <b>Error starting new chat</b>\n\n<i>Technical details:</i> {}",
                    e
                )
                .to_string(),
            )
            .await?;
        }
    }
    Ok(())
}

pub async fn handle_web_app_data(
    bot: Bot,
    msg: Message,
    bot_deps: BotDependencies,
) -> AnyResult<()> {
    let web_app_data = msg.web_app_data().unwrap();
    let payload = web_app_data.data.clone();

    let payload = serde_json::from_str::<CredentialsPayload>(&payload);

    if payload.is_err() {
        send_message(msg, bot, "❌ Error parsing payload".to_string()).await?;
        return Ok(());
    };

    let payload = payload.unwrap();

    let user = msg.from.clone();

    if user.is_none() {
        send_message(msg, bot, "❌ User not found".to_string()).await?;
        return Ok(());
    }

    let user = user.unwrap();

    let username = user.username;

    if username.is_none() {
        send_message(
            msg,
            bot,
            "❌ Username not found, required for login".to_string(),
        )
        .await?;
        return Ok(());
    }

    let username = username.unwrap();

    let user_id = user.id;

    bot_deps
        .auth
        .generate_new_jwt(
            username.clone(),
            user_id,
            payload.account_address,
            payload.resource_account_address,
        )
        .await;

    // Initialize default model preferences for new user
    let _ = initialize_user_preferences(&username, &bot_deps.user_model_prefs).await;

    return Ok(());
}

pub async fn handle_message(bot: Bot, msg: Message, bot_deps: BotDependencies) -> AnyResult<()> {
    // Sentinel: moderate every message in group if sentinel is on
    if !msg.chat.is_private() {
        let group_id = msg.chat.id.to_string();
        let chat_id = msg.chat.id;
        let user = msg.from.clone();

        let sentinel_executed =
            handle_message_sentinel(bot.clone(), msg.clone(), bot_deps.clone(), group_id.clone())
                .await?;

        if user.is_none() {
            return Ok(());
        }

        let user_id = user.as_ref().unwrap().id.to_string();

        let username = user.as_ref().unwrap().username.clone();

        let group_credentials = bot_deps.group.get_credentials(msg.chat.id);

        if username.is_none() {
            log::error!("Username not found");
            return Ok(());
        }

        if group_credentials.is_none() {
            log::error!("Group credentials not found");

            send_message(
                msg,
                bot,
                "❌ Group not found, please login again".to_string(),
            )
            .await?;
            return Ok(());
        }

        let group_credentials = group_credentials.unwrap();

        let username = username.unwrap();

        if !group_credentials.users.contains(&username) {
            bot_deps
                .group
                .add_user_to_group(msg.chat.id, username)
                .await?;
        }

        // Try to find the pending token input with the formatted group ID
        let formatted_group_id = format!("{}-{}", group_id, bot_deps.group.account_seed);

        let dao_executed = handle_message_dao(
            bot.clone(),
            msg.clone(),
            bot_deps.clone(),
            user_id.clone(),
            formatted_group_id,
        )
        .await?;

        if dao_executed {
            return Ok(());
        }

        let welcome_executed = handle_welcome_message(
            bot.clone(),
            bot_deps.clone(),
            &msg,
            user_id.clone(),
            group_id.clone(),
        )
        .await?;

        if welcome_executed {
            return Ok(());
        }

        let sponsor_executed = handle_sponsor_message(
            &bot,
            &msg,
            &bot_deps,
            group_id.clone(),
            user.clone().unwrap().id,
            msg.chat.id,
        )
        .await?;

        if sponsor_executed {
            return Ok(());
        }

        let moderation_executed =
            handle_message_moderation(&bot, &msg, &bot_deps, chat_id.to_string()).await?;

        if moderation_executed {
            return Ok(());
        }

        let scheduled_payments_executed = handle_message_scheduled_payments(
            bot.clone(),
            msg.clone(),
            bot_deps.clone(),
            user.clone().unwrap(),
        )
        .await?;

        if scheduled_payments_executed {
            return Ok(());
        }

        let scheduled_prompts_executed = handle_message_scheduled_prompts(
            bot.clone(),
            msg.clone(),
            bot_deps.clone(),
            user.clone().unwrap(),
        )
        .await?;

        if scheduled_prompts_executed {
            return Ok(());
        }

        let filters_executed =
            handle_message_filters(&bot, msg.clone(), bot_deps.clone(), user.unwrap()).await?;

        if filters_executed {
            return Ok(());
        }

        // Process message against existing filters
        let filter_matches_processed =
            process_message_for_filters(bot.clone(), msg.clone(), bot_deps.clone()).await?;

        if filter_matches_processed {
            return Ok(());
        }

        if sentinel_executed {
            return Ok(());
        }
    }

    if msg.media_group_id().is_some() && msg.photo().is_some() {
        let media_aggregator = bot_deps.media_aggregator.clone();
        media_aggregator.add_message(msg, bot_deps.clone()).await;
        return Ok(());
    }

    // Photo-only message (no text/caption) may belong to a pending command
    if msg.text().is_none() && msg.caption().is_none() && msg.photo().is_some() {
        let cmd_collector = bot_deps.cmd_collector.clone();
        cmd_collector
            .try_attach_photo(msg, bot_deps.clone(), None)
            .await;
        return Ok(());
    }

    // Handle group file uploads when group is awaiting files (documents only)
    if !msg.chat.is_private()
        && msg.caption().is_none()
        && msg.document().is_some()
        && bot_deps
            .group_file_upload_state
            .is_awaiting(msg.chat.id.to_string())
            .await
    {
        handle_group_file_upload(bot, msg, bot_deps.clone()).await?;
        return Ok(());
    }

    // Handle private user file uploads (documents only)
    if msg.caption().is_none() && msg.chat.is_private() && msg.document().is_some() {
        handle_file_upload(bot, msg, bot_deps.clone()).await?;
    }
    Ok(())
}

// removed: handle_sentinel — sentinel toggling is available in Group Settings → Moderation

// removed: handle_moderation_settings — wizard now launched via /groupsettings Moderation menu

pub async fn handle_wallet_address(
    bot: Bot,
    msg: Message,
    bot_deps: BotDependencies,
) -> AnyResult<()> {
    println!("handle_wallet_address");
    let user = msg.from.clone();

    if user.is_none() {
        send_message(msg, bot, "❌ User not found".to_string()).await?;
        return Ok(());
    }

    let user = user.unwrap();

    let username = user.username;

    if username.is_none() {
        send_message(msg, bot, "❌ Username not found".to_string()).await?;
        return Ok(());
    }

    let username = username.unwrap();

    let user_credentials = bot_deps.auth.get_credentials(&username);

    if user_credentials.is_none() {
        send_message(msg, bot, "❌ User not found".to_string()).await?;
        return Ok(());
    }

    let user_credentials = user_credentials.unwrap();

    let wallet_address = user_credentials.resource_account_address;

    send_html_message(
        msg,
        bot,
        format!(
            "💰 <b>Your Wallet Address</b>\n\n<code>{}</code>",
            wallet_address
        )
        .to_string(),
    )
    .await?;

    Ok(())
}

pub async fn handle_mod(bot: Bot, msg: Message, bot_deps: BotDependencies) -> AnyResult<()> {
    // Check if sentinel is on for this chat
    if !msg.chat.is_private() {
        let sentinel_on = bot_deps.sentinel.get_sentinel(msg.chat.id.to_string());

        if sentinel_on {
            send_html_message(msg, bot, "🛡️ <b>Sentinel Mode Active</b>\n\n/report is disabled while sentinel is ON. All messages are being automatically moderated.".to_string()).await?;
            return Ok(());
        }
    }

    let group_credentials = bot_deps.group.get_credentials(msg.chat.id.clone());

    if group_credentials.is_none() {
        send_message(msg, bot, "❌ Group not found".to_string()).await?;
        return Ok(());
    }

    // Check if the command is used in reply to a message
    if let Some(reply_to_msg) = msg.clone().reply_to_message() {
        let user = reply_to_msg.from.clone();

        if user.is_none() {
            send_message(msg, bot, "❌ User not found".to_string()).await?;
            return Ok(());
        }

        // Extract text from the replied message
        let message_text = reply_to_msg
            .text()
            .or_else(|| reply_to_msg.caption())
            .unwrap_or_default();

        if message_text.is_empty() {
            send_html_message(msg, bot, format!("⚠️ <b>No Text Found</b>\n\n📝 Message ID: <code>{}</code>\n\n❌ The replied message contains no text to moderate.", reply_to_msg.id).to_string()).await?;
            return Ok(());
        }

        // Create moderation service using environment API key
        let moderation_service = bot_deps.moderation.clone();

        // Moderate the message
        // Load overrides
        let formatted_group_id = format!("{}-{}", msg.chat.id.0, bot_deps.group.account_seed);
        let settings_tree = bot_deps.db.open_tree("moderation_settings").unwrap();
        let overrides = if let Ok(Some(raw)) = settings_tree.get(formatted_group_id.as_bytes()) {
            #[derive(Serialize, Deserialize)]
            struct ModerationSettings {
                allowed_items: Vec<String>,
                disallowed_items: Vec<String>,
                updated_by_user_id: i64,
                updated_at_unix_ms: i64,
            }
            if let Ok(ms) = serde_json::from_slice::<ModerationSettings>(&raw) {
                Some(ModerationOverrides {
                    allowed_items: ms.allowed_items,
                    disallowed_items: ms.disallowed_items,
                })
            } else {
                None
            }
        } else {
            None
        };
        match moderation_service
            .moderate_message(message_text, &bot, &msg, &reply_to_msg, overrides)
            .await
        {
            Ok(result) => {
                log::info!(
                    "Manual moderation result: {} for message: {} (tokens: {})",
                    result.verdict,
                    message_text,
                    result.total_tokens
                );

                let purchase_result = create_purchase_request(
                    0,
                    0,
                    0,
                    result.total_tokens,
                    Model::GPT5Nano,
                    &group_credentials.unwrap().jwt,
                    Some(msg.chat.id.0.to_string()),
                    None,
                    bot_deps,
                )
                .await;

                if purchase_result.is_err() {
                    log::error!(
                        "Failed to purchase ai for flagged content: {}",
                        purchase_result.err().unwrap()
                    );
                    return Ok(());
                }

                // Only respond if the message is flagged
                if result.verdict == "F" {
                    // First, mute the user who sent the flagged message
                    if let Some(flagged_user) = &reply_to_msg.from {
                        // Create restricted permissions (muted)
                        let restricted_permissions = teloxide::types::ChatPermissions::empty();

                        // Mute the user indefinitely
                        if let Err(mute_error) = bot
                            .restrict_chat_member(
                                msg.chat.id,
                                flagged_user.id,
                                restricted_permissions,
                            )
                            .await
                        {
                            log::error!("Failed to mute user {}: {}", flagged_user.id, mute_error);
                        } else {
                            log::info!(
                                "Successfully muted user {} for flagged content",
                                flagged_user.id
                            );
                        }

                        // Create keyboard with admin controls
                        let keyboard = InlineKeyboardMarkup::new(vec![vec![
                            InlineKeyboardButton::callback(
                                "🔇 Unmute",
                                format!("unmute:{}", flagged_user.id),
                            ),
                            InlineKeyboardButton::callback(
                                "🚫 Ban",
                                format!("ban:{}:{}", flagged_user.id, reply_to_msg.id.0),
                            ),
                        ]]);

                        // Build a visible user mention (prefer @username, else clickable name)
                        let user_mention = if let Some(username) = &flagged_user.username {
                            format!("@{}", username)
                        } else {
                            let name = teloxide::utils::html::escape(&flagged_user.first_name);
                            format!(
                                "<a href=\"tg://user?id={}\">{}</a>",
                                flagged_user.id.0, name
                            )
                        };

                        // Send the flagged message response
                        send_markdown_message_with_keyboard(
                            bot.clone(),
                            msg.clone(),
                            KeyboardMarkupType::InlineKeyboardType(keyboard),
                            &format!(
                                "🛡️ <b>Content Flagged & User Muted</b>\n\n📝 Message ID: <code>{}</code>\n\n❌ Status: <b>FLAGGED</b> 🔴\n🔇 User has been muted\n👤 <b>User:</b> {}\n\n💬 <i>Flagged message:</i>\n<blockquote><span class=\"tg-spoiler\">{}</span></blockquote>",
                                reply_to_msg.id,
                                user_mention,
                                teloxide::utils::html::escape(message_text)
                            ),
                        ).await?;
                        // Immediately remove the offending message from the chat
                        if let Err(e) = bot.delete_message(msg.chat.id, reply_to_msg.id).await {
                            log::warn!(
                                "Failed to delete offending replied message {}: {}",
                                reply_to_msg.id.0,
                                e
                            );
                        }
                    } else {
                        // Fallback if no user found in the replied message
                        send_html_message(msg.clone(), bot.clone(), format!("🛡️ <b>Content Flagged</b>\n\n📝 Message ID: <code>{}</code>\n\n❌ Status: <b>FLAGGED</b> 🔴\n⚠️ Could not identify user to mute\n\n💬 <i>Flagged message:</i>\n<blockquote><span class=\"tg-spoiler\">{}</span></blockquote>", reply_to_msg.id, teloxide::utils::html::escape(message_text)).to_string()).await?;
                        // Remove the offending message regardless
                        if let Err(e) = bot.delete_message(msg.chat.id, reply_to_msg.id).await {
                            log::warn!(
                                "Failed to delete offending replied message {}: {}",
                                reply_to_msg.id.0,
                                e
                            );
                        }
                    }
                }
                // Silent when passed (P) - no response
            }
            Err(e) => {
                log::error!("Moderation failed: {}", e);
                send_html_message(msg, bot, format!("🛡️ <b>Moderation Error</b>\n\n📝 Message ID: <code>{}</code>\n\n❌ <b>Error:</b> Failed to analyze message. Please try again later.\n\n🔧 <i>Technical details:</i> {}", reply_to_msg.id, e).to_string()).await?;
            }
        }
    } else {
        // Not a reply to a message, show usage instructions
        send_html_message(msg, bot, "❌ <b>Invalid Usage</b>\n\n📝 The <code>/report</code> command must be used in reply to a message.\n\n💡 <b>How to use:</b>\n1. Find the message you want to moderate\n2. Reply to that message with <code>/report</code>\n\n🛡️ This will analyze the content of the replied message for violations.".to_string()).await?;
    }
    Ok(())
}

pub async fn handle_balance(
    bot: Bot,
    msg: Message,
    symbol: &str,
    bot_deps: BotDependencies,
) -> AnyResult<()> {
    let user = msg.from.clone();

    if user.is_none() {
        send_message(msg, bot, "❌ User not found".to_string()).await?;
        return Ok(());
    }

    let user = user.unwrap();

    let username = user.username;

    if username.is_none() {
        log::error!("❌ Username not found");
        send_message(msg, bot, "❌ Username not found".to_string()).await?;
        return Ok(());
    }

    let username = username.unwrap();

    let user_credentials = bot_deps.auth.get_credentials(&username);

    if user_credentials.is_none() {
        log::error!("❌ User not found");
        send_message(msg, bot, "❌ User not found".to_string()).await?;
        return Ok(());
    }

    let (token_type, decimals, token_symbol) =
        if symbol.to_lowercase() == "apt" || symbol.to_lowercase() == "aptos" {
            (
                "0x1::aptos_coin::AptosCoin".to_string(),
                8u8,
                "APT".to_string(),
            )
        } else {
            let token = bot_deps.panora.get_token_by_symbol(symbol).await;

            if token.is_err() {
                log::error!("❌ Error getting token: {}", token.as_ref().err().unwrap());
                send_message(msg, bot, "❌ Error getting token".to_string()).await?;
                return Ok(());
            }

            let token = token.unwrap();

            let token_type = if token.token_address.as_ref().is_some() {
                token.token_address.as_ref().unwrap().to_string()
            } else {
                token.fa_address.clone()
            };

            (token_type, token.decimals, token.symbol.clone())
        };

    let user_credentials = user_credentials.unwrap();

    let balance = bot_deps
        .panora
        .aptos
        .node
        .get_account_balance(
            user_credentials.resource_account_address,
            token_type.to_string(),
        )
        .await;

    if balance.is_err() {
        log::error!(
            "❌ Error getting balance: {}",
            balance.as_ref().err().unwrap()
        );
        send_message(msg, bot, "❌ Error getting balance".to_string()).await?;
        return Ok(());
    }

    let raw_balance = balance.unwrap().into_inner();

    let balance_i64 = raw_balance.as_i64();

    if balance_i64.is_none() {
        log::error!("❌ Balance not found");
        send_message(msg, bot, "❌ Balance not found".to_string()).await?;
        return Ok(());
    }

    let raw_balance = balance_i64.unwrap();

    // Convert raw balance to human readable format using decimals
    let human_balance = raw_balance as f64 / 10_f64.powi(decimals as i32);

    println!(
        "Raw balance: {}, Human balance: {}",
        raw_balance, human_balance
    );

    send_html_message(
        msg,
        bot,
        format!("💰 <b>Balance</b>: {:.2} {}", human_balance, token_symbol).to_string(),
    )
    .await?;

    Ok(())
}

pub async fn handle_group_balance(
    bot: Bot,
    msg: Message,
    bot_deps: BotDependencies,
    symbol: &str,
) -> AnyResult<()> {
    if !msg.chat.is_group() && !msg.chat.is_supergroup() {
        send_message(
            msg,
            bot,
            "❌ This command can only be used in a group".to_string(),
        )
        .await?;
        return Ok(());
    }

    let group_credentials = bot_deps.group.get_credentials(msg.chat.id);

    if group_credentials.is_none() {
        send_message(msg, bot, "❌ Group not found".to_string()).await?;
        return Ok(());
    }

    let group_credentials = group_credentials.unwrap();

    let (token_type, decimals, token_symbol) =
        if symbol.to_lowercase() == "apt" || symbol.to_lowercase() == "aptos" {
            (
                "0x1::aptos_coin::AptosCoin".to_string(),
                8u8,
                "APT".to_string(),
            )
        } else {
            let tokens = bot_deps.panora.get_token_by_symbol(symbol).await;

            if tokens.is_err() {
                log::error!("❌ Error getting token: {}", tokens.as_ref().err().unwrap());
                send_message(msg, bot, "❌ Error getting token".to_string()).await?;
                return Ok(());
            }

            let token = tokens.unwrap();

            let token_type = if token.token_address.as_ref().is_some() {
                token.token_address.as_ref().unwrap().to_string()
            } else {
                token.fa_address.clone()
            };

            (token_type, token.decimals, token.symbol.clone())
        };

    let balance = bot_deps
        .panora
        .aptos
        .node
        .get_account_balance(
            group_credentials.resource_account_address,
            token_type.to_string(),
        )
        .await;

    if balance.is_err() {
        log::error!(
            "❌ Error getting balance: {}",
            balance.as_ref().err().unwrap()
        );
        send_message(msg, bot, "❌ Error getting balance".to_string()).await?;
        return Ok(());
    }

    let raw_balance = balance.unwrap().into_inner();

    let balance_i64 = raw_balance.as_i64();

    if balance_i64.is_none() {
        log::error!("❌ Balance not found");
        send_message(msg, bot, "❌ Balance not found".to_string()).await?;
        return Ok(());
    }

    let raw_balance = balance_i64.unwrap();

    // Convert raw balance to human readable format using decimals
    let human_balance = raw_balance as f64 / 10_f64.powi(decimals as i32);

    send_html_message(
        msg,
        bot,
        format!("💰 <b>Balance</b>: {:.2} {}", human_balance, token_symbol).to_string(),
    )
    .await?;

    Ok(())
}

pub async fn handle_group_wallet_address(
    bot: Bot,
    msg: Message,
    bot_deps: BotDependencies,
) -> AnyResult<()> {
    if !msg.chat.is_group() && !msg.chat.is_supergroup() {
        send_message(
            msg,
            bot,
            "❌ This command can only be used in a group".to_string(),
        )
        .await?;
        return Ok(());
    }

    let group_credentials = bot_deps.group.get_credentials(msg.chat.id);

    log::info!("Group id: {:?}", msg.chat.id);

    if group_credentials.is_none() {
        send_message(msg, bot, "❌ Group not found".to_string()).await?;
        return Ok(());
    }

    let group_credentials = group_credentials.unwrap();

    send_html_message(
        msg,
        bot,
        format!(
            "💰 <b>Group Wallet Address</b>\n\n<code>{}</code>",
            group_credentials.resource_account_address
        )
        .to_string(),
    )
    .await?;

    Ok(())
}

// Removed unused function handle_moderation_rules

pub async fn handle_rules(bot: Bot, msg: Message, bot_deps: BotDependencies) -> AnyResult<()> {
    if !msg.chat.is_group() && !msg.chat.is_supergroup() {
        send_message(
            msg,
            bot,
            "❌ This command can only be used in a group".to_string(),
        )
        .await?;
        return Ok(());
    }

    let core = r#"
<b>🛡️ Core Moderation Rules</b>

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
- \"I can offer you whitelist access\"
- \"DM me for details\"
- \"React and I'll message you\"
- \"I'm a [title] and can help you\"
- \"Send me your wallet address\"
- \"Contact me privately\"
- \"I'll send you the link\"
"#;

    let settings = bot_deps
        .moderation
        .get_moderation_settings(msg.chat.id.to_string())
        .unwrap_or(crate::ai::moderation::dto::ModerationSettings::from((
            vec![],
            vec![],
            0,
            0,
        )));

    let mut custom_section = String::new();
    if settings.allowed_items.is_empty() && settings.disallowed_items.is_empty() {
        custom_section.push_str("<b>📋 Custom Rules</b>\n<i>No custom rules set.</i>");
    } else {
        custom_section.push_str("<b>📋 Custom Rules</b>\n");
        if !settings.allowed_items.is_empty() {
            custom_section.push_str("\n<b>Allowed</b>:\n");
            for item in settings.allowed_items.iter() {
                custom_section.push_str(&format!("• {}\n", item));
            }
        }
        if !settings.disallowed_items.is_empty() {
            custom_section.push_str("\n<b>Disallowed</b>:\n");
            for item in settings.disallowed_items.iter() {
                custom_section.push_str(&format!("• {}\n", item));
            }
        }
    }

    let text = format!(
        "{}\n\n{}\n\n<i>Ask an admin if unclear.</i>",
        core, custom_section
    );

    send_html_message(msg, bot, text).await?;
    Ok(())
}

async fn check_group_resource_account_address(
    bot: &Bot,
    group_credentials: GroupCredentials,
    msg: Message,
    bot_deps: &BotDependencies,
) -> AnyResult<GroupCredentials> {
    let group_id = group_credentials.group_id.clone();

    const MAX_RETRIES: u32 = 5;
    const RETRY_DELAY_MS: u64 = 2000;

    for attempt in 1..=MAX_RETRIES {
        let resource_account_address = bot_deps
            .panora
            .aptos
            .node
            .view_function(ViewRequest {
                function: format!(
                    "{}::group::get_group_account",
                    bot_deps.panora.aptos.contract_address
                ),
                type_arguments: vec![],
                arguments: vec![value::Value::String(group_id.clone())],
            })
            .await;

        if resource_account_address.is_ok() {
            let resource_account_address = resource_account_address.unwrap().into_inner();

            let resource_account_address =
                serde_json::from_value::<Vec<String>>(resource_account_address);

            if resource_account_address.is_ok() {
                let resource_account_address = resource_account_address.unwrap();

                let new_credentials = GroupCredentials {
                    jwt: group_credentials.jwt.clone(),
                    group_id: group_credentials.group_id.clone(),
                    resource_account_address: resource_account_address[0].clone(),
                    users: group_credentials.users.clone(),
                };

                bot_deps
                    .group
                    .save_credentials(new_credentials)
                    .map_err(|_| anyhow::anyhow!("Error saving group credentials"))?;

                let updated_credentials = GroupCredentials {
                    jwt: group_credentials.jwt,
                    group_id: group_credentials.group_id,
                    resource_account_address: resource_account_address[0].clone(),
                    users: group_credentials.users,
                };

                return Ok(updated_credentials);
            }
        }

        // If this is not the last attempt, wait before retrying
        if attempt < MAX_RETRIES {
            log::warn!(
                "Failed to get resource account address (attempt {}/{}), retrying in {}ms...",
                attempt,
                MAX_RETRIES,
                RETRY_DELAY_MS
            );
            sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
        }
    }

    // All retries failed
    send_message(
        msg,
        bot.clone(),
        "❌ Error getting resource account address after multiple attempts".to_string(),
    )
    .await?;
    return Err(anyhow::anyhow!(
        "Error getting resource account address after {} attempts",
        MAX_RETRIES
    ));
}
