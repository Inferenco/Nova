use anyhow::Result;
use teloxide::{
    prelude::*,
    types::{
        InlineKeyboardButton as Btn, InlineKeyboardMarkup, MaybeInaccessibleMessage,
    },
};

use crate::{
    dependencies::BotDependencies,
    scheduled_prompts::dto::{PendingStep, RepeatPolicy},
    scheduled_prompts::handler::finalize_and_register,
    scheduled_prompts::helpers::{
        build_hours_keyboard_with_nav_prompt, build_image_keyboard_with_nav_prompt,
        build_minutes_keyboard_with_nav_prompt, build_nav_keyboard_prompt,
        build_repeat_keyboard_with_nav_prompt, reset_from_step_prompts, summarize,
        send_step_message, delete_message_safe,
    },
};

pub async fn handle_scheduled_prompts_callback(
    bot: Bot,
    query: teloxide::types::CallbackQuery,
    bot_deps: BotDependencies,
) -> Result<()> {
    let data = query.data.as_deref().unwrap_or("");
    let user = &query.from;
    let message = match &query.message {
        Some(MaybeInaccessibleMessage::Regular(m)) => m,
        _ => {
            bot.answer_callback_query(query.id)
                .text("❌ Invalid context")
                .await?;
            return Ok(());
        }
    };

    // Admin-only actions
    let admins = bot.get_chat_administrators(message.chat.id).await?;
    if !admins.iter().any(|m| m.user.id == user.id) {
        bot.answer_callback_query(query.id)
            .text("❌ Admins only")
            .await?;
        return Ok(());
    }
    let key = (&message.chat.id.0, &(user.id.0 as i64));

    if data.starts_with("sched_back") {
        let key = (&message.chat.id.0, &(user.id.0 as i64));
        if let Some(mut st) = bot_deps.scheduled_storage.get_pending(key) {
            let prev = match st.step {
                PendingStep::AwaitingConfirm => Some(PendingStep::AwaitingRepeat),
                PendingStep::AwaitingRepeat => Some(PendingStep::AwaitingMinute),
                PendingStep::AwaitingMinute => Some(PendingStep::AwaitingHour),
                PendingStep::AwaitingHour => Some(PendingStep::AwaitingImage),
                PendingStep::AwaitingImage => Some(PendingStep::AwaitingPrompt),
                PendingStep::AwaitingPrompt => None,
            };
            if let Some(prev_step) = prev {
                // Delete current message
                if let Some(current_msg_id) = st.current_bot_message_id {
                    delete_message_safe(&bot, message.chat.id, current_msg_id).await;
                }
                
                reset_from_step_prompts(&mut st, prev_step.clone());
                st.step = prev_step.clone();
                bot.answer_callback_query(query.id).await?;
                
                // Send fresh message for previous step
                let (text, kb) = match prev_step {
                    PendingStep::AwaitingPrompt => {
                        ("📝 Send the prompt you want to schedule — you can reply to this message or just send it as your next message.".to_string(),
                         build_nav_keyboard_prompt(false))
                    }
                    PendingStep::AwaitingImage => {
                        ("📷 Attach an image to use with this scheduled prompt (optional)\n\nSend a photo, or click Skip Image to continue.".to_string(),
                         build_image_keyboard_with_nav_prompt(true))
                    }
                    PendingStep::AwaitingHour => {
                        ("Select start hour (UTC)".to_string(),
                         build_hours_keyboard_with_nav_prompt(true))
                    }
                    PendingStep::AwaitingMinute => {
                        ("Select start minute (UTC)".to_string(),
                         build_minutes_keyboard_with_nav_prompt(true))
                    }
                    PendingStep::AwaitingRepeat => {
                        ("Select repeat interval".to_string(),
                         build_repeat_keyboard_with_nav_prompt(true))
                    }
                    PendingStep::AwaitingConfirm => {
                        ("".to_string(), build_nav_keyboard_prompt(false)) // unreachable
                    }
                };
                
                // Send new message and capture ID
                match send_step_message(bot.clone(), message.chat.id, st.thread_id, &text, kb).await {
                    Ok(sent_msg) => {
                        st.current_bot_message_id = Some(sent_msg.id.0);
                        bot_deps.scheduled_storage.put_pending(key, &st)?;
                    }
                    Err(e) => {
                        log::error!("Failed to send back step message: {}", e);
                    }
                }
            } else {
                bot.answer_callback_query(query.id)
                    .text("ℹ️ Already at first step")
                    .await?;
            }
        } else {
            bot.answer_callback_query(query.id)
                .text("ℹ️ No pending schedule to navigate")
                .await?;
        }
    } else if data == "sched_cancel" {
        let key = (&message.chat.id.0, &(user.id.0 as i64));
        if let Some(st) = bot_deps.scheduled_storage.get_pending(key) {
            // Delete current message
            if let Some(current_msg_id) = st.current_bot_message_id {
                delete_message_safe(&bot, message.chat.id, current_msg_id).await;
            }
            
            bot_deps.scheduled_storage.delete_pending(key)?;
            bot.answer_callback_query(query.id)
                .text("✅ Cancelled")
                .await?;
        } else {
            bot.answer_callback_query(query.id)
                .text("ℹ️ No pending schedule to cancel")
                .await?;
        }
    } else if data == "sched_skip_image" {
        if let Some(mut st) = bot_deps.scheduled_storage.get_pending(key) {
            // Delete current message
            if let Some(current_msg_id) = st.current_bot_message_id {
                delete_message_safe(&bot, message.chat.id, current_msg_id).await;
            }
            
            st.step = PendingStep::AwaitingHour;
            st.image_url = None;
            bot.answer_callback_query(query.id).await?;
            
            // Send new message and capture ID
            match send_step_message(
                bot.clone(),
                message.chat.id,
                st.thread_id,
                "Select start hour (UTC)",
                build_hours_keyboard_with_nav_prompt(true),
            )
            .await {
                Ok(sent_msg) => {
                    st.current_bot_message_id = Some(sent_msg.id.0);
                    bot_deps.scheduled_storage.put_pending(key, &st)?;
                }
                Err(e) => {
                    log::error!("Failed to send hour selection message: {}", e);
                }
            }
        }
    } else if data.starts_with("sched_hour:") {
        let hour: u8 = data.split(':').nth(1).unwrap_or("0").parse().unwrap_or(0);
        if let Some(mut st) = bot_deps.scheduled_storage.get_pending(key) {
            // Delete current message
            if let Some(current_msg_id) = st.current_bot_message_id {
                delete_message_safe(&bot, message.chat.id, current_msg_id).await;
            }
            
            st.step = PendingStep::AwaitingMinute;
            st.hour_utc = Some(hour);
            bot.answer_callback_query(query.id).await?;
            
            // Send new message and capture ID
            match send_step_message(
                bot.clone(),
                message.chat.id,
                st.thread_id,
                "Select start minute (UTC)",
                build_minutes_keyboard_with_nav_prompt(true),
            )
            .await {
                Ok(sent_msg) => {
                    st.current_bot_message_id = Some(sent_msg.id.0);
                    bot_deps.scheduled_storage.put_pending(key, &st)?;
                }
                Err(e) => {
                    log::error!("Failed to send minute selection message: {}", e);
                }
            }
        }
    } else if data.starts_with("sched_min:") {
        let minute: u8 = data.split(':').nth(1).unwrap_or("0").parse().unwrap_or(0);
        if let Some(mut st) = bot_deps.scheduled_storage.get_pending(key) {
            // Delete current message
            if let Some(current_msg_id) = st.current_bot_message_id {
                delete_message_safe(&bot, message.chat.id, current_msg_id).await;
            }
            
            st.step = PendingStep::AwaitingRepeat;
            st.minute_utc = Some(minute);
            bot.answer_callback_query(query.id).await?;
            
            // Send new message and capture ID
            match send_step_message(
                bot.clone(),
                message.chat.id,
                st.thread_id,
                "Select repeat interval",
                build_repeat_keyboard_with_nav_prompt(true),
            )
            .await {
                Ok(sent_msg) => {
                    st.current_bot_message_id = Some(sent_msg.id.0);
                    bot_deps.scheduled_storage.put_pending(key, &st)?;
                }
                Err(e) => {
                    log::error!("Failed to send repeat selection message: {}", e);
                }
            }
        }
    } else if data.starts_with("sched_repeat:") {
        let repeat = match data.split(':').nth(1).unwrap_or("") {
            "none" => RepeatPolicy::None,
            "5m" => RepeatPolicy::Every5m,
            "15m" => RepeatPolicy::Every15m,
            "30m" => RepeatPolicy::Every30m,
            "45m" => RepeatPolicy::Every45m,
            "1h" => RepeatPolicy::Every1h,
            "3h" => RepeatPolicy::Every3h,
            "6h" => RepeatPolicy::Every6h,
            "12h" => RepeatPolicy::Every12h,
            "1d" => RepeatPolicy::Daily,
            "1w" => RepeatPolicy::Weekly,
            "1mo" => RepeatPolicy::Monthly,
            _ => RepeatPolicy::Every1h,
        };
        if let Some(mut st) = bot_deps.scheduled_storage.get_pending(key) {
            // Delete current message
            if let Some(current_msg_id) = st.current_bot_message_id {
                delete_message_safe(&bot, message.chat.id, current_msg_id).await;
            }
            
            st.step = PendingStep::AwaitingConfirm;
            st.repeat = Some(repeat);
            
            let summary = summarize(&st);
            let kb = InlineKeyboardMarkup::new(vec![
                vec![Btn::callback(
                    "✔️ Create schedule".to_string(),
                    "sched_confirm".to_string(),
                )],
                vec![
                    Btn::callback("↩️ Back".to_string(), "sched_back".to_string()),
                    Btn::callback("❌ Cancel".to_string(), "sched_cancel".to_string()),
                ],
            ]);
            bot.answer_callback_query(query.id).await?;
            
            // Send new confirmation message and capture ID
            match send_step_message(bot.clone(), message.chat.id, st.thread_id, &summary, kb).await {
                Ok(sent_msg) => {
                    st.current_bot_message_id = Some(sent_msg.id.0);
                    bot_deps.scheduled_storage.put_pending(key, &st)?;
                }
                Err(e) => {
                    log::error!("Failed to send confirmation message: {}", e);
                }
            }
        }
    } else if data == "sched_confirm" {
        if let Some(st) = bot_deps.scheduled_storage.get_pending(key) {
            // Delete the confirmation message
            if let Some(current_msg_id) = st.current_bot_message_id {
                delete_message_safe(&bot, message.chat.id, current_msg_id).await;
            }
            
            bot_deps.scheduled_storage.delete_pending(key)?;
            finalize_and_register(*message.clone(), bot.clone(), bot_deps.clone(), st).await?;
            bot.answer_callback_query(query.id).await?;
        }
    } else if data.starts_with("sched_cancel:") {
        let id = data.split(':').nth(1).unwrap_or("");
        let rec = bot_deps.scheduled_storage.get_schedule(id);
        if let Some(mut rec) = rec {
            if rec.group_id != message.chat.id.0 as i64 {
                bot.answer_callback_query(query.id)
                    .text("❌ Wrong group")
                    .await?;
                return Ok(());
            }
            rec.active = false;
            bot_deps.scheduled_storage.put_schedule(&rec)?;
            bot.answer_callback_query(query.id)
                .text("✅ Cancelled")
                .await?;
            // Delete the message that contained the cancel button
            if let Err(e) = bot.delete_message(message.chat.id, message.id).await {
                log::warn!(
                    "Failed to delete schedule-cancel message {}: {}",
                    message.id.0,
                    e
                );
            }
        }
    } else if data.starts_with("sched_close") {
        let id = data.split(':').nth(1).unwrap_or("");
        bot.answer_callback_query(query.id).await?;
        if let Err(e) = bot.delete_message(message.chat.id, message.id).await {
            log::warn!(
                "Failed to close schedule message {} (schedule {}): {}",
                message.id.0,
                id,
                e
            );
        }
    } else {
        bot.answer_callback_query(query.id)
            .text("❌ Unknown action")
            .await?;
    }

    Ok(())
}
