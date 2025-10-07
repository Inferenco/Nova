use chrono::Utc;
use teloxide::{prelude::*, types::ChatId};
use tokio_cron_scheduler::Job;

use crate::dependencies::BotDependencies;
use crate::scheduled_payments::dto::ScheduledPaymentRecord;
use crate::scheduled_payments::storage::ScheduledPaymentsStorage;
use crate::scheduled_prompts::dto::RepeatPolicy;

fn next_week_cadence(now_ts: i64, weeks: u8) -> i64 {
    let days = (weeks as i64) * 7;
    now_ts + days * 24 * 3600
}

pub async fn register_all_schedules(bot: Bot, bot_deps: BotDependencies) -> anyhow::Result<()> {
    let storage = ScheduledPaymentsStorage::new(&bot_deps.db)?;
    for item in storage.scheduled.iter() {
        if let Ok((_, ivec)) = item {
            if let Ok(mut rec) = serde_json::from_slice::<ScheduledPaymentRecord>(&ivec) {
                if rec.active {
                    if let Err(e) = register_schedule(bot.clone(), bot_deps.clone(), &mut rec).await
                    {
                        log::error!("Failed to register payment schedule {}: {}", rec.id, e);
                    }
                    if let Err(e) = storage.put_schedule(&rec) {
                        log::warn!(
                            "Failed to persist payment schedule {} after register: {}",
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
    record: &mut ScheduledPaymentRecord,
) -> anyhow::Result<()> {
    let scheduler = bot_deps.scheduler.clone();
    let schedule_id = record.id.clone();
    let group_chat_id = ChatId(record.group_id as i64);

    if record.next_run_at.is_none() {
        record.next_run_at = record.start_timestamp_utc;
    }

    let job = Job::new_async("0 * * * * *", move |_uuid, _l| {
        let bot = bot.clone();
        let bot_deps = bot_deps.clone();
        let schedule_id = schedule_id.clone();
        let group_chat_id = group_chat_id;
        Box::pin(async move {
            let storage = bot_deps.scheduled_payments.clone();
            let mut rec = match storage.get_schedule(&schedule_id) {
                Some(r) => r,
                None => return,
            };
            if !rec.active {
                return;
            }

            let now_ts = Utc::now().timestamp();
            if let Some(lock) = rec.locked_until {
                if now_ts < lock {
                    return;
                }
            }

            if let Some(next_at) = rec.next_run_at {
                if now_ts < next_at {
                    return;
                }
            }

            // Lock
            rec.locked_until = Some(now_ts + 120);
            let _ = storage.put_schedule(&rec);

            // Execute payment via service.pay_members
            let result = (|| async {
                let group_credentials = match bot_deps.group.get_credentials(group_chat_id) {
                    Some(c) => c,
                    None => return Err(anyhow::anyhow!("Group credentials not found")),
                };

                // Validate critical payment data before proceeding
                let amount = match rec.amount_smallest_units {
                    Some(amt) => {
                        if amt > 0 {
                            amt
                        } else {
                            return Err(anyhow::anyhow!("Scheduled payment amount cannot be zero"));
                        }
                    }
                    None => return Err(anyhow::anyhow!("Scheduled payment amount is missing")),
                };

                let coin_type = match &rec.token_type {
                    Some(token) if !token.is_empty() => token.clone(),
                    Some(_) => {
                        return Err(anyhow::anyhow!("Scheduled payment token type is empty"));
                    }
                    None => return Err(anyhow::anyhow!("Scheduled payment token type is missing")),
                };

                let recipient_address = match &rec.recipient_address {
                    Some(addr) if !addr.is_empty() => addr.clone(),
                    Some(_) => {
                        return Err(anyhow::anyhow!(
                            "Scheduled payment recipient address is empty"
                        ));
                    }
                    None => {
                        return Err(anyhow::anyhow!(
                            "Scheduled payment recipient address is missing"
                        ));
                    }
                };

                let token = group_credentials.jwt;
                let version = if coin_type.contains("::") {
                    quark_core::helpers::dto::CoinVersion::V1
                } else {
                    quark_core::helpers::dto::CoinVersion::V2
                };
                let users = vec![recipient_address];
                let payload = quark_core::helpers::dto::PayUsersRequest {
                    amount,
                    users,
                    coin_type,
                    version,
                };
                bot_deps.service.pay_members(token, payload).await
            })()
            .await;

            match result {
                Ok(resp) => {
                    rec.last_attempt_status = Some("success".to_string());
                    rec.last_error = None;
                    rec.last_run_at = Some(now_ts);
                    rec.run_count += 1;
                    // Compute next occurrence
                    let weeks = rec.weekly_weeks.unwrap_or(1);
                    rec.next_run_at = match rec.repeat {
                        RepeatPolicy::Daily => Some(now_ts + 24 * 3600),
                        RepeatPolicy::Weekly => Some(next_week_cadence(now_ts, weeks)),
                        _ => Some(next_week_cadence(now_ts, weeks)),
                    };
                    rec.locked_until = None;
                    let _ = storage.put_schedule(&rec);
                    if rec.notify_on_success {
                        let network = std::env::var("APTOS_NETWORK")
                            .unwrap_or_else(|_| "mainnet".to_string())
                            .to_lowercase();
                        let hash = resp.hash;
                        let amount_smallest = rec.amount_smallest_units.unwrap_or(0);
                        let decimals = rec.decimals.unwrap_or(8) as i32;
                        let human_amount = (amount_smallest as f64) / 10f64.powi(decimals);
                        let symbol = rec.symbol.as_deref().unwrap_or("Unknown");
                        let recipient_username =
                            rec.recipient_username.as_deref().unwrap_or("Unknown");
                        let text = format!(
                            "✅ Payment sent\nAmount: {:.4} {}\nTo: @{}\nSchedule: {}\n🔗 Explorer: https://explorer.aptoslabs.com/txn/{}?network={}",
                            human_amount, symbol, recipient_username, rec.id, hash, network
                        );
                        if let Err(e) = bot
                            .send_message(ChatId(rec.creator_user_id), text.clone())
                            .await
                        {
                            // DM failed -> optional group fallback
                            let _ = bot
                                .send_message(
                                    group_chat_id,
                                    format!("{}\n(tag: @{})", text, rec.creator_username),
                                )
                                .await;
                            log::warn!("Failed to DM creator: {}", e);
                        }
                    }
                }
                Err(e) => {
                    rec.last_attempt_status = Some("failure".to_string());
                    rec.last_error = Some(e.to_string());
                    rec.locked_until = None;
                    let _ = storage.put_schedule(&rec);
                    if rec.notify_on_failure {
                        use teloxide::types::InlineKeyboardButton as Btn;
                        use teloxide::types::InlineKeyboardMarkup as Kb;
                        let kb = Kb::new(vec![
                            vec![Btn::callback(
                                "🔁 Retry now",
                                format!("schedpay_runnow:{}", rec.id),
                            )],
                            vec![Btn::callback(
                                "⏸ Pause",
                                format!("schedpay_toggle:{}", rec.id),
                            )],
                        ]);
                        if let Err(err) = bot
                            .send_message(
                                ChatId(rec.creator_user_id),
                                format!("❌ Payment failed: {}", e),
                            )
                            .reply_markup(kb.clone())
                            .await
                        {
                            let _ = bot
                                .send_message(group_chat_id, "❌ Scheduled payment failed (unable to DM). Use /listscheduledpayments for actions.")
                                .await;
                            log::warn!("Failed to DM creator: {}", err);
                        }
                    }
                }
            }
        })
    })?;

    let id = scheduler.add(job).await?;
    record.scheduler_job_id = Some(id.to_string());
    Ok(())
}
