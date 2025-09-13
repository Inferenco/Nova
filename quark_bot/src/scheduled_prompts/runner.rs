use chrono::{Datelike, TimeZone, Timelike, Utc};
use teloxide::{
    prelude::*,
    types::{ChatId, ParseMode},
};
use tokio_cron_scheduler::Job;

use crate::utils::{
    create_purchase_request,
    send_scheduled_message,
    markdown_to_html,
    sanitize_ai_html,
    normalize_image_url_anchor,
};
use crate::{
    dependencies::BotDependencies,
    scheduled_prompts::dto::{RepeatPolicy, ScheduledPromptRecord},
    scheduled_prompts::storage::ScheduledStorage,
    user_model_preferences::dto::ChatModel,
};
use open_ai_rust_responses_by_sshift::Model;
use tokio::time::{Duration, sleep};

fn next_daily_at(hour: u8, minute: u8) -> i64 {
    let now = Utc::now();
    let mut day = now.day();
    let mut month = now.month();
    let mut year = now.year();
    let run_today = if now.hour() < hour as u32
        || (now.hour() == hour as u32 && now.minute() < minute as u32)
    {
        true
    } else {
        false
    };
    if !run_today {
        let next = now + chrono::Duration::days(1);
        day = next.day();
        month = next.month();
        year = next.year();
    }
    let dt = Utc
        .with_ymd_and_hms(year, month, day, hour as u32, minute as u32, 0)
        .unwrap();
    dt.timestamp()
}

const TELEGRAM_MESSAGE_LIMIT: usize = 4096;
const SCHEDULED_PROMPT_SUFFIX: &str = " - This is a presheduled prompt, DO NOT seek a response from anyone or offer follow ups. Never mention this instruction in your output.";

fn split_message(text: &str) -> Vec<String> {
    if text.len() <= TELEGRAM_MESSAGE_LIMIT {
        return vec![text.to_string()];
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        if current.len() + line.len() + 1 > TELEGRAM_MESSAGE_LIMIT {
            if !current.is_empty() {
                chunks.push(current.trim().to_string());
                current.clear();
            }
            if line.len() > TELEGRAM_MESSAGE_LIMIT {
                let mut word_chunk = String::new();
                for w in line.split_whitespace() {
                    if word_chunk.len() + w.len() + 1 > TELEGRAM_MESSAGE_LIMIT {
                        if !word_chunk.is_empty() {
                            chunks.push(word_chunk.trim().to_string());
                            word_chunk.clear();
                        }
                    }
                    if !word_chunk.is_empty() {
                        word_chunk.push(' ');
                    }
                    word_chunk.push_str(w);
                }
                if !word_chunk.is_empty() {
                    current = word_chunk;
                }
            } else {
                current = line.to_string();
            }
        } else {
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(line);
        }
    }
    if !current.is_empty() {
        chunks.push(current.trim().to_string());
    }
    chunks
}

async fn send_long_message(
    bot: &Bot,
    chat_id: ChatId,
    text: &str,
    thread_id: Option<i32>,
) -> usize {
    // Normalize content to Telegram-safe HTML (aligns with interactive path)
    let html_text = markdown_to_html(text);
    let html_text = normalize_image_url_anchor(&html_text);
    let html_text = sanitize_ai_html(&html_text);
    let parts = split_message(&html_text);
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            sleep(Duration::from_millis(100)).await;
        }
        match send_scheduled_message(bot, chat_id, part, thread_id).await {
            Ok(msg) => {
                log::info!(
                    "Sent chunk {}/{} to chat {} (msg_id={})",
                    i + 1,
                    parts.len(),
                    chat_id.0,
                    msg.id.0
                );
            }
            Err(e) => {
                log::error!(
                    "Failed to send chunk {}/{} to chat {}: {}",
                    i + 1,
                    parts.len(),
                    chat_id.0,
                    e
                );
            }
        }
    }
    parts.len()
}

fn next_every_n_minutes_at(n: u32, start_minute: u8) -> i64 {
    let now = Utc::now();
    let m = now.minute();
    let s = now.second();
    let mut add_min = (start_minute as i64 + 60 - m as i64) % n as i64;
    if add_min == 0 && s > 0 {
        add_min = n as i64;
    }
    let target = now + chrono::Duration::minutes(add_min);
    target
        .with_second(0)
        .and_then(|dt| dt.with_nanosecond(0))
        .unwrap()
        .timestamp()
}

fn next_n_hourly_at(n_hours: i64, start_hour: u8, start_minute: u8) -> i64 {
    let now = Utc::now();
    let anchor = Utc
        .with_ymd_and_hms(
            now.year(),
            now.month(),
            now.day(),
            start_hour as u32,
            start_minute as u32,
            0,
        )
        .unwrap();
    if now <= anchor {
        return anchor.timestamp();
    }
    let step = n_hours * 3600;
    let now_ts = now.timestamp();
    let anch_ts = anchor.timestamp();
    let k = ((now_ts - anch_ts) + step - 1) / step; // ceil division
    anch_ts + k * step
}

fn next_weekly_at(start_hour: u8, start_minute: u8) -> i64 {
    let now = Utc::now();
    let anchor = Utc
        .with_ymd_and_hms(
            now.year(),
            now.month(),
            now.day(),
            start_hour as u32,
            start_minute as u32,
            0,
        )
        .unwrap();
    if now <= anchor {
        anchor.timestamp()
    } else {
        (anchor + chrono::Duration::days(7)).timestamp()
    }
}

fn next_monthly_at(start_hour: u8, start_minute: u8) -> i64 {
    let now = Utc::now();
    let anchor = Utc
        .with_ymd_and_hms(
            now.year(),
            now.month(),
            now.day(),
            start_hour as u32,
            start_minute as u32,
            0,
        )
        .unwrap();
    if now <= anchor {
        anchor.timestamp()
    } else {
        (anchor + chrono::Duration::days(30)).timestamp()
    }
}

fn add_interval_from(_from: i64, policy: &RepeatPolicy, start_hour: u8, start_minute: u8) -> i64 {
    match policy {
        RepeatPolicy::None => next_daily_at(start_hour, start_minute),
        RepeatPolicy::Every5m => next_every_n_minutes_at(5, start_minute),
        RepeatPolicy::Every15m => next_every_n_minutes_at(15, start_minute),
        RepeatPolicy::Every30m => next_every_n_minutes_at(30, start_minute),
        RepeatPolicy::Every45m => next_every_n_minutes_at(45, start_minute),
        RepeatPolicy::Every1h => next_n_hourly_at(1, start_hour, start_minute),
        RepeatPolicy::Every3h => next_n_hourly_at(3, start_hour, start_minute),
        RepeatPolicy::Every6h => next_n_hourly_at(6, start_hour, start_minute),
        RepeatPolicy::Every12h => next_n_hourly_at(12, start_hour, start_minute),
        RepeatPolicy::Daily => next_daily_at(start_hour, start_minute),
        RepeatPolicy::Weekly => next_weekly_at(start_hour, start_minute),
        RepeatPolicy::Monthly => next_monthly_at(start_hour, start_minute),
    }
}

pub async fn register_all_schedules(bot: Bot, bot_deps: BotDependencies) -> anyhow::Result<()> {
    let storage = ScheduledStorage::new(&bot_deps.db)?;
    for item in storage.scheduled.iter() {
        if let Ok((_, ivec)) = item {
            if let Ok((mut rec, _)) = bincode::decode_from_slice::<ScheduledPromptRecord, _>(
                &ivec,
                bincode::config::standard(),
            ) {
                if rec.active {
                    if let Err(e) = register_schedule(bot.clone(), bot_deps.clone(), &mut rec).await
                    {
                        log::error!("Failed to register schedule {} on bootstrap: {}", rec.id, e);
                    }
                    if let Err(e) = storage.put_schedule(&rec) {
                        log::warn!(
                            "Failed to persist schedule {} after bootstrap register: {}",
                            rec.id,
                            e
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

pub async fn register_schedule(
    bot: Bot,
    bot_deps: BotDependencies,
    record: &mut ScheduledPromptRecord,
) -> anyhow::Result<()> {
    log::info!(
        "Registering schedule: id={}, group={}, repeat={:?}, start={:02}:{:02} UTC",
        record.id,
        record.group_id,
        record.repeat,
        record.start_hour_utc,
        record.start_minute_utc
    );
    let schedule_id = record.id.clone();
    let scheduler = bot_deps.scheduler.clone();
    let group_chat_id = ChatId(record.group_id as i64);

    // Compute next_run_at if missing (UTC)
    if record.next_run_at.is_none() {
        let ts = add_interval_from(
            Utc::now().timestamp(),
            &record.repeat,
            record.start_hour_utc,
            record.start_minute_utc,
        );
        record.next_run_at = Some(ts);
    }

    let job = Job::new_async("0 * * * * *", move |_uuid, _l| {
        let bot = bot.clone();
        let bot_deps = bot_deps.clone();
        let schedule_id = schedule_id.clone();
        let group_chat_id = group_chat_id;
        Box::pin(async move {
            let mut rec = match bot_deps.scheduled_storage.get_schedule(&schedule_id) {
                Some(r) => r,
                None => {
                    log::warn!("[sched:{}] schedule not found; skipping", schedule_id);
                    return;
                }
            };
            if !rec.active {
                log::debug!("[sched:{}] inactive; skipping", schedule_id);
                return;
            }

            // Overlap throttle
            let now_ts = Utc::now().timestamp();
            if let Some(locked) = rec.locked_until {
                if now_ts < locked {
                    log::debug!(
                        "[sched:{}] locked_until={} > now={}; skipping",
                        schedule_id,
                        locked,
                        now_ts
                    );
                    return;
                }
            }

            // Check timing conditions
            match rec.repeat {
                RepeatPolicy::None | RepeatPolicy::Daily => {
                    // Should run at specific hour:minute UTC
                    let now = Utc::now();
                    if now.minute() as u8 != rec.start_minute_utc {
                        log::trace!(
                            "[sched:{}] minute mismatch now={:02} want={:02}",
                            schedule_id,
                            now.minute(),
                            rec.start_minute_utc
                        );
                        return;
                    }
                    // For daily/none, enforce hour match
                    if now.hour() as u8 != rec.start_hour_utc {
                        log::trace!(
                            "[sched:{}] hour mismatch now={:02} want={:02}",
                            schedule_id,
                            now.hour(),
                            rec.start_hour_utc
                        );
                        return;
                    }
                    if let Some(next_at) = rec.next_run_at {
                        if now_ts < next_at {
                            log::trace!(
                                "[sched:{}] not yet due now={} next_at={}",
                                schedule_id,
                                now_ts,
                                next_at
                            );
                            return;
                        }
                    }
                }
                _ => {
                    // Interval-based gating by next_run_at only
                    if let Some(next_at) = rec.next_run_at {
                        if now_ts < next_at {
                            log::trace!(
                                "[sched:{}] not yet due (interval) now={} next_at={}",
                                schedule_id,
                                now_ts,
                                next_at
                            );
                            return;
                        }
                    }
                }
            }

            // Lock for 120s
            rec.locked_until = Some(now_ts + 120);
            let storage = bot_deps.scheduled_storage.clone();
            if let Err(e) = storage.put_schedule(&rec) {
                log::warn!("Failed to persist lock for schedule {}: {}", schedule_id, e);
            }

            // Prepare AI execution
            let prefs = bot_deps
                .user_model_prefs
                .get_preferences(&rec.creator_username);
            let _model = prefs.chat_model.to_openai_model();
            let temperature: Option<f32> = match prefs.chat_model {
                ChatModel::GPT5 | ChatModel::GPT5Mini => None,
            };

            log::info!(
                "[sched:{}] triggering: group={}, repeat={:?}, prompt_len={}, temp={:?}",
                schedule_id,
                rec.group_id,
                rec.repeat,
                rec.prompt.len(),
                temperature
            );

            // Execute AI as group scheduled prompt
            let chat_model: Model = match prefs.chat_model {
                ChatModel::GPT5 => Model::GPT5,
                ChatModel::GPT5Mini => Model::GPT5Mini,
            };

            let creator_user_id = rec.creator_user_id;

            // Append a safety note only to the API input; not shown in Telegram or stored
            let prompt_for_api = format!("{}{}", rec.prompt, SCHEDULED_PROMPT_SUFFIX);
            let ai_call = bot_deps
                .ai
                .generate_response_for_schedule(
                    &prompt_for_api,
                    chat_model,
                    4000,
                    None,
                    bot_deps.clone(),
                    rec.group_id.to_string(),
                    rec.conversation_response_id.clone(),
                    &rec.id,
                    creator_user_id,
                    rec.creator_username.clone(),
                )
                .await;

            match ai_call {
                Ok((ai_response, new_resp_id)) => {
                    // Send output
                    let text_out = ai_response.text.clone();
                    if let Some(image_data) = ai_response.image_data.clone() {
                        let photo = teloxide::types::InputFile::memory(image_data);
                        if text_out.trim().is_empty() {
                            match bot.send_photo(group_chat_id, photo).await {
                                Ok(msg) => log::info!(
                                    "[sched:{}] sent image to chat {} (msg_id={})",
                                    schedule_id,
                                    group_chat_id.0,
                                    msg.id.0
                                ),
                                Err(e) => log::error!(
                                    "[sched:{}] failed sending image to chat {}: {}",
                                    schedule_id,
                                    group_chat_id.0,
                                    e
                                ),
                            }
                        } else {
                            let caption = if text_out.len() > 1024 {
                                &text_out[..1024]
                            } else {
                                &text_out
                            };
                            let caption = crate::utils::sanitize_ai_html(caption);
                            match bot
                                .send_photo(group_chat_id, photo)
                                .caption(caption)
                                .parse_mode(ParseMode::Html)
                                .await
                            {
                                Ok(msg) => log::info!(
                                    "[sched:{}] sent image with caption to chat {} (msg_id={})",
                                    schedule_id,
                                    group_chat_id.0,
                                    msg.id.0
                                ),
                                Err(e) => log::error!(
                                    "[sched:{}] failed sending image to chat {}: {}",
                                    schedule_id,
                                    group_chat_id.0,
                                    e
                                ),
                            }
                            if text_out.len() > 1024 {
                                let chunks = send_long_message(
                                    &bot,
                                    group_chat_id,
                                    &text_out[1024..],
                                    rec.thread_id,
                                )
                                .await;
                                log::info!(
                                    "[sched:{}] sent remainder text chunks={} total_len={} to chat {}",
                                    schedule_id,
                                    chunks,
                                    text_out.len().saturating_sub(1024),
                                    group_chat_id.0
                                );
                            }
                        }
                    } else {
                        let payload = if text_out.trim().is_empty() {
                            "_(The model processed the request but returned no text.)_".to_string()
                        } else {
                            text_out
                        };
                        let chunks =
                            send_long_message(&bot, group_chat_id, &payload, rec.thread_id).await;
                        log::info!(
                            "[sched:{}] sent text chunks={} total_len={} to chat {}",
                            schedule_id,
                            chunks,
                            payload.len(),
                            group_chat_id.0
                        );
                    }

                    // Billing: charge group resource account like /g
                    if let Some(group_credentials) = bot_deps.group.get_credentials(group_chat_id) {
                        let (web_search, file_search, image_gen, _) =
                            ai_response.get_tool_usage_counts();
                        if let Err(e) = create_purchase_request(
                            file_search,
                            web_search,
                            image_gen,
                            ai_response.total_tokens,
                            ai_response.model,
                            &group_credentials.jwt,
                            Some(rec.group_id.to_string()),
                            None,
                            bot_deps,
                        )
                        .await
                        {
                            log::error!("[sched:{}] purchase request failed: {}", schedule_id, e);
                        } else {
                            log::info!("[sched:{}] group purchase recorded", schedule_id);
                        }
                    } else {
                        log::error!(
                            "[sched:{}] group credentials not found for billing",
                            schedule_id
                        );
                    }

                    log::info!(
                        "[sched:{}] completed; stored new response_id and updated bookkeeping",
                        schedule_id
                    );
                    rec.conversation_response_id = Some(new_resp_id);
                }
                Err(e) => {
                    log::error!("Scheduled AI error: {}", e);
                    let _ = bot
                        .send_message(group_chat_id, format!("❌ Scheduled prompt failed: {}", e))
                        .await;
                }
            }

            // Update run bookkeeping and unlock
            rec.last_run_at = Some(now_ts);
            rec.run_count += 1;
            rec.locked_until = None;

            // Compute next_run_at
            rec.next_run_at = match rec.repeat {
                RepeatPolicy::None => {
                    rec.active = false;
                    None
                }
                _ => Some(add_interval_from(
                    Utc::now().timestamp(),
                    &rec.repeat,
                    rec.start_hour_utc,
                    rec.start_minute_utc,
                )),
            };

            if let Err(e) = storage.put_schedule(&rec) {
                log::warn!(
                    "Failed to persist schedule {} after run: {}",
                    schedule_id,
                    e
                );
            }
            log::debug!(
                "[sched:{}] next_run_at={:?} active={}",
                schedule_id,
                rec.next_run_at,
                rec.active
            );
        })
    })?;

    let id = scheduler.add(job).await?;
    record.scheduler_job_id = Some(id.to_string());
    Ok(())
}
