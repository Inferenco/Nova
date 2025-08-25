use anyhow::Result;
use chrono::Utc;
use open_ai_rust_responses_by_sshift::Model;
use teloxide::{
    prelude::*,
    types::{InlineKeyboardMarkup, Message, ParseMode, User},
};
use uuid::Uuid;

use crate::{
    dependencies::BotDependencies,
    scheduled_prompts::{
        dto::{PendingStep, PendingWizardState, RepeatPolicy, ScheduledPromptRecord},
        helpers::{build_hours_keyboard, summarize},
        runner::{register_all_schedules, register_schedule},
    },
    utils::create_purchase_request,
};

pub async fn bootstrap_scheduled_prompts(bot: Bot, bot_deps: BotDependencies) -> Result<()> {
    register_all_schedules(bot, bot_deps).await
}

pub async fn handle_scheduleprompt_command(
    bot: Bot,
    msg: Message,
    bot_deps: BotDependencies,
) -> Result<()> {
    if !msg.chat.is_group() && !msg.chat.is_supergroup() {
        bot.send_message(msg.chat.id, "❌ This command is only available in groups.")
            .await?;
        return Ok(());
    }

    // Admin check
    let admins = bot.get_chat_administrators(msg.chat.id).await?;
    let user = match msg.from {
        Some(u) => u,
        None => {
            return Ok(());
        }
    };
    if !admins.iter().any(|m| m.user.id == user.id) {
        bot.send_message(msg.chat.id, "❌ Only administrators can use this command.")
            .await?;
        return Ok(());
    }

    let username = match user.username.clone() {
        Some(u) => u,
        None => {
            bot.send_message(msg.chat.id, "❌ Username required to schedule prompts.")
                .await?;
            return Ok(());
        }
    };

    let state = PendingWizardState {
        group_id: msg.chat.id.0 as i64,
        creator_user_id: user.id.0 as i64,
        creator_username: username,
        step: PendingStep::AwaitingPrompt,
        prompt: None,
        hour_utc: None,
        minute_utc: None,
        repeat: None,
    };
    bot_deps
        .scheduled_storage
        .put_pending((&state.group_id, &state.creator_user_id), &state)?;

    let note = "\n\nℹ️ Note about tools for scheduled prompts:\n\n• Unavailable: any tool that requires user confirmation or performs transactions (e.g., pay users, withdrawals, funding, creating proposals or other interactive flows).\n\nTip: Schedule informational queries, summaries, monitoring, or analytics. Avoid actions that need real-time human approval.";

    bot.send_message(
        msg.chat.id,
        format!(
            "📝 Send the prompt you want to schedule — you can <b>reply to this message</b> or just <b>send it as your next message</b>.{}\n\nIf your prompt is rejected for using a forbidden action, <b>try again</b> with a safer prompt.",
            note
        ),
    )
    .parse_mode(ParseMode::Html)
    .await?;
    Ok(())
}

pub async fn handle_listscheduled_command(
    bot: Bot,
    msg: Message,
    bot_deps: BotDependencies,
) -> Result<()> {
    // Admin check
    let admins = bot.get_chat_administrators(msg.chat.id).await?;
    let user = match msg.from {
        Some(u) => u,
        None => {
            return Ok(());
        }
    };
    if !admins.iter().any(|m| m.user.id == user.id) {
        bot.send_message(msg.chat.id, "❌ Only administrators can use this command.")
            .await?;
        return Ok(());
    }

    let list = bot_deps
        .scheduled_storage
        .list_schedules_for_group(msg.chat.id.0 as i64);

    if list.is_empty() {
        bot.send_message(msg.chat.id, "📭 No active scheduled prompts in this group.")
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
        let kb =
            InlineKeyboardMarkup::new(vec![vec![teloxide::types::InlineKeyboardButton::callback(
                "❌ Cancel".to_string(),
                format!("sched_cancel:{}", rec.id),
            )]]);
        bot.send_message(msg.chat.id, title)
            .reply_markup(kb)
            .await?;
    }

    Ok(())
}

pub async fn finalize_and_register(
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
        bot.send_message(
            ChatId(state.group_id as i64),
            "❌ You already have 10 active scheduled prompts in this group.\n\nPlease cancel one with /listscheduled before adding a new schedule.",
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
    };

    bot_deps.scheduled_storage.put_schedule(&rec)?;
    register_schedule(bot.clone(), bot_deps.clone(), &mut rec).await?;
    bot_deps.scheduled_storage.put_schedule(&rec)?;

    bot.send_message(
        ChatId(rec.group_id as i64),
        format!(
            "✅ Scheduled created!\n\n{}",
            summarize(&PendingWizardState {
                group_id: rec.group_id,
                creator_user_id: rec.creator_user_id,
                creator_username: rec.creator_username,
                step: PendingStep::AwaitingConfirm,
                prompt: Some(rec.prompt),
                hour_utc: Some(rec.start_hour_utc),
                minute_utc: Some(rec.start_minute_utc),
                repeat: Some(rec.repeat),
            })
        ),
    )
    .parse_mode(ParseMode::Html)
    .await?;

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
                                    "❌ This prompt can't be scheduled. PLEASE TRY AGAIN\n\n<b>Reason:</b> {}\n\n<b>Allowed for schedules</b>: informational queries, analytics, web/file search, time, market snapshots, and image generation.\n\n<b>Blocked</b>: payments/transfers, withdrawals/funding, DAO/proposal creation, or any on-chain/interactive actions.\n\nPlease send a new prompt (you can just send it here without replying).",
                                    teloxide::utils::html::escape(&reason)
                                );
                                bot.send_message(msg.chat.id, warn)
                                    .parse_mode(ParseMode::Html)
                                    .await?;
                                // Do not advance wizard; let user try again by sending a new prompt
                                return Ok(true);
                            }
                        }
                        Err(e) => {
                            log::warn!("schedule_guard check failed: {}", e);
                        }
                    }
                }

                st.prompt = Some(text);
                st.step = PendingStep::AwaitingHour;
                if let Err(e) = bot_deps.scheduled_storage.put_pending(key, &st) {
                    log::error!("Failed to persist scheduled wizard state: {}", e);
                    bot.send_message(
                        msg.chat.id,
                        "❌ Error saving schedule state. Please try /scheduleprompt again.",
                    )
                    .await?;
                    return Ok(true);
                }
                let kb = build_hours_keyboard();
                bot.send_message(msg.chat.id, "Select start hour (UTC)")
                    .reply_markup(kb)
                    .await?;
                return Ok(true);
            }
        }
    }

    Ok(false)
}
