use anyhow::Result as AnyResult;
use chrono::Utc;
use quark_core::helpers::dto::{CoinVersion, CreateProposalRequest};
use reqwest::Url;
use teloxide::{
    prelude::*,
    types::{
        InlineKeyboardButton, InlineKeyboardButtonKind, InlineKeyboardMarkup,
        MaybeInaccessibleMessage, Message, ParseMode,
    },
};

use uuid::Uuid;

use crate::{
    dao::dto::ProposalEntry,
    dependencies::BotDependencies,
    utils::{format_time_duration, send_html_message, send_message},
};

pub async fn execute_create_proposal(
    arguments: &serde_json::Value,
    bot: Bot,
    msg: Message,
    group_id: Option<String>,
    bot_deps: BotDependencies,
) -> String {
    log::info!(
        "execute_create_proposal called with arguments: {}",
        arguments
    );

    if group_id.is_none() {
        log::error!("Group ID is missing");
        return "❌ Group ID is required".to_string();
    }

    let group_id = group_id.unwrap();

    let group_id_parsed = ChatId(group_id.parse::<i64>().unwrap());

    let auth = bot_deps.group.get_credentials(group_id_parsed);

    if auth.is_none() {
        log::error!("Error getting credentials, maybe the group is not logged in");
        return "❌ Error getting credentials, maybe the group is not logged in".to_string();
    }

    let auth = auth.unwrap();

    let user = msg.from.as_ref();

    if user.is_none() {
        log::error!("User is required");
        return "❌ User is required".to_string();
    }

    let user = user.unwrap();

    let admin_ids = bot.get_chat_administrators(msg.chat.id).await;

    let admin_ids = match admin_ids {
        Ok(ids) => ids,
        Err(e) => {
            return format!("❌ Error getting chat administrators: {}", e);
        }
    };

    if admin_ids.is_empty() {
        return "❌ Error getting chat administrators".to_string();
    }

    let is_admin = admin_ids.iter().any(|admin| admin.user.id == user.id);

    if !is_admin {
        return "❌ You are not an admin of this group".to_string();
    }

    let name = arguments["name"].as_str();

    if name.is_none() {
        return "❌ Name is required".to_string();
    }

    let description = arguments["description"].as_str();

    if description.is_none() {
        return "❌ Description is required".to_string();
    }

    let options = arguments["options"].as_array();

    if options.is_none() {
        return "❌ Options are required".to_string();
    }

    let options = options.unwrap();

    if options.is_empty() {
        return "❌ Options are required".to_string();
    }

    let options = options
        .iter()
        .map(|option| option.as_str().unwrap().to_string())
        .collect::<Vec<String>>();

    let symbol_opt = arguments["symbol"].as_str();

    // Get symbol from arguments or use saved DAO token preference
    let symbol = if symbol_opt.is_some() && !symbol_opt.unwrap().is_empty() {
        symbol_opt.unwrap().to_uppercase()
    } else {
        // Use saved DAO token preference
        let dao_admin_preferences = bot_deps
            .dao
            .get_dao_admin_preferences(auth.group_id.clone());

        if dao_admin_preferences.is_err() {
            return "❌ No symbol provided and no DAO token preference found. Please set a DAO token preference or provide a symbol.".to_string();
        }

        let symbol_opt = dao_admin_preferences.unwrap().default_dao_token;

        if symbol_opt.is_none() {
            return "❌ No symbol provided and no DAO token preference found. Please set a DAO token preference or provide a symbol.".to_string();
        }

        symbol_opt.unwrap()
    };

    let start_date = arguments["start_date"].as_str();

    let end_date = arguments["end_date"].as_str();

    if start_date.is_none() {
        return "❌ Start date is required".to_string();
    }

    if end_date.is_none() {
        return "❌ End date is required".to_string();
    }

    let start_date = start_date.unwrap();

    let start_date = start_date.parse::<u64>();

    if start_date.is_err() {
        return "❌ Start date is invalid".to_string();
    }

    let start_date = start_date.unwrap();

    if end_date.is_none() {
        return "❌ End date is invalid".to_string();
    }

    let end_date = end_date.unwrap().parse::<u64>();

    if end_date.is_err() {
        return "❌ End date is invalid".to_string();
    }

    let end_date = end_date.unwrap();

    if start_date > end_date {
        return "❌ Start date must be before end date".to_string();
    }

    // Log the date values for debugging
    log::info!(
        "DAO creation - Start date: {}, End date: {}, Current time: {}",
        start_date,
        end_date,
        Utc::now().timestamp()
    );

    let now = Utc::now().timestamp();

    // Allow DAO to start immediately or within a reasonable time window
    // The start date should not be more than 30 days in the future
    let max_future_start = now + (30 * 24 * 60 * 60); // 30 days from now

    // Allow immediate start (start_date can be now or in the past)
    // But ensure it's not too far in the future
    if start_date > max_future_start as u64 {
        return "❌ Start date cannot be more than 30 days in the future".to_string();
    }

    let token = bot_deps.panora.get_token_by_symbol(&symbol).await;

    if token.is_err() {
        return "❌ Error getting token address".to_string();
    }

    let token = token.unwrap();

    log::info!("Token: {:?}", token);

    let version = if token.token_address.is_some() {
        CoinVersion::V1
    } else {
        CoinVersion::V2
    };

    let proposal_id = Uuid::new_v4().to_string();

    let group_id_formatted = format!("{}-{}", group_id, bot_deps.group.account_seed);

    let request = CreateProposalRequest {
        name: name.unwrap().to_string(),
        description: description.unwrap().to_string(),
        options,
        start_date,
        end_date,
        proposal_id,
        version,
        currency: if token.token_address.is_some() {
            token.token_address.unwrap()
        } else {
            token.fa_address
        },
        thread_id: if let Some(thread_id) = msg.thread_id {
            Some(thread_id.0.0)
        } else {
            None
        },
    };

    log::info!("Creating proposal with request: {:?}", request);

    let proposal_entry = ProposalEntry::from((&request, group_id_formatted));

    let response = bot_deps.service.create_proposal(auth.jwt, request).await;

    if response.is_err() {
        return "❌ Error creating proposal".to_string();
    }

    let proposal_result = bot_deps.dao.create_dao(proposal_entry);

    if proposal_result.is_err() {
        return "❌ Error creating proposal".to_string();
    }

    return format!("Proposal created successfully: {}", response.unwrap().hash);
}

pub async fn handle_dao_preference_callback(
    bot: Bot,
    query: CallbackQuery,
    bot_deps: BotDependencies,
) -> AnyResult<()> {
    let data = query.data.as_ref().unwrap();
    let msg = match &query.message {
        Some(MaybeInaccessibleMessage::Regular(message)) => message,
        _ => return Ok(()),
    };

    let admins = bot.get_chat_administrators(msg.chat.id).await?;
    let is_admin = admins
        .iter()
        .any(|admin| admin.user.id.to_string() == query.from.id.to_string());

    if !is_admin {
        bot.answer_callback_query(query.id)
            .text("❌ Only group admins can manage DAO preferences.")
            .await?;
        return Ok(());
    }

    if data == "dao_preferences_done" {
        // Clear any pending token input state
        let group_id = msg.chat.id.to_string();
        let user_id = query.from.id.0.to_string();
        let formatted_group_id = format!("{}-{}", group_id, bot_deps.group.account_seed);
        let key = format!("{}_{}", user_id, formatted_group_id);
        bot_deps.dao.remove_pending_tokens(key).unwrap();

        bot.edit_message_text(
            msg.chat.id,
            msg.id,
            crate::utils::sanitize_telegram_html("✅ <b>DAO preferences saved successfully!</b>"),
        )
        .parse_mode(ParseMode::Html)
        .await?;

        bot.answer_callback_query(query.id).await?;
        return Ok(());
    }

    if data.starts_with("dao_set_expiration_") {
        let group_id = data.strip_prefix("dao_set_expiration_").unwrap();

        // Show options for expiration time
        let keyboard = InlineKeyboardMarkup::new(vec![
            vec![
                InlineKeyboardButton::new(
                    "24h",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_exp_{}_{}",
                        group_id,
                        24 * 3600
                    )),
                ),
                InlineKeyboardButton::new(
                    "48h",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_exp_{}_{}",
                        group_id,
                        48 * 3600
                    )),
                ),
                InlineKeyboardButton::new(
                    "72h",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_exp_{}_{}",
                        group_id,
                        72 * 3600
                    )),
                ),
            ],
            vec![
                InlineKeyboardButton::new(
                    "1 week",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_exp_{}_{}",
                        group_id,
                        7 * 24 * 3600
                    )),
                ),
                InlineKeyboardButton::new(
                    "2 weeks",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_exp_{}_{}",
                        group_id,
                        14 * 24 * 3600
                    )),
                ),
                InlineKeyboardButton::new(
                    "4 weeks",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_exp_{}_{}",
                        group_id,
                        28 * 24 * 3600
                    )),
                ),
            ],
            vec![InlineKeyboardButton::new(
                "🔙 Back",
                InlineKeyboardButtonKind::CallbackData("dao_preferences_back".to_string()),
            )],
        ]);

        bot.edit_message_text(
            msg.chat.id,
            msg.id,
            crate::utils::sanitize_telegram_html(
                "🗑️ <b>Select Deletion After Conclusion Duration</b>\n\n\
            Choose how long voting results are stored after voting concludes:",
            ),
        )
        .parse_mode(ParseMode::Html)
        .reply_markup(keyboard)
        .await?;
    } else if data.starts_with("dao_set_notifications_") {
        let group_id = data.strip_prefix("dao_set_notifications_").unwrap();

        // Show options for notification interval
        let keyboard = InlineKeyboardMarkup::new(vec![
            vec![
                InlineKeyboardButton::new(
                    "5min",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_notif_{}_{}",
                        group_id,
                        5 * 60
                    )),
                ),
                InlineKeyboardButton::new(
                    "10min",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_notif_{}_{}",
                        group_id,
                        10 * 60
                    )),
                ),
                InlineKeyboardButton::new(
                    "15min",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_notif_{}_{}",
                        group_id,
                        15 * 60
                    )),
                ),
            ],
            vec![
                InlineKeyboardButton::new(
                    "30min",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_notif_{}_{}",
                        group_id,
                        30 * 60
                    )),
                ),
                InlineKeyboardButton::new(
                    "1h",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_notif_{}_{}",
                        group_id,
                        60 * 60
                    )),
                ),
                InlineKeyboardButton::new(
                    "2h",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_notif_{}_{}",
                        group_id,
                        2 * 60 * 60
                    )),
                ),
            ],
            vec![
                InlineKeyboardButton::new(
                    "6h",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_notif_{}_{}",
                        group_id,
                        6 * 60 * 60
                    )),
                ),
                InlineKeyboardButton::new(
                    "12h",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_notif_{}_{}",
                        group_id,
                        12 * 60 * 60
                    )),
                ),
                InlineKeyboardButton::new(
                    "24h",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_notif_{}_{}",
                        group_id,
                        24 * 60 * 60
                    )),
                ),
            ],
            vec![InlineKeyboardButton::new(
                "🔙 Back",
                InlineKeyboardButtonKind::CallbackData("dao_preferences_back".to_string()),
            )],
        ]);

        bot.edit_message_text(
            msg.chat.id,
            msg.id,
            crate::utils::sanitize_telegram_html(
                "🔔 <b>Select Notification Interval</b>\n\n\
            Choose how often to send notifications for active proposals:",
            ),
        )
        .parse_mode(ParseMode::Html)
        .reply_markup(keyboard)
        .await?;
    } else if data.starts_with("dao_set_results_notifications_") {
        let group_id = data.strip_prefix("dao_set_results_notifications_").unwrap();

        // Show options for results notification interval
        let keyboard = InlineKeyboardMarkup::new(vec![
            vec![
                InlineKeyboardButton::new(
                    "5min",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_res_notif_{}_{}",
                        group_id,
                        5 * 60
                    )),
                ),
                InlineKeyboardButton::new(
                    "15min",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_res_notif_{}_{}",
                        group_id,
                        15 * 60
                    )),
                ),
                InlineKeyboardButton::new(
                    "30min",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_res_notif_{}_{}",
                        group_id,
                        30 * 60
                    )),
                ),
            ],
            vec![
                InlineKeyboardButton::new(
                    "1h",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_res_notif_{}_{}",
                        group_id,
                        60 * 60
                    )),
                ),
                InlineKeyboardButton::new(
                    "3h",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_res_notif_{}_{}",
                        group_id,
                        3 * 60 * 60
                    )),
                ),
                InlineKeyboardButton::new(
                    "6h",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_res_notif_{}_{}",
                        group_id,
                        6 * 60 * 60
                    )),
                ),
            ],
            vec![
                InlineKeyboardButton::new(
                    "1d",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_res_notif_{}_{}",
                        group_id,
                        24 * 60 * 60
                    )),
                ),
                InlineKeyboardButton::new(
                    "2d",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_res_notif_{}_{}",
                        group_id,
                        2 * 24 * 60 * 60
                    )),
                ),
                InlineKeyboardButton::new(
                    "5d",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_res_notif_{}_{}",
                        group_id,
                        5 * 24 * 60 * 60
                    )),
                ),
            ],
            vec![InlineKeyboardButton::new(
                "🔙 Back",
                InlineKeyboardButtonKind::CallbackData("dao_preferences_back".to_string()),
            )],
        ]);

        bot.edit_message_text(
            msg.chat.id,
            msg.id,
            crate::utils::sanitize_telegram_html(
                "🔔 <b>Select Results Notification Interval</b>\n\n\
            Choose how often to send notifications for DAO results:",
            ),
        )
        .parse_mode(ParseMode::Html)
        .reply_markup(keyboard)
        .await?;
    } else if data.starts_with("dao_set_token_") {
        let group_id = data.strip_prefix("dao_set_token_").unwrap();
        let user_id = query.from.id.0.to_string();

        // Store pending token input state in database
        let key = format!("{}_{}", user_id, group_id);
        bot_deps
            .dao
            .insert_pending_tokens(key, group_id.to_string())
            .unwrap();

        // Prompt user to send a message with the token ticker
        let keyboard = InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::new(
            "🔙 Back",
            InlineKeyboardButtonKind::CallbackData("dao_preferences_back".to_string()),
        )]]);

        bot.edit_message_text(
            msg.chat.id,
            msg.id,
            crate::utils::sanitize_telegram_html(
                "💰 <b>Enter DAO Token</b>\n\n\
            Please send a message with your preferred token ticker or emojicoin.\n\n\
            <b>Examples:</b>\n\
            • <code>APT</code>\n\
            • <code>USDC</code>\n\
            • <code>GUI</code>\n\
            • <code>eth</code> (will be converted to ETH)\n\n\
            <i>Token tickers will be automatically converted to uppercase.</i>",
            ),
        )
        .parse_mode(ParseMode::Html)
        .reply_markup(keyboard)
        .await?;
    } else if data.starts_with("dao_set_vote_duration_") {
        let group_id = data.strip_prefix("dao_set_vote_duration_").unwrap();

        // Show options for vote duration
        let keyboard = InlineKeyboardMarkup::new(vec![
            vec![
                InlineKeyboardButton::new(
                    "1 hour",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_vote_duration_{}_{}",
                        group_id, 3600
                    )),
                ),
                InlineKeyboardButton::new(
                    "6 hours",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_vote_duration_{}_{}",
                        group_id,
                        6 * 3600
                    )),
                ),
                InlineKeyboardButton::new(
                    "12 hours",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_vote_duration_{}_{}",
                        group_id,
                        12 * 3600
                    )),
                ),
            ],
            vec![
                InlineKeyboardButton::new(
                    "24 hours",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_vote_duration_{}_{}",
                        group_id,
                        24 * 3600
                    )),
                ),
                InlineKeyboardButton::new(
                    "3 days",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_vote_duration_{}_{}",
                        group_id,
                        3 * 24 * 3600
                    )),
                ),
                InlineKeyboardButton::new(
                    "5 days",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_vote_duration_{}_{}",
                        group_id,
                        5 * 24 * 3600
                    )),
                ),
            ],
            vec![
                InlineKeyboardButton::new(
                    "1 week",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_vote_duration_{}_{}",
                        group_id,
                        7 * 24 * 3600
                    )),
                ),
                InlineKeyboardButton::new(
                    "2 weeks",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_vote_duration_{}_{}",
                        group_id,
                        14 * 24 * 3600
                    )),
                ),
                InlineKeyboardButton::new(
                    "4 weeks",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_vote_duration_{}_{}",
                        group_id,
                        28 * 24 * 3600
                    )),
                ),
            ],
            vec![InlineKeyboardButton::new(
                "🔙 Back",
                InlineKeyboardButtonKind::CallbackData("dao_preferences_back".to_string()),
            )],
        ]);

        bot.edit_message_text(
            msg.chat.id,
            msg.id,
            crate::utils::sanitize_telegram_html(
                "🗳️ <b>Select Vote Duration</b>\n\n\
            Choose how long votes should remain open for proposals:",
            ),
        )
        .parse_mode(ParseMode::Html)
        .reply_markup(keyboard)
        .await?;
    } else if data.starts_with("dao_manage_disabled_") {
        let group_id = data.strip_prefix("dao_manage_disabled_").unwrap();

        // Get all proposals for this group
        let all_proposals = match bot_deps.dao.get_active_daos() {
            Ok(proposals) => proposals,
            Err(e) => {
                log::error!("Failed to get proposals: {}", e);
                bot.answer_callback_query(query.id)
                    .text("❌ Error retrieving proposals")
                    .await?;
                return Ok(());
            }
        };

        // Filter proposals for this group that have disabled notifications
        let disabled_proposals: Vec<_> = all_proposals
            .into_iter()
            .filter(|p| p.group_id == group_id && p.disabled_notifications)
            .collect();

        if disabled_proposals.is_empty() {
            bot.edit_message_text(
                msg.chat.id,
                msg.id,
                crate::utils::sanitize_telegram_html(
                    "🔕 <b>No Disabled Notifications</b>\n\n\
                All proposals in this group have notifications enabled.",
                ),
            )
            .parse_mode(ParseMode::Html)
            .reply_markup(InlineKeyboardMarkup::new(vec![vec![
                InlineKeyboardButton::new(
                    "🔙 Back",
                    InlineKeyboardButtonKind::CallbackData("dao_preferences_back".to_string()),
                ),
            ]]))
            .await?;
        } else {
            // Create keyboard with disabled proposals
            let mut keyboard_buttons = Vec::new();

            for proposal in &disabled_proposals {
                keyboard_buttons.push(vec![InlineKeyboardButton::new(
                    format!("✅ Enable: {}", proposal.name),
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_enable_notifications_{}",
                        proposal.proposal_id
                    )),
                )]);
            }

            // Add back button
            keyboard_buttons.push(vec![InlineKeyboardButton::new(
                "🔙 Back",
                InlineKeyboardButtonKind::CallbackData("dao_preferences_back".to_string()),
            )]);

            let keyboard = InlineKeyboardMarkup::new(keyboard_buttons);

            let message_text = format!(
                "🔕 <b>Disabled Notifications</b>\n\n\
                Found {} proposal(s) with disabled notifications:\n\n\
                💡 <i>Click on a proposal to re-enable its notifications</i>",
                disabled_proposals.len()
            );

            let message_text = crate::utils::sanitize_telegram_html(&message_text);
            bot.edit_message_text(msg.chat.id, msg.id, message_text)
                .parse_mode(ParseMode::Html)
                .reply_markup(keyboard)
                .await?;
        }
    } else if data.starts_with("dao_enable_notifications_") {
        let proposal_id = data.strip_prefix("dao_enable_notifications_").unwrap();

        // Enable notifications for this proposal
        if let Err(e) = bot_deps
            .dao
            .update_disabled_notifications(proposal_id.to_string(), false)
        {
            log::error!(
                "Failed to enable notifications for proposal {}: {}",
                proposal_id,
                e
            );
            bot.answer_callback_query(query.id)
                .text("❌ Error enabling notifications")
                .await?;
            return Ok(());
        }

        bot.answer_callback_query(query.id.clone())
            .text("✅ Notifications enabled for this proposal")
            .await?;

        // Go back to the manage disabled menu
        let group_id = msg.chat.id.to_string();
        let group_id_formatted = format!("{}-{}", group_id, bot_deps.group.account_seed);
        let all_proposals = match bot_deps.dao.get_active_daos() {
            Ok(proposals) => proposals,
            Err(e) => {
                log::error!("Failed to get proposals: {}", e);
                return Ok(());
            }
        };

        // Filter proposals for this group that have disabled notifications
        let disabled_proposals: Vec<_> = all_proposals
            .into_iter()
            .filter(|p| p.group_id == group_id_formatted && p.disabled_notifications)
            .collect();

        if disabled_proposals.is_empty() {
            bot.edit_message_text(
                msg.chat.id,
                msg.id,
                crate::utils::sanitize_telegram_html(
                    "🔕 <b>No Disabled Notifications</b>\n\n\
                All proposals in this group have notifications enabled.",
                ),
            )
            .parse_mode(ParseMode::Html)
            .reply_markup(InlineKeyboardMarkup::new(vec![vec![
                InlineKeyboardButton::new(
                    "🔙 Back",
                    InlineKeyboardButtonKind::CallbackData("dao_preferences_back".to_string()),
                ),
            ]]))
            .await?;
        } else {
            // Create keyboard with remaining disabled proposals
            let mut keyboard_buttons = Vec::new();

            for proposal in &disabled_proposals {
                keyboard_buttons.push(vec![InlineKeyboardButton::new(
                    format!("✅ Enable: {}", proposal.name),
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_enable_notifications_{}",
                        proposal.proposal_id
                    )),
                )]);
            }

            // Add back button
            keyboard_buttons.push(vec![InlineKeyboardButton::new(
                "🔙 Back",
                InlineKeyboardButtonKind::CallbackData("dao_preferences_back".to_string()),
            )]);

            let keyboard = InlineKeyboardMarkup::new(keyboard_buttons);

            let message_text = format!(
                "🔕 <b>Disabled Notifications</b>\n\n\
                Found {} proposal(s) with disabled notifications:\n\n\
                💡 <i>Click on a proposal to re-enable its notifications</i>",
                disabled_proposals.len()
            );

            let message_text = crate::utils::sanitize_telegram_html(&message_text);
            bot.edit_message_text(msg.chat.id, msg.id, message_text)
                .parse_mode(ParseMode::Html)
                .reply_markup(keyboard)
                .await?;
        }
    } else if data.starts_with("dao_exp_") {
        let parts: Vec<&str> = data.split('_').collect();
        if parts.len() >= 4 {
            let group_id = parts[2];
            let expiration_time: u64 = parts[3].parse().unwrap_or(24 * 3600);

            // Update expiration time
            match bot_deps.dao.get_dao_admin_preferences(group_id.to_string()) {
                Ok(mut prefs) => {
                    prefs.expiration_time = expiration_time;
                    if let Err(_) = bot_deps
                        .dao
                        .set_dao_admin_preferences(group_id.to_string(), prefs)
                    {
                        bot.answer_callback_query(query.id)
                            .text("❌ Error updating preferences")
                            .await?;
                        return Ok(());
                    }
                }
                Err(_) => {
                    bot.answer_callback_query(query.id)
                        .text("❌ Error: No admin preferences found for this group")
                        .await?;
                    return Ok(());
                }
            }

            // Show popup notification
            bot.answer_callback_query(query.id.clone())
                .text(format!(
                    "✅ Deletion after conclusion duration updated to {}",
                    format_time_duration(expiration_time)
                ))
                .await?;

            // Return to DAO preferences menu
            let group_id = msg.chat.id.to_string();
            let group_id_formatted = format!("{}-{}", group_id, bot_deps.group.account_seed);
            let current_prefs = match bot_deps
                .dao
                .get_dao_admin_preferences(group_id_formatted.clone())
            {
                Ok(prefs) => prefs,
                Err(_) => return Ok(()),
            };

            let keyboard = InlineKeyboardMarkup::new(vec![
                vec![InlineKeyboardButton::new(
                    format!(
                        "🗑️ Deletion After Conclusion: {}",
                        format_time_duration(current_prefs.expiration_time)
                    ),
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_set_expiration_{}",
                        group_id_formatted
                    )),
                )],
                vec![InlineKeyboardButton::new(
                    format!(
                        "🔔 Notification Interval: {}",
                        format_time_duration(current_prefs.interval_active_proposal_notifications)
                    ),
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_set_notifications_{}",
                        group_id_formatted
                    )),
                )],
                vec![InlineKeyboardButton::new(
                    format!(
                        "🔔 Results Notification: {}",
                        format_time_duration(current_prefs.interval_dao_results_notifications)
                    ),
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_set_results_notifications_{}",
                        group_id_formatted
                    )),
                )],
                vec![InlineKeyboardButton::new(
                    format!(
                        "🗳️ Vote Duration: {}",
                        format_time_duration(current_prefs.vote_duration.unwrap_or(24 * 60 * 60))
                    ),
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_set_vote_duration_{}",
                        group_id_formatted
                    )),
                )],
                vec![InlineKeyboardButton::new(
                    "↩️ Back",
                    InlineKeyboardButtonKind::CallbackData("back_to_group_settings".to_string()),
                )],
            ]);

            bot.edit_message_text(
                msg.chat.id,
                msg.id,
                crate::utils::sanitize_telegram_html("🏛️ <b>DAO Preferences</b>\n\nConfigure group DAO settings:"),
            )
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard)
            .await?;
        }
    } else if data.starts_with("dao_notif_") {
        let parts: Vec<&str> = data.split('_').collect();
        if parts.len() >= 4 {
            let group_id = parts[2];
            let notification_interval: u64 = parts[3].parse().unwrap_or(60 * 60);

            // Update notification interval
            match bot_deps.dao.get_dao_admin_preferences(group_id.to_string()) {
                Ok(mut prefs) => {
                    prefs.interval_active_proposal_notifications = notification_interval;
                    if let Err(_) = bot_deps
                        .dao
                        .set_dao_admin_preferences(group_id.to_string(), prefs)
                    {
                        bot.answer_callback_query(query.id)
                            .text("❌ Error updating preferences")
                            .await?;
                        return Ok(());
                    }
                }
                Err(_) => {
                    bot.answer_callback_query(query.id)
                        .text("❌ Error: No admin preferences found for this group")
                        .await?;
                    return Ok(());
                }
            }

            // Show popup notification
            bot.answer_callback_query(query.id.clone())
                .text(format!(
                    "✅ Notification interval updated to {}",
                    format_time_duration(notification_interval)
                ))
                .await?;

            // Return to DAO preferences menu
            let group_id = msg.chat.id.to_string();
            let group_id_formatted = format!("{}-{}", group_id, bot_deps.group.account_seed);
            let current_prefs = match bot_deps
                .dao
                .get_dao_admin_preferences(group_id_formatted.clone())
            {
                Ok(prefs) => prefs,
                Err(_) => return Ok(()),
            };

            let keyboard = InlineKeyboardMarkup::new(vec![
                vec![InlineKeyboardButton::new(
                    format!(
                        "🗑️ Deletion After Conclusion: {}",
                        format_time_duration(current_prefs.expiration_time)
                    ),
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_set_expiration_{}",
                        group_id_formatted
                    )),
                )],
                vec![InlineKeyboardButton::new(
                    format!(
                        "🔔 Notification Interval: {}",
                        format_time_duration(current_prefs.interval_active_proposal_notifications)
                    ),
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_set_notifications_{}",
                        group_id_formatted
                    )),
                )],
                vec![InlineKeyboardButton::new(
                    format!(
                        "🔔 Results Notification: {}",
                        format_time_duration(current_prefs.interval_dao_results_notifications)
                    ),
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_set_results_notifications_{}",
                        group_id_formatted
                    )),
                )],
                vec![InlineKeyboardButton::new(
                    format!(
                        "🗳️ Vote Duration: {}",
                        format_time_duration(current_prefs.vote_duration.unwrap_or(24 * 60 * 60))
                    ),
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_set_vote_duration_{}",
                        group_id_formatted
                    )),
                )],
                vec![InlineKeyboardButton::new(
                    "↩️ Back",
                    InlineKeyboardButtonKind::CallbackData("back_to_group_settings".to_string()),
                )],
            ]);

            bot.edit_message_text(
                msg.chat.id,
                msg.id,
                crate::utils::sanitize_telegram_html("🏛️ <b>DAO Preferences</b>\n\nConfigure group DAO settings:"),
            )
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard)
            .await?;
        }
    } else if data.starts_with("dao_res_notif_") {
        let parts: Vec<&str> = data.split('_').collect();
        if parts.len() >= 5 {
            let group_id = parts[3];
            let results_notification_interval: u64 = parts[4].parse().unwrap_or(60 * 60);

            // Update results notification interval
            match bot_deps.dao.get_dao_admin_preferences(group_id.to_string()) {
                Ok(mut prefs) => {
                    prefs.interval_dao_results_notifications = results_notification_interval;
                    if let Err(_) = bot_deps
                        .dao
                        .set_dao_admin_preferences(group_id.to_string(), prefs)
                    {
                        bot.answer_callback_query(query.id)
                            .text("❌ Error updating preferences")
                            .await?;
                        return Ok(());
                    }
                }
                Err(_) => {
                    bot.answer_callback_query(query.id)
                        .text("❌ Error: No admin preferences found for this group")
                        .await?;
                    return Ok(());
                }
            }

            // Show popup notification
            bot.answer_callback_query(query.id.clone())
                .text(format!(
                    "✅ Results notification interval updated to {}",
                    format_time_duration(results_notification_interval)
                ))
                .await?;

            // Return to DAO preferences menu
            let group_id = msg.chat.id.to_string();
            let group_id_formatted = format!("{}-{}", group_id, bot_deps.group.account_seed);
            let current_prefs = match bot_deps
                .dao
                .get_dao_admin_preferences(group_id_formatted.clone())
            {
                Ok(prefs) => prefs,
                Err(_) => return Ok(()),
            };

            let keyboard = InlineKeyboardMarkup::new(vec![
                vec![InlineKeyboardButton::new(
                    format!(
                        "🗑️ Deletion After Conclusion: {}",
                        format_time_duration(current_prefs.expiration_time)
                    ),
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_set_expiration_{}",
                        group_id_formatted
                    )),
                )],
                vec![InlineKeyboardButton::new(
                    format!(
                        "🔔 Notification Interval: {}",
                        format_time_duration(current_prefs.interval_active_proposal_notifications)
                    ),
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_set_notifications_{}",
                        group_id_formatted
                    )),
                )],
                vec![InlineKeyboardButton::new(
                    format!(
                        "🔔 Results Notification: {}",
                        format_time_duration(current_prefs.interval_dao_results_notifications)
                    ),
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_set_results_notifications_{}",
                        group_id_formatted
                    )),
                )],
                vec![InlineKeyboardButton::new(
                    format!(
                        "🗳️ Vote Duration: {}",
                        format_time_duration(current_prefs.vote_duration.unwrap_or(24 * 60 * 60))
                    ),
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_set_vote_duration_{}",
                        group_id_formatted
                    )),
                )],
                vec![InlineKeyboardButton::new(
                    "↩️ Back",
                    InlineKeyboardButtonKind::CallbackData("back_to_group_settings".to_string()),
                )],
            ]);

            bot.edit_message_text(
                msg.chat.id,
                msg.id,
                "🏛️ <b>DAO Preferences</b>\n\nConfigure group DAO settings:",
            )
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard)
            .await?;
        }
    } else if data.starts_with("dao_vote_duration_") {
        let parts: Vec<&str> = data.split('_').collect();
        if parts.len() >= 5 {
            let group_id = parts[3];
            let vote_duration: u64 = parts[4].parse().unwrap_or(24 * 3600);

            // Update vote duration
            match bot_deps.dao.get_dao_admin_preferences(group_id.to_string()) {
                Ok(mut prefs) => {
                    prefs.vote_duration = Some(vote_duration);
                    if let Err(_) = bot_deps
                        .dao
                        .set_dao_admin_preferences(group_id.to_string(), prefs)
                    {
                        bot.answer_callback_query(query.id)
                            .text("❌ Error updating preferences")
                            .await?;
                        return Ok(());
                    }
                }
                Err(_) => {
                    bot.answer_callback_query(query.id)
                        .text("❌ Error: No admin preferences found for this group")
                        .await?;
                    return Ok(());
                }
            }

            // Show popup notification
            bot.answer_callback_query(query.id.clone())
                .text(format!(
                    "✅ Vote duration updated to {}",
                    format_time_duration(vote_duration)
                ))
                .await?;

            // Return to DAO preferences menu
            let group_id = msg.chat.id.to_string();
            let group_id_formatted = format!("{}-{}", group_id, bot_deps.group.account_seed);
            let current_prefs = match bot_deps
                .dao
                .get_dao_admin_preferences(group_id_formatted.clone())
            {
                Ok(prefs) => prefs,
                Err(_) => return Ok(()),
            };

            let keyboard = InlineKeyboardMarkup::new(vec![
                vec![InlineKeyboardButton::new(
                    format!(
                        "🗑️ Deletion After Conclusion: {}",
                        format_time_duration(current_prefs.expiration_time)
                    ),
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_set_expiration_{}",
                        group_id_formatted
                    )),
                )],
                vec![InlineKeyboardButton::new(
                    format!(
                        "🔔 Notification Interval: {}",
                        format_time_duration(current_prefs.interval_active_proposal_notifications)
                    ),
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_set_notifications_{}",
                        group_id_formatted
                    )),
                )],
                vec![InlineKeyboardButton::new(
                    format!(
                        "🔔 Results Notification: {}",
                        format_time_duration(current_prefs.interval_dao_results_notifications)
                    ),
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_set_results_notifications_{}",
                        group_id_formatted
                    )),
                )],
                vec![InlineKeyboardButton::new(
                    format!(
                        "🗳️ Vote Duration: {}",
                        format_time_duration(current_prefs.vote_duration.unwrap_or(24 * 60 * 60))
                    ),
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "dao_set_vote_duration_{}",
                        group_id_formatted
                    )),
                )],
                vec![InlineKeyboardButton::new(
                    "↩️ Back",
                    InlineKeyboardButtonKind::CallbackData("back_to_group_settings".to_string()),
                )],
            ]);

            bot.edit_message_text(
                msg.chat.id,
                msg.id,
                "🏛️ <b>DAO Preferences</b>\n\nConfigure group DAO settings:",
            )
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard)
            .await?;
        }
    } else if data == "dao_preferences_back" {
        // Go back to main preferences menu - just edit the message back to the main menu
        let group_id = msg.chat.id.to_string();
        let group_id_formatted = format!("{}-{}", group_id, bot_deps.group.account_seed);

        // Clear any pending token input state
        let user_id = query.from.id.0.to_string();
        let key = format!("{}_{}", user_id, group_id_formatted);
        bot_deps.dao.remove_pending_tokens(key).unwrap();
        let current_prefs = match bot_deps
            .dao
            .get_dao_admin_preferences(group_id_formatted.clone())
        {
            Ok(prefs) => prefs,
            Err(_) => return Ok(()),
        };

        let keyboard = InlineKeyboardMarkup::new(vec![
            vec![InlineKeyboardButton::new(
                format!(
                    "🗑️ Deletion After Conclusion Duration: {}",
                    format_time_duration(current_prefs.expiration_time)
                ),
                InlineKeyboardButtonKind::CallbackData(format!(
                    "dao_set_expiration_{}",
                    group_id_formatted
                )),
            )],
            vec![InlineKeyboardButton::new(
                format!(
                    "🔔 Notification Interval: {}",
                    format_time_duration(current_prefs.interval_active_proposal_notifications)
                ),
                InlineKeyboardButtonKind::CallbackData(format!(
                    "dao_set_notifications_{}",
                    group_id_formatted
                )),
            )],
            vec![InlineKeyboardButton::new(
                format!(
                    "🔔 Results Notification Interval: {}",
                    format_time_duration(current_prefs.interval_dao_results_notifications)
                ),
                InlineKeyboardButtonKind::CallbackData(format!(
                    "dao_set_results_notifications_{}",
                    group_id_formatted
                )),
            )],
            vec![InlineKeyboardButton::new(
                format!(
                    "💰 DAO Token: {}",
                    current_prefs
                        .default_dao_token
                        .as_ref()
                        .unwrap_or(&"".to_string())
                ),
                InlineKeyboardButtonKind::CallbackData(format!(
                    "dao_set_token_{}",
                    group_id_formatted
                )),
            )],
            vec![InlineKeyboardButton::new(
                format!(
                    "🗳️ Vote Duration: {}",
                    format_time_duration(current_prefs.vote_duration.unwrap_or(24 * 60 * 60))
                ),
                InlineKeyboardButtonKind::CallbackData(format!(
                    "dao_set_vote_duration_{}",
                    group_id_formatted
                )),
            )],
            vec![InlineKeyboardButton::new(
                "🔕 Manage Disabled Notifications",
                InlineKeyboardButtonKind::CallbackData(format!(
                    "dao_manage_disabled_{}",
                    group_id_formatted
                )),
            )],
            vec![InlineKeyboardButton::new(
                "✅ Done",
                InlineKeyboardButtonKind::CallbackData("dao_preferences_done".to_string()),
            )],
        ]);

        let message_text = format!(
            "🏛️ <b>DAO Admin Preferences</b>\n\n\
            📊 <b>Current Settings:</b>\n\
            🗑️ <b>Deletion After Conclusion Duration:</b> {}\n\
            🔔 <b>Notification Interval:</b> {}\n\
            💰 <b>DAO Token:</b> {}\n\
            🗳️ <b>Vote Duration:</b> {}\n\n\
            💡 <i>Click the buttons below to modify these settings</i>",
            format_time_duration(current_prefs.expiration_time),
            format_time_duration(current_prefs.interval_active_proposal_notifications),
            current_prefs.default_dao_token.unwrap_or("".to_string()),
            format_time_duration(current_prefs.vote_duration.unwrap_or(24 * 60 * 60))
        );

        bot.edit_message_text(msg.chat.id, msg.id, message_text)
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard)
            .await?;
    }

    bot.answer_callback_query(query.id).await?;
    Ok(())
}

pub async fn handle_disable_notifications_callback(
    bot: Bot,
    query: CallbackQuery,
    bot_deps: BotDependencies,
) -> AnyResult<()> {
    let app_url =
        std::env::var("APP_URL").map_err(|e| anyhow::anyhow!("APP_URL is not set: {}", e))?;

    let msg = match &query.message {
        Some(MaybeInaccessibleMessage::Regular(message)) => message,
        _ => return Ok(()),
    };

    // Extract proposal ID from the message text
    let message_text = msg.text().unwrap_or("");
    let lines: Vec<&str> = message_text.lines().collect();

    // Find the proposal name line (starts with 🏛️)
    let proposal_name = lines
        .iter()
        .find(|line| line.starts_with("🏛️"))
        .map(|line| line.trim_start_matches("🏛️ ").trim())
        .unwrap_or("");

    if proposal_name.is_empty() {
        bot.answer_callback_query(query.id)
            .text("❌ Could not identify the proposal")
            .await?;
        return Ok(());
    }

    // Get all active proposals to find the one with this name
    let active_proposals = match bot_deps.dao.get_active_daos() {
        Ok(proposals) => proposals,
        Err(e) => {
            log::error!("Failed to get active proposals: {}", e);
            bot.answer_callback_query(query.id)
                .text("❌ Error retrieving proposals")
                .await?;
            return Ok(());
        }
    };

    // Find the proposal by name
    let proposal = active_proposals.iter().find(|p| p.name == proposal_name);

    match proposal {
        Some(proposal) => {
            // Disable notifications for this proposal
            if let Err(e) = bot_deps
                .dao
                .update_disabled_notifications(proposal.proposal_id.clone(), true)
            {
                log::error!("Failed to disable notifications: {}", e);
                bot.answer_callback_query(query.id)
                    .text("❌ Error disabling notifications")
                    .await?;
                return Ok(());
            }

            // Remove the disable button from the keyboard
            let mut keyboard_buttons = Vec::new();

            // Recreate the voting options
            for (index, option) in proposal.options.iter().enumerate() {
                let base_url = format!(
                    "{}/dao?group_id={}&proposal_id={}&choice_id={}&coin_type={}&coin_version={}&dao_name={}&dao_description={}",
                    app_url,
                    proposal.group_id,
                    proposal.proposal_id,
                    index,
                    proposal.coin_type,
                    match proposal.version {
                        CoinVersion::V1 => "V1",
                        CoinVersion::V2 => "V2",
                    },
                    proposal.name,
                    proposal.description
                );

                // Parse URL with error handling
                let parsed_url = match Url::parse(&base_url) {
                    Ok(url) => url,
                    Err(e) => {
                        log::error!(
                            "Failed to parse URL for proposal {}: {}",
                            proposal.proposal_id,
                            e
                        );
                        continue;
                    }
                };

                keyboard_buttons.push(vec![InlineKeyboardButton::url(
                    format!("🗳️ Vote: {}", option),
                    parsed_url,
                )]);
            }

            // Add voting help button
            keyboard_buttons.push(vec![InlineKeyboardButton::callback(
                "ℹ️ How to Vote",
                "voting_help",
            )]);

            let keyboard = InlineKeyboardMarkup::new(keyboard_buttons);

            // Update the message with new keyboard (without the disable button)
            bot.edit_message_reply_markup(msg.chat.id, msg.id)
                .reply_markup(keyboard)
                .await?;

            bot.answer_callback_query(query.id)
                .text("✅ Notifications disabled for this proposal")
                .await?;
        }
        None => {
            bot.answer_callback_query(query.id)
                .text("❌ Could not find the proposal")
                .await?;
        }
    }

    Ok(())
}

pub async fn handle_message_dao(
    bot: Bot,
    msg: Message,
    bot_deps: BotDependencies,
    user_id: String,
    formatted_group_id: String,
) -> AnyResult<bool> {
    let key = format!("{}_{}", user_id, formatted_group_id);
    if let Ok(_) = bot_deps.dao.get_pending_tokens(key.clone()) {
        // User is in token input mode
        if let Some(text) = msg.text() {
            let text = text.trim();
            if !text.is_empty() {
                // Process the token: convert to uppercase except for emojis
                let processed_token = if text.chars().any(|c| c.is_ascii_alphabetic()) {
                    // Contains letters, convert to uppercase
                    text.to_uppercase()
                } else {
                    // Likely an emoji or special characters, keep as-is
                    text.to_string()
                };

                // Update DAO token preference using the formatted group ID
                if let Ok(mut prefs) = bot_deps
                    .dao
                    .get_dao_admin_preferences(formatted_group_id.clone())
                {
                    prefs.default_dao_token = Some(processed_token.clone());
                    if let Ok(_) = bot_deps
                        .dao
                        .set_dao_admin_preferences(formatted_group_id.clone(), prefs)
                    {
                        // Clear the pending state
                        bot_deps.dao.remove_pending_tokens(key).unwrap();

                        send_html_message(
                            msg,
                            bot,
                            format!("✅ <b>DAO token updated to {}</b>", processed_token),
                        )
                        .await?;
                        return Ok(true);
                    }
                }

                // If we get here, there was an error
                bot_deps.dao.remove_pending_tokens(key).unwrap();
                send_message(
                    msg,
                    bot,
                    "❌ Error updating DAO token preference".to_string(),
                )
                .await?;
                return Ok(true);
            }
        }

        // Invalid input, ask again
        send_message(
            msg,
            bot,
            "❌ Please send a valid token ticker or emojicoin. Example: APT, USDC, or GUI"
                .to_string(),
        )
        .await?;

        return Ok(true);
    } else {
        return Ok(false);
    }
}
