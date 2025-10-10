use anyhow::Result;
use chrono::Utc;
use open_ai_rust_responses_by_sshift::Model;
use teloxide::{
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, Message, User},
    net::Download,
};
use tokio::fs::File;
use uuid::Uuid;

use crate::{
    dependencies::BotDependencies,
    scheduled_prompts::{
        dto::{PendingStep, PendingWizardState, RepeatPolicy, ScheduledPromptRecord},
        helpers::{build_hours_keyboard_with_nav_prompt, build_image_keyboard_with_nav_prompt, build_nav_keyboard_prompt, summarize, send_step_message, cleanup_and_transition},
        runner::register_schedule,
    },
    utils::{
        KeyboardMarkupType, create_purchase_request,
        send_markdown_message_with_keyboard, send_message,
    },
};

pub async fn handle_scheduleprompt_command(
    bot: Bot,
    msg: Message,
    bot_deps: BotDependencies,
) -> Result<()> {
    if !msg.chat.is_group() && !msg.chat.is_supergroup() {
        send_message(
            msg.clone(),
            bot,
            "❌ This command is only available in groups.".to_string(),
        )
        .await?;
        return Ok(());
    }

    // Admin check
    let admins = bot.get_chat_administrators(msg.chat.id).await?;
    let user = match msg.from.as_ref() {
        Some(u) => u,
        None => {
            return Ok(());
        }
    };
    if !admins.iter().any(|m| m.user.id == user.id) {
        send_message(
            msg.clone(),
            bot,
            "❌ Only administrators can use this command.".to_string(),
        )
        .await?;
        return Ok(());
    }

    let username = match user.username.clone() {
        Some(u) => u,
        None => {
            send_message(
                msg.clone(),
                bot,
                "❌ Username required to schedule prompts.".to_string(),
            )
            .await?;
            return Ok(());
        }
    };

    let mut state = PendingWizardState {
        group_id: msg.chat.id.0 as i64,
        creator_user_id: user.id.0 as i64,
        creator_username: username,
        step: PendingStep::AwaitingPrompt,
        prompt: None,
        image_url: None,
        hour_utc: None,
        minute_utc: None,
        repeat: None,
        thread_id: if let Some(thread_id) = msg.thread_id {
            Some(thread_id.0.0)
        } else {
            None
        },
        current_bot_message_id: None,
        user_message_ids: Vec::new(),
    };

    let note = "\n\nℹ️ Note about tools for scheduled prompts:\n\n• Allowed: market data via GeckoTerminal and BitcoinTry exchange, time lookups, fear & greed index, group recaps.\n• Unavailable: any tool that requires user confirmation or performs transactions (e.g., pay users, withdrawals, funding, creating proposals or other interactive flows).\n\nTip: Schedule informational queries, summaries, monitoring, or analytics. Avoid actions that need real-time human approval.";

    // First step: show Cancel only
    let kb = build_nav_keyboard_prompt(false);
    let sent_msg = send_step_message(
        bot,
        msg.chat.id,
        state.thread_id,
        &format!(
            "📝 Send the prompt you want to schedule — you can <b>reply to this message</b> or just <b>send it as your next message</b>.{}\n\nIf your prompt is rejected for using a forbidden action, <b>try again</b> with a safer prompt.",
            note
        ),
        kb,
    )
    .await?;
    
    // Store the message ID
    state.current_bot_message_id = Some(sent_msg.id.0);
    
    bot_deps
        .scheduled_storage
        .put_pending((&state.group_id, &state.creator_user_id), &state)?;

    Ok(())
}

pub async fn handle_listscheduled_command(
    bot: Bot,
    msg: Message,
    bot_deps: BotDependencies,
) -> Result<()> {
    // Admin check
    let admins = bot.get_chat_administrators(msg.chat.id).await?;
    let user = match msg.from.as_ref() {
        Some(u) => u,
        None => {
            return Ok(());
        }
    };
    if !admins.iter().any(|m| m.user.id == user.id) {
        send_message(
            msg.clone(),
            bot,
            "❌ Only administrators can use this command.".to_string(),
        )
        .await?;
        return Ok(());
    }

    let list = bot_deps
        .scheduled_storage
        .list_schedules_for_group(msg.chat.id.0 as i64);

    if list.is_empty() {
        send_message(
            msg.clone(),
            bot.clone(),
            "📭 No active scheduled prompts in this group.".to_string(),
        )
        .await?;
        return Ok(());
    }

    for rec in list {
        let repeat_label = match rec.repeat {
            RepeatPolicy::None => "No repeat".to_string(),
            RepeatPolicy::Every5m => "Every 5 min".to_string(),
            RepeatPolicy::Every15m => "Every 15 min".to_string(),
            RepeatPolicy::Every30m => "Every 30 min".to_string(),
            RepeatPolicy::Every45m => "Every 45 min".to_string(),
            RepeatPolicy::Every1h => "Every 1 hour".to_string(),
            RepeatPolicy::Every3h => "Every 3 hours".to_string(),
            RepeatPolicy::Every6h => "Every 6 hours".to_string(),
            RepeatPolicy::Every12h => "Every 12 hours".to_string(),
            RepeatPolicy::Daily => "Daily".to_string(),
            RepeatPolicy::Weekly => "Weekly".to_string(),
            RepeatPolicy::Monthly => "Monthly".to_string(),
        };
        let title = format!(
            "⏰ {:02}:{:02} UTC — {}\n\n{}",
            rec.start_hour_utc,
            rec.start_minute_utc,
            repeat_label,
            if rec.prompt.len() > 180 {
                format!("{}…", &rec.prompt[..180])
            } else {
                rec.prompt.clone()
            }
        );
        let kb = InlineKeyboardMarkup::new(vec![vec![
            InlineKeyboardButton::callback(
                "🗑️ Delete".to_string(),
                format!("sched_cancel:{}", rec.id),
            ),
            InlineKeyboardButton::callback(
                "✖️ Close".to_string(),
                format!("sched_close:{}", rec.id),
            ),
        ]]);
        send_markdown_message_with_keyboard(
            bot.clone(),
            msg.clone(),
            KeyboardMarkupType::InlineKeyboardType(kb),
            &title,
        )
        .await?;
    }

    Ok(())
}

pub async fn finalize_and_register(
    msg: Message,
    bot: Bot,
    bot_deps: BotDependencies,
    state: PendingWizardState,
) -> Result<()> {
    // Enforce per-group cap: max 10 active schedules
    let active_count = bot_deps
        .scheduled_storage
        .list_schedules_for_group(state.group_id)
        .len();
    if active_count >= 10 {
        send_message(
            msg.clone(),
            bot.clone(),
            "❌ You already have 10 active scheduled prompts in this group.\n\nPlease cancel one with /listscheduled before adding a new schedule.".to_string(),
        )
        .await?;
        return Ok(());
    }

    let id = Uuid::new_v4().to_string();
    let mut rec = ScheduledPromptRecord {
        id: id.clone(),
        group_id: state.group_id,
        creator_user_id: state.creator_user_id,
        creator_username: state.creator_username.clone(),
        prompt: state.prompt.clone().unwrap_or_default(),
        image_url: state.image_url.clone(),
        start_hour_utc: state.hour_utc.unwrap_or(0),
        start_minute_utc: state.minute_utc.unwrap_or(0),
        repeat: state.repeat.clone().unwrap_or(RepeatPolicy::None),
        active: true,
        created_at: Utc::now().timestamp(),
        last_run_at: None,
        next_run_at: None,
        run_count: 0,
        locked_until: None,
        scheduler_job_id: None,
        conversation_response_id: None,
        thread_id: state.thread_id,
    };

    bot_deps.scheduled_storage.put_schedule(&rec)?;
    register_schedule(bot.clone(), bot_deps.clone(), &mut rec).await?;
    bot_deps.scheduled_storage.put_schedule(&rec)?;

    // Send success message to the thread (not as a reply to deleted message)
    use teloxide::types::ParseMode;
    use teloxide::prelude::*;
    let success_text = format!(
        "✅ Scheduled created!\n\n{}",
        summarize(&PendingWizardState {
            group_id: rec.group_id,
            creator_user_id: rec.creator_user_id,
            creator_username: rec.creator_username,
            step: PendingStep::AwaitingConfirm,
            prompt: Some(rec.prompt),
            image_url: rec.image_url,
            hour_utc: Some(rec.start_hour_utc),
            minute_utc: Some(rec.start_minute_utc),
            repeat: Some(rec.repeat),
            thread_id: rec.thread_id,
            current_bot_message_id: None,
            user_message_ids: Vec::new(),
        })
    );
    
    let mut request = bot.send_message(msg.chat.id, success_text)
        .parse_mode(ParseMode::Html);
    
    // For forum topics, use message_thread_id instead of reply_to
    if let Some(thread) = state.thread_id {
        request = request.message_thread_id(teloxide::types::ThreadId(teloxide::types::MessageId(thread)));
    }
    
    request.await?;

    Ok(())
}

pub async fn handle_message_scheduled_prompts(
    bot: Bot,
    msg: Message,
    bot_deps: BotDependencies,
    user: User,
) -> Result<bool> {
    let key = (&msg.chat.id.0, &(user.id.0 as i64));
    if let Some(mut st) = bot_deps.scheduled_storage.get_pending(key) {
        if st.step == PendingStep::AwaitingPrompt {
            // Accept prompt if message is a reply OR a regular follow-up (non-command) from the same user
            let is_reply = msg.reply_to_message().is_some();
            let text_raw = msg.text().or_else(|| msg.caption()).unwrap_or("");
            let is_command = text_raw.trim_start().starts_with('/');
            if is_reply || (!is_command && !text_raw.trim().is_empty()) {
                let text = text_raw.to_string();
                // Guard scheduled prompt against forbidden tools
                {
                    let guard = &bot_deps.schedule_guard;
                    match guard.check_prompt(&text).await {
                        Ok(res) => {
                            // Bill the group for the guard check like moderation
                            if let Some(group_credentials) =
                                bot_deps.group.get_credentials(msg.chat.id)
                            {
                                if let Err(e) = create_purchase_request(
                                    0, // file_search
                                    0, // web_search
                                    0, // image_gen
                                    res.total_tokens,
                                    Model::GPT5Nano,
                                    &group_credentials.jwt,
                                    Some(msg.chat.id.0.to_string()),
                                    None,
                                    bot_deps.clone(),
                                )
                                .await
                                {
                                    log::warn!("schedule guard purchase request failed: {}", e);
                                }
                            }
                            if res.verdict == "F" {
                                let reason = res.reason.unwrap_or_else(|| {
                                    "Prompt requests a forbidden action for scheduled runs"
                                        .to_string()
                                });
                                let warn = format!(
                                    "❌ This prompt can't be scheduled. PLEASE TRY AGAIN\n\n<b>Reason:</b> {}\n\n<b>Common fixes:</b>\n• For token prices: Specify a network (e.g., \"Show WSOH price on BSC\")\n• For trending pools: Add network (e.g., \"Show trending pools on Aptos\")\n• For new pools: Include network (e.g., \"Show new pools on Solana\")\n\n<b>✅ Always allowed:</b> Time queries, fear &amp; greed index, weather, general info\n\n<b>❌ Never allowed:</b> Payments, withdrawals, DAO creation, user interactions\n\nPlease send a new prompt with the network specified.",
                                    teloxide::utils::html::escape(&reason)
                                );
                                crate::utils::send_html_message(msg.clone(), bot, warn).await?;
                                // Do not advance wizard; let user try again by sending a new prompt
                                return Ok(true);
                            }
                        }
                        Err(e) => {
                            log::warn!("schedule_guard check failed: {}", e);
                        }
                    }
                }

                // Clean up old messages
                cleanup_and_transition(&bot, &mut st, msg.chat.id, Some(msg.id.0)).await;
                
                st.prompt = Some(text);
                st.step = PendingStep::AwaitingImage;
                
                // Send next step and capture message ID
                let kb = build_image_keyboard_with_nav_prompt(true);
                match send_step_message(
                    bot.clone(),
                    msg.chat.id,
                    st.thread_id,
                    "📷 Attach an image to use with this scheduled prompt (optional)\n\nSend a photo, or click Skip Image to continue.",
                    kb,
                )
                .await {
                    Ok(sent_msg) => {
                        st.current_bot_message_id = Some(sent_msg.id.0);
                        if let Err(e) = bot_deps.scheduled_storage.put_pending(key, &st) {
                            log::error!("Failed to persist scheduled wizard state: {}", e);
                            send_message(
                                msg.clone(),
                                bot,
                                "❌ Error saving schedule state. Please try /scheduleprompt again."
                                    .to_string(),
                            )
                            .await?;
                        }
                        return Ok(true);
                    }
                    Err(e) => {
                        log::error!("Failed to send step message: {}", e);
                        return Ok(true);
                    }
                }
            }
        } else if st.step == PendingStep::AwaitingImage {
            // Handle image upload
            if let Some(photo_sizes) = msg.photo() {
                if let Some(largest_photo) = photo_sizes.last() {
                    let user_id = user.id.0 as i64;
                    // Download the image to temp file
                    let file_info = bot.get_file(largest_photo.file.id.clone()).await?;
                    let extension = file_info
                        .path
                        .split('.')
                        .last()
                        .unwrap_or("jpg")
                        .to_string();
                    let temp_path = format!("/tmp/sched_{}_{}.{}", user_id, largest_photo.file.unique_id, extension);
                    let mut file = File::create(&temp_path)
                        .await
                        .map_err(|e| teloxide::RequestError::from(std::sync::Arc::new(e)))?;
                    bot.download_file(&file_info.path, &mut file)
                        .await
                        .map_err(|e| teloxide::RequestError::from(e))?;
                    
                    // Upload to GCS using AI handler
                    match bot_deps.ai.upload_user_images(vec![(temp_path, extension)]).await {
                        Ok(urls) if !urls.is_empty() => {
                            // Clean up old messages
                            cleanup_and_transition(&bot, &mut st, msg.chat.id, Some(msg.id.0)).await;
                            
                            st.image_url = Some(urls[0].clone());
                            st.step = PendingStep::AwaitingHour;
                            
                            // Send next step and capture message ID
                            let kb = build_hours_keyboard_with_nav_prompt(true);
                            match send_step_message(
                                bot.clone(),
                                msg.chat.id,
                                st.thread_id,
                                "✅ Image uploaded! Now select start hour (UTC)",
                                kb,
                            )
                            .await {
                                Ok(sent_msg) => {
                                    st.current_bot_message_id = Some(sent_msg.id.0);
                                    if let Err(e) = bot_deps.scheduled_storage.put_pending(key, &st) {
                                        log::error!("Failed to persist scheduled wizard state: {}", e);
                                        send_message(
                                            msg.clone(),
                                            bot,
                                            "❌ Error saving schedule state. Please try /scheduleprompt again."
                                                .to_string(),
                                        )
                                        .await?;
                                    }
                                    return Ok(true);
                                }
                                Err(e) => {
                                    log::error!("Failed to send step message: {}", e);
                                    return Ok(true);
                                }
                            }
                        }
                        Ok(_) => {
                            log::error!("Image upload returned empty URLs");
                            send_message(
                                msg.clone(),
                                bot,
                                "❌ Failed to upload image. Please try again or click Skip Image.".to_string(),
                            )
                            .await?;
                            return Ok(true);
                        }
                        Err(e) => {
                            log::error!("Failed to upload image to GCS: {}", e);
                            send_message(
                                msg.clone(),
                                bot,
                                "❌ Failed to upload image. Please try again or click Skip Image.".to_string(),
                            )
                            .await?;
                            return Ok(true);
                        }
                    }
                }
            }
        }
    }

    Ok(false)
}
