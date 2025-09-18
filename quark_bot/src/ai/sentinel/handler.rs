use anyhow::Result as AnyResult;
use open_ai_rust_responses_by_sshift::Model;
use teloxide::{
    prelude::*,
    sugar::request::RequestReplyExt,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, Message, ParseMode},
};

use crate::{
    ai::moderation::dto::ModerationOverrides,
    dependencies::BotDependencies,
    payment::dto::PaymentPrefs,
    utils::{create_purchase_request, send_scheduled_message},
};

pub async fn handle_message_sentinel(
    bot: Bot,
    msg: Message,
    bot_deps: BotDependencies,
    chat_id: String,
) -> AnyResult<bool> {
    let thread_id = msg.thread_id;
    let sentinel_on = bot_deps.sentinel.get_sentinel(chat_id.clone());
    if sentinel_on {
        // Skip moderation if there's an active moderation settings wizard
        if let Some(_) = &msg.from {
            if let Ok(moderation_state) = bot_deps.moderation.get_moderation_state(chat_id.clone())
            {
                if moderation_state.step == "AwaitingAllowed"
                    || moderation_state.step == "AwaitingDisallowed"
                {
                    log::info!(
                        "Sentinel moderation state is {}, skipping moderation",
                        moderation_state.step
                    );
                    return Ok(true);
                }
            }
        }
        // Don't moderate admin or bot messages
        if let Some(user) = &msg.from {
            if user.is_bot {
                return Ok(true);
            }

            // Check admin status
            let admins = bot.get_chat_administrators(msg.chat.id).await?;
            let is_admin = admins.iter().any(|member| member.user.id == user.id);
            if is_admin {
                // Special case: if group is awaiting file uploads and this is a document-only message,
                // let it pass through to the group file upload handler instead of stopping here
                let is_awaiting_files = bot_deps
                    .group_file_upload_state
                    .is_awaiting(chat_id.clone())
                    .await;
                let is_document_only =
                    msg.document().is_some() && msg.text().is_none() && msg.caption().is_none();

                if is_awaiting_files && is_document_only {
                    return Ok(false); // Let other handlers process this message
                }

                return Ok(true);
            }
        }

        let group_credentials = bot_deps.group.get_credentials(msg.chat.id);

        if group_credentials.is_none() {
            return Ok(true);
        }

        let group_credentials = group_credentials.unwrap();

        let address = group_credentials.resource_account_address.clone();

        let default_payment_prefs = bot_deps.default_payment_prefs.clone();

        let coin = bot_deps
            .payment
            .get_payment_token(msg.chat.id.to_string(), &bot_deps)
            .await
            .unwrap_or(PaymentPrefs::from((
                default_payment_prefs.label,
                default_payment_prefs.currency,
                default_payment_prefs.version,
            )));

        let group_balance = bot_deps
            .panora
            .aptos
            .get_account_balance(&address, &coin.currency)
            .await?;

        let token = bot_deps.panora.get_token_by_symbol(&coin.label).await;

        if token.is_err() {
            send_scheduled_message(
                &bot,
                msg.chat.id,
                "❌ Token not found, please contact support",
                if let Some(thread_id) = thread_id {
                    Some(thread_id.0.0)
                } else {
                    None
                },
            )
            .await?;
            return Ok(true);
        }

        let token = token.unwrap();

        let token_price = token.usd_price;

        if token_price.is_none() {
            send_scheduled_message(
                &bot,
                msg.chat.id,
                "❌ Token price not found, please contact support",
                if let Some(thread_id) = thread_id {
                    Some(thread_id.0.0)
                } else {
                    None
                },
            )
            .await?;
            return Ok(true);
        }

        let token_price = token_price.unwrap();

        let token_price = token_price.parse::<f64>();

        if token_price.is_err() {
            send_scheduled_message(
                &bot,
                msg.chat.id,
                "❌ Token price not found, please contact support",
                if let Some(thread_id) = thread_id {
                    Some(thread_id.0.0)
                } else {
                    None
                },
            )
            .await?;
            return Ok(true);
        }

        let token_price = token_price.unwrap();

        let token_decimals = token.decimals;

        let min_deposit = (bot_deps.panora.min_deposit / 10_f64) / token_price;

        let min_deposit = (min_deposit as f64 * 10_f64.powi(token_decimals as i32)) as u64;

        if group_balance < min_deposit as i64 {
            let min_deposit_formatted = format!(
                "{:.2}",
                min_deposit as f64 / 10_f64.powi(token_decimals as i32)
            );

            let group_balance_formatted = format!(
                "{:.2}",
                group_balance as f64 / 10_f64.powi(token_decimals as i32)
            );

            let text = format!(
                "User balance is less than the minimum deposit. Please fund your account transfering {} to <code>{}</code> address. Minimum deposit: {} {} (Your balance: {} {})",
                token.symbol,
                address,
                min_deposit_formatted,
                token.symbol,
                group_balance_formatted,
                token.symbol
            );
            let request = bot.send_message(msg.chat.id, text);

            if let Some(thread_id) = thread_id {
                request
                    .reply_to(thread_id.0)
                    .parse_mode(ParseMode::Html)
                    .await?;
            } else {
                request.parse_mode(ParseMode::Html).await?;
            }

            return Ok(true);
        }

        // Use the same moderation logic as /mod, via injected dependency
        let moderation_service = bot_deps.moderation.clone();
        // Load overrides
        let overrides = bot_deps.moderation.get_moderation_settings(chat_id);

        let overrides = match overrides {
            Ok(overrides) => Some(ModerationOverrides {
                allowed_items: overrides.allowed_items,
                disallowed_items: overrides.disallowed_items,
            }),
            Err(e) => {
                log::error!("Failed to get moderation settings: {}", e);
                None
            }
        };

        let message_text = msg.text().or_else(|| msg.caption()).unwrap_or("");
        match moderation_service
            .moderate_message(message_text, &bot, &msg, &msg, overrides)
            .await
        {
            Ok(result) => {
                log::info!(
                    "Sentinel moderation result: {} for message: {} (tokens: {})",
                    result.verdict,
                    message_text,
                    result.total_tokens
                );

                let is_valid = bot_deps.group.verify(msg.chat.id).await;

                if !is_valid {
                    bot.send_message(msg.chat.id, "❌ Group credentials fails")
                        .await?;
                    return Ok(true);
                }

                let new_credentials = bot_deps.group.get_credentials(msg.chat.id);

                if new_credentials.is_none() {
                    bot.send_message(msg.chat.id, "❌ Group credentials fails")
                        .await?;
                    return Ok(true);
                };

                let new_credentials = new_credentials.unwrap();

                let purchase_result = create_purchase_request(
                    0,
                    0,
                    0,
                    result.total_tokens,
                    Model::GPT5Nano,
                    &new_credentials.jwt,
                    Some(msg.chat.id.0.to_string()),
                    None,
                    bot_deps,
                )
                .await;

                if let Err(e) = purchase_result {
                    log::error!("Failed to purchase ai for flagged content: {}", e);
                    return Ok(true);
                }

                if result.verdict == "F" {
                    // Mute the user
                    if let Some(flagged_user) = &msg.from {
                        let restricted_permissions = teloxide::types::ChatPermissions::empty();

                        // Check if the user is already muted
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
                                "Successfully muted user {} for flagged content (sentinel)",
                                flagged_user.id
                            );
                        }
                        // Add admin buttons
                        let keyboard = InlineKeyboardMarkup::new(vec![vec![
                            InlineKeyboardButton::callback(
                                "🔇 Unmute",
                                format!("unmute:{}", flagged_user.id),
                            ),
                            InlineKeyboardButton::callback(
                                "🚫 Ban",
                                format!("ban:{}:{}", flagged_user.id, msg.id.0),
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

                        let text = format!(
                            "🛡️ <b>Content Flagged & User Muted</b>\n\n📝 Message ID: <code>{}</code>\n\n❌ Status: <b>FLAGGED</b> 🔴\n🔇 User has been muted\n👤 <b>User:</b> {}\n\n💬 <i>Flagged message:</i>\n<blockquote><span class=\"tg-spoiler\">{}</span></blockquote>",
                            msg.id,
                            user_mention,
                            teloxide::utils::html::escape(message_text)
                        );
                        let request = bot
                            .send_message(msg.chat.id, text)
                            .parse_mode(ParseMode::Html)
                            .reply_markup(keyboard);

                        if let Some(thread_id) = thread_id {
                            request
                                .reply_to(thread_id.0)
                                .parse_mode(ParseMode::Html)
                                .await?;
                        } else {
                            request.parse_mode(ParseMode::Html).await?;
                        }
                        // Immediately remove the offending message from the chat
                    }
                    if let Err(e) = bot.delete_message(msg.chat.id, msg.id).await {
                        log::warn!("Failed to delete offending message {}: {}", msg.id.0, e);
                    }
                }
            }
            Err(e) => {
                log::error!("Sentinel moderation failed: {}", e);
            }
        }
        return Ok(true);
    }

    Ok(false)
}
