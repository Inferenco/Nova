use anyhow::Result;
use chrono::{Timelike, Utc};
use teloxide::{prelude::*, types::{InlineKeyboardMarkup, InlineKeyboardButton as Btn}};

use crate::dependencies::BotDependencies;
use crate::scheduled_payments::dto::PendingPaymentStep;
use crate::scheduled_payments::helpers::{
    build_hours_keyboard_with_nav_payments,
    build_minutes_keyboard_with_nav_payments,
    build_repeat_keyboard_payments,
    build_repeat_keyboard_with_nav_payments,
    build_nav_keyboard_payments,
    reset_from_step_payments,
    summarize,
    send_step_message,
    delete_message_safe,
};
use crate::scheduled_prompts::dto::RepeatPolicy;

pub async fn handle_scheduled_payments_callback(
    bot: Bot,
    query: teloxide::types::CallbackQuery,
    bot_deps: BotDependencies,
) -> Result<()> {
    let data = query.data.as_deref().unwrap_or("");
    let user = &query.from;
    let message = match &query.message {
        Some(teloxide::types::MaybeInaccessibleMessage::Regular(m)) => m,
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

    if data.starts_with("schedpay_hour:") {
        let hour: u8 = data.split(':').nth(1).unwrap_or("0").parse().unwrap_or(0);
        if let Some(mut st) = bot_deps.scheduled_payments.get_pending(key) {
            // Delete current message
            if let Some(current_msg_id) = st.current_bot_message_id {
                delete_message_safe(&bot, message.chat.id, current_msg_id).await;
            }
            
            st.step = PendingPaymentStep::AwaitingMinute;
            st.hour_utc = Some(hour);
            bot.answer_callback_query(query.id).await?;
            
            // Send new message and capture ID
            match send_step_message(
                bot.clone(),
                message.chat.id,
                "Select minute (UTC)",
                build_minutes_keyboard_with_nav_payments(true),
            )
            .await {
                Ok(sent_msg) => {
                    st.current_bot_message_id = Some(sent_msg.id.0);
                    bot_deps.scheduled_payments.put_pending(key, &st)?;
                }
                Err(e) => {
                    log::error!("Failed to send minute selection message: {}", e);
                }
            }
        }
    } else if data.starts_with("schedpay_min:") {
        let minute: u8 = data.split(':').nth(1).unwrap_or("0").parse().unwrap_or(0);
        if let Some(mut st) = bot_deps.scheduled_payments.get_pending(key) {
            // Delete current message
            if let Some(current_msg_id) = st.current_bot_message_id {
                delete_message_safe(&bot, message.chat.id, current_msg_id).await;
            }
            
            st.step = PendingPaymentStep::AwaitingRepeat;
            st.minute_utc = Some(minute);
            bot.answer_callback_query(query.id).await?;
            
            // Send new message and capture ID
            match send_step_message(
                bot.clone(),
                message.chat.id,
                "Select repeat interval",
                build_repeat_keyboard_with_nav_payments(true),
            )
            .await {
                Ok(sent_msg) => {
                    st.current_bot_message_id = Some(sent_msg.id.0);
                    bot_deps.scheduled_payments.put_pending(key, &st)?;
                }
                Err(e) => {
                    log::error!("Failed to send repeat selection message: {}", e);
                }
            }
        }
    } else if data.starts_with("schedpay_repeat:") {
        let (repeat, weeks) = match data.split(':').nth(1).unwrap_or("") {
            "1d" => (RepeatPolicy::Daily, None),
            "1w" => (RepeatPolicy::Weekly, Some(1)),
            "2w" => (RepeatPolicy::Weekly, Some(2)),
            "4w" => (RepeatPolicy::Weekly, Some(4)),
            _ => (RepeatPolicy::Weekly, Some(1)),
        };
        if let Some(mut st) = bot_deps.scheduled_payments.get_pending(key) {
            // Delete current message
            if let Some(current_msg_id) = st.current_bot_message_id {
                delete_message_safe(&bot, message.chat.id, current_msg_id).await;
            }
            
            st.step = PendingPaymentStep::AwaitingConfirm;
            st.repeat = Some(repeat);
            st.weekly_weeks = weeks;
            
            let summary = summarize(&st);
            let kb = InlineKeyboardMarkup::new(vec![
                vec![Btn::callback(
                    "✔️ Create schedule".to_string(),
                    "schedpay_confirm".to_string(),
                )],
                vec![
                    Btn::callback("↩️ Back".to_string(), "schedpay_back".to_string()),
                    Btn::callback("❌ Cancel".to_string(), "schedpay_cancel".to_string()),
                ],
            ]);
            bot.answer_callback_query(query.id).await?;
            
            // Send new confirmation message and capture ID
            match send_step_message(bot.clone(), message.chat.id, &summary, kb).await {
                Ok(sent_msg) => {
                    st.current_bot_message_id = Some(sent_msg.id.0);
                    bot_deps.scheduled_payments.put_pending(key, &st)?;
                }
                Err(e) => {
                    log::error!("Failed to send confirmation message: {}", e);
                }
            }
        }
    } else if data == "schedpay_confirm" {
        // Only the creator can confirm their own pending payment
        if let Some(st) = bot_deps.scheduled_payments.get_pending(key) {
            if st.creator_user_id != user.id.0 as i64 {
                bot.answer_callback_query(query.id)
                    .text("❌ Only the creator can confirm this payment")
                    .await?;
                return Ok(());
            }
            
            // Delete the confirmation message
            if let Some(current_msg_id) = st.current_bot_message_id {
                delete_message_safe(&bot, message.chat.id, current_msg_id).await;
            }
            
            bot_deps.scheduled_payments.delete_pending(key)?;
            super::handler::finalize_and_register_payment(
                *message.clone(),
                bot.clone(),
                bot_deps.clone(),
                st,
            )
            .await?;
            bot.answer_callback_query(query.id).await?;
        } else {
            // No pending payment exists - still respond to prevent UI hang
            bot.answer_callback_query(query.id)
                .text("ℹ️ No pending payment to confirm")
                .await?;
        }
    } else if data == "schedpay_cancel" {
        // Only the creator can cancel their own pending payment
        if let Some(st) = bot_deps.scheduled_payments.get_pending(key) {
            if st.creator_user_id != user.id.0 as i64 {
                bot.answer_callback_query(query.id)
                    .text("❌ Only the creator can cancel this payment")
                    .await?;
                return Ok(());
            }
            
            // Delete current message
            if let Some(current_msg_id) = st.current_bot_message_id {
                delete_message_safe(&bot, message.chat.id, current_msg_id).await;
            }
            
            bot_deps.scheduled_payments.delete_pending(key)?;
            bot.answer_callback_query(query.id)
                .text("✅ Cancelled")
                .await?;
        } else {
            // No pending payment exists - still respond to prevent UI hang
            bot.answer_callback_query(query.id)
                .text("ℹ️ No pending payment to cancel")
                .await?;
        }
    } else if data == "schedpay_back" {
        if let Some(mut st) = bot_deps.scheduled_payments.get_pending(key) {
            // Determine previous step
            let prev = match st.step {
                PendingPaymentStep::AwaitingConfirm => Some(PendingPaymentStep::AwaitingRepeat),
                PendingPaymentStep::AwaitingRepeat => Some(PendingPaymentStep::AwaitingMinute),
                PendingPaymentStep::AwaitingMinute => Some(PendingPaymentStep::AwaitingHour),
                PendingPaymentStep::AwaitingHour => Some(PendingPaymentStep::AwaitingDate),
                PendingPaymentStep::AwaitingDate => Some(PendingPaymentStep::AwaitingAmount),
                PendingPaymentStep::AwaitingAmount => Some(PendingPaymentStep::AwaitingToken),
                PendingPaymentStep::AwaitingToken => Some(PendingPaymentStep::AwaitingRecipient),
                PendingPaymentStep::AwaitingRecipient => None,
            };
            if let Some(prev_step) = prev {
                // Delete current message
                if let Some(current_msg_id) = st.current_bot_message_id {
                    delete_message_safe(&bot, message.chat.id, current_msg_id).await;
                }
                
                // reset values from this step onward so user starts again from here
                reset_from_step_payments(&mut st, prev_step.clone());
                st.step = prev_step.clone();
                bot.answer_callback_query(query.id).await?;
                
                // Send fresh message for previous step
                let (text, kb) = match prev_step {
                    PendingPaymentStep::AwaitingRecipient => {
                        ("👤 Send the recipient @username to receive payment (must have a linked wallet).".to_string(),
                         build_nav_keyboard_payments(false))
                    }
                    PendingPaymentStep::AwaitingToken => {
                        ("💳 Send token symbol (e.g., APT, USDC, or emoji)".to_string(),
                         build_nav_keyboard_payments(true))
                    }
                    PendingPaymentStep::AwaitingAmount => {
                        ("💰 Send amount (decimal)".to_string(),
                         build_nav_keyboard_payments(true))
                    }
                    PendingPaymentStep::AwaitingDate => {
                        ("📅 Send start date in YYYY-MM-DD (UTC)".to_string(),
                         build_nav_keyboard_payments(true))
                    }
                    PendingPaymentStep::AwaitingHour => {
                        ("⏰ Select hour (UTC)".to_string(),
                         build_hours_keyboard_with_nav_payments(true))
                    }
                    PendingPaymentStep::AwaitingMinute => {
                        ("Select minute (UTC)".to_string(),
                         build_minutes_keyboard_with_nav_payments(true))
                    }
                    PendingPaymentStep::AwaitingRepeat => {
                        ("Select repeat interval".to_string(),
                         build_repeat_keyboard_with_nav_payments(true))
                    }
                    PendingPaymentStep::AwaitingConfirm => {
                        ("".to_string(), build_nav_keyboard_payments(false)) // unreachable
                    }
                };
                
                // Send new message and capture ID
                match send_step_message(bot.clone(), message.chat.id, &text, kb).await {
                    Ok(sent_msg) => {
                        st.current_bot_message_id = Some(sent_msg.id.0);
                        bot_deps.scheduled_payments.put_pending(key, &st)?;
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
                .text("ℹ️ No pending payment to navigate")
                .await?;
        }
    } else if data.starts_with("schedpay_toggle:") {
        let id = data.split(':').nth(1).unwrap_or("");
        if let Some(mut rec) = bot_deps.scheduled_payments.get_schedule(id) {
            // Only the creator can toggle their own scheduled payment
            if rec.creator_user_id != user.id.0 as i64 {
                bot.answer_callback_query(query.id)
                    .text("❌ Only the creator can toggle this payment")
                    .await?;
                return Ok(());
            }
            rec.active = !rec.active;
            let _ = bot_deps.scheduled_payments.put_schedule(&rec);
            bot.answer_callback_query(query.id)
                .text(if rec.active {
                    "▶️ Resumed"
                } else {
                    "⏸ Paused"
                })
                .await?;
        } else {
            // Schedule not found - still respond to prevent UI hang
            bot.answer_callback_query(query.id)
                .text("ℹ️ Scheduled payment not found")
                .await?;
        }
    } else if data.starts_with("schedpay_delete:") {
        let id = data.split(':').nth(1).unwrap_or("");
        if let Some(mut rec) = bot_deps.scheduled_payments.get_schedule(id) {
            // Only the creator can delete their own scheduled payment
            if rec.creator_user_id != user.id.0 as i64 {
                bot.answer_callback_query(query.id)
                    .text("❌ Only the creator can delete this payment")
                    .await?;
                return Ok(());
            }
            rec.active = false;
            let _ = bot_deps.scheduled_payments.put_schedule(&rec);
            bot.answer_callback_query(query.id)
                .text("🗑 Deleted")
                .await?;
            if let Some(teloxide::types::MaybeInaccessibleMessage::Regular(m)) = &query.message {
                let _ = bot.delete_message(m.chat.id, m.id).await;
            }
        } else {
            // Schedule not found - still respond to prevent UI hang
            bot.answer_callback_query(query.id)
                .text("ℹ️ Scheduled payment not found")
                .await?;
        }
    } else if data.starts_with("schedpay_edit:") {
        // Creator-only edit: present submenu and open scoped wizard
        let id = data.split(':').nth(1).unwrap_or("");
        if let Some(rec) = bot_deps.scheduled_payments.get_schedule(id) {
            if rec.creator_user_id != query.from.id.0 as i64 {
                bot.answer_callback_query(query.id)
                    .text("❌ Only the creator can edit")
                    .await?;
                return Ok(());
            }
            let st = crate::scheduled_payments::dto::PendingPaymentWizardState {
                group_id: rec.group_id,
                creator_user_id: rec.creator_user_id,
                creator_username: rec.creator_username.clone(),
                step: crate::scheduled_payments::dto::PendingPaymentStep::AwaitingRecipient,
                schedule_id: Some(rec.id.clone()),
                recipient_username: rec.recipient_username.clone(),
                recipient_address: rec.recipient_address.clone(),
                symbol: rec.symbol.clone(),
                token_type: rec.token_type.clone(),
                decimals: rec.decimals,
                amount_display: rec
                    .amount_smallest_units
                    .and_then(|v| rec.decimals.map(|d| v as f64 / 10f64.powi(d as i32))),
                date: rec.start_timestamp_utc.and_then(|ts| {
                    chrono::DateTime::<chrono::Utc>::from_timestamp(ts, 0)
                        .map(|dt| dt.format("%Y-%m-%d").to_string())
                }),
                hour_utc: rec.start_timestamp_utc.and_then(|ts| {
                    chrono::DateTime::<chrono::Utc>::from_timestamp(ts, 0).map(|dt| dt.hour() as u8)
                }),
                minute_utc: rec.start_timestamp_utc.and_then(|ts| {
                    chrono::DateTime::<chrono::Utc>::from_timestamp(ts, 0)
                        .map(|dt| dt.minute() as u8)
                }),
                repeat: Some(rec.repeat.clone()),
                weekly_weeks: rec.weekly_weeks,
                current_bot_message_id: None,
                user_message_ids: Vec::new(),
            };
            bot_deps
                .scheduled_payments
                .put_pending((&st.group_id, &st.creator_user_id), &st)?;
            bot.answer_callback_query(query.id).await?;
            if let Some(teloxide::types::MaybeInaccessibleMessage::Regular(m)) = &query.message {
                use teloxide::types::{InlineKeyboardButton as Btn, InlineKeyboardMarkup as Kb};
                let kb = Kb::new(vec![
                    vec![
                        Btn::callback("👤 Recipient", format!("schedpay_editrecipient:{}", id)),
                        Btn::callback("💳 Token", format!("schedpay_edittoken:{}", id)),
                    ],
                    vec![
                        Btn::callback("💰 Amount", format!("schedpay_editamount:{}", id)),
                        Btn::callback("🗓 Date/Time", format!("schedpay_editdate:{}", id)),
                    ],
                    vec![Btn::callback(
                        "🔁 Repeat",
                        format!("schedpay_editrepeat:{}", id),
                    )],
                ]);
                bot.edit_message_text(m.chat.id, m.id, "✏️ What would you like to edit?")
                    .reply_markup(kb)
                    .await?;
            }
        }
    } else if data.starts_with("schedpay_editrecipient:") {
        let id = data.split(':').nth(1).unwrap_or("");
        if let Some(rec) = bot_deps.scheduled_payments.get_schedule(id) {
            let key = (&rec.group_id, &rec.creator_user_id);
            if let Some(mut st) = bot_deps.scheduled_payments.get_pending(key) {
                st.step = crate::scheduled_payments::dto::PendingPaymentStep::AwaitingRecipient;
                bot_deps.scheduled_payments.put_pending(key, &st)?;
            }
            if let Some(teloxide::types::MaybeInaccessibleMessage::Regular(m)) = &query.message {
                bot.edit_message_text(m.chat.id, m.id, "👤 Send the new @recipient username")
                    .await?;
            }
            bot.answer_callback_query(query.id).await?;
        }
    } else if data.starts_with("schedpay_edittoken:") {
        let id = data.split(':').nth(1).unwrap_or("");
        if let Some(rec) = bot_deps.scheduled_payments.get_schedule(id) {
            let key = (&rec.group_id, &rec.creator_user_id);
            if let Some(mut st) = bot_deps.scheduled_payments.get_pending(key) {
                st.step = crate::scheduled_payments::dto::PendingPaymentStep::AwaitingToken;
                bot_deps.scheduled_payments.put_pending(key, &st)?;
            }
            if let Some(teloxide::types::MaybeInaccessibleMessage::Regular(m)) = &query.message {
                bot.edit_message_text(m.chat.id, m.id, "💳 Send token symbol (e.g., APT, USDC)")
                    .await?;
            }
            bot.answer_callback_query(query.id).await?;
        }
    } else if data.starts_with("schedpay_editamount:") {
        let id = data.split(':').nth(1).unwrap_or("");
        if let Some(rec) = bot_deps.scheduled_payments.get_schedule(id) {
            let key = (&rec.group_id, &rec.creator_user_id);
            if let Some(mut st) = bot_deps.scheduled_payments.get_pending(key) {
                st.step = crate::scheduled_payments::dto::PendingPaymentStep::AwaitingAmount;
                bot_deps.scheduled_payments.put_pending(key, &st)?;
            }
            if let Some(teloxide::types::MaybeInaccessibleMessage::Regular(m)) = &query.message {
                bot.edit_message_text(m.chat.id, m.id, "💰 Send new amount")
                    .await?;
            }
            bot.answer_callback_query(query.id).await?;
        }
    } else if data.starts_with("schedpay_editdate:") {
        let id = data.split(':').nth(1).unwrap_or("");
        if let Some(rec) = bot_deps.scheduled_payments.get_schedule(id) {
            let key = (&rec.group_id, &rec.creator_user_id);
            if let Some(mut st) = bot_deps.scheduled_payments.get_pending(key) {
                st.step = crate::scheduled_payments::dto::PendingPaymentStep::AwaitingDate;
                bot_deps.scheduled_payments.put_pending(key, &st)?;
            }
            if let Some(teloxide::types::MaybeInaccessibleMessage::Regular(m)) = &query.message {
                bot.edit_message_text(m.chat.id, m.id, "📅 Send new date YYYY-MM-DD (UTC)")
                    .await?;
            }
            bot.answer_callback_query(query.id).await?;
        }
    } else if data.starts_with("schedpay_editrepeat:") {
        let id = data.split(':').nth(1).unwrap_or("");
        if let Some(rec) = bot_deps.scheduled_payments.get_schedule(id) {
            let key = (&rec.group_id, &rec.creator_user_id);
            if let Some(mut st) = bot_deps.scheduled_payments.get_pending(key) {
                st.step = crate::scheduled_payments::dto::PendingPaymentStep::AwaitingRepeat;
                bot_deps.scheduled_payments.put_pending(key, &st)?;
            }
            if let Some(teloxide::types::MaybeInaccessibleMessage::Regular(m)) = &query.message {
                bot.edit_message_text(m.chat.id, m.id, "🔁 Select new repeat interval")
                    .reply_markup(build_repeat_keyboard_payments())
                    .await?;
            }
            bot.answer_callback_query(query.id).await?;
        }
    } else if data.starts_with("schedpay_runnow:") {
        let id = data.split(':').nth(1).unwrap_or("");
        if let Some(mut rec) = bot_deps.scheduled_payments.get_schedule(id) {
            // Only the creator can run their own scheduled payment immediately
            if rec.creator_user_id != user.id.0 as i64 {
                bot.answer_callback_query(query.id)
                    .text("❌ Only the creator can run this payment")
                    .await?;
                return Ok(());
            }
            // Set due now and let runner pick it up on next tick
            rec.next_run_at = Some(Utc::now().timestamp());
            let _ = bot_deps.scheduled_payments.put_schedule(&rec);
            bot.answer_callback_query(query.id)
                .text("⚡ Queued to run")
                .await?;
        } else {
            // Schedule not found - still respond to prevent UI hang
            bot.answer_callback_query(query.id)
                .text("ℹ️ Scheduled payment not found")
                .await?;
        }
    } else if data.starts_with("schedpay_close:") {
        let id = data.split(':').nth(1).unwrap_or("");
        if !id.is_empty() {
            // ID provided - validate creator and close specific payment display
            if let Some(rec) = bot_deps.scheduled_payments.get_schedule(id) {
                // Only the creator can close their own scheduled payment display
                if rec.creator_user_id != user.id.0 as i64 {
                    bot.answer_callback_query(query.id)
                        .text("❌ Only the creator can close this display")
                        .await?;
                    return Ok(());
                }
                bot.answer_callback_query(query.id).text("Closed").await?;
                if let Some(teloxide::types::MaybeInaccessibleMessage::Regular(m)) = &query.message
                {
                    let _ = bot.edit_message_reply_markup(m.chat.id, m.id).await;
                }
            } else {
                // Schedule not found - still respond to prevent UI hang
                bot.answer_callback_query(query.id)
                    .text("ℹ️ Scheduled payment not found")
                    .await?;
            }
        } else {
            // No ID provided - just close the current message markup (for backward compatibility)
            bot.answer_callback_query(query.id).text("Closed").await?;
            if let Some(teloxide::types::MaybeInaccessibleMessage::Regular(m)) = &query.message {
                let _ = bot.edit_message_reply_markup(m.chat.id, m.id).await;
            }
        }
    }

    Ok(())
}
