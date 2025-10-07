use std::env;

use chrono::Utc;
use reqwest::Url;
use teloxide::{Bot, types::{ChatId, InlineKeyboardButton, InlineKeyboardMarkup}};
use tokio_cron_scheduler::Job;
use aptos_rust_sdk_types::api_types::view::ViewRequest;

use crate::{
    dao::{dao::Dao, dto::ProposalEntry},
    panora::handler::Panora,
    utils::{format_timestamp, send_scheduled_message, send_scheduled_message_with_keyboard},
    welcome::welcome_service::WelcomeService,
};
use quark_core::helpers::dto::CoinVersion;

// Retry function for handling rate limits with exponential backoff


// Helper function to escape special characters for MarkdownV2
fn escape_markdown_v2(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '_' | '*' | '[' | ']' | '(' | ')' | '~' | '`' | '>' | '#' | '+' | '-' | '=' | '|' | '{' | '}' | '.' | '!' => format!("\\{}", c),
            _ => c.to_string(),
        })
        .collect()
}

pub fn job_token_list(panora: Panora) -> Job {
    Job::new_async("0 0 * * * *", move |_uuid, _l| {
        let panora = panora.clone();
        Box::pin(async move {
            match panora.set_panora_token_list().await {
                Ok(_) => log::info!("Successfully updated Panora token list"),
                Err(e) => {
                    log::error!("Failed to update Panora token list: {}", e);
                }
            }
        })
    })
    .expect("Failed to create cron job")
}

pub fn job_welcome_service_cleanup(welcome_service: WelcomeService, bot: Bot) -> Job {
    Job::new_async("0 */5 * * * *", move |_uuid, _l| {
        let welcome_service = welcome_service.clone();
        let bot = bot.clone();
        Box::pin(async move {
            log::info!("Running welcome service cleanup job");
            
            // Cleanup expired verifications
            if let Err(e) = welcome_service.cleanup_all_expired_verifications(&bot).await {
                log::error!("Failed to cleanup expired welcome verifications: {}", e);
            }
        })
    })
    .expect("Failed to create cron job")
}

pub fn job_token_ai_fees(panora: Panora) -> Job {
    // Run every 15 minutes instead of every minute to avoid rate limits
    Job::new_async("0 */15 * * * *", move |_uuid, _l| {
        let panora = panora.clone();
        Box::pin(async move {
            // Get token address first
            let token_address = match panora.aptos.get_token_address().await {
                Ok(address) => address,
                Err(e) => {
                    log::error!("Failed to get token address for AI fees job: {}", e);
                    return;
                }
            };
            
            // Use Panora method with built-in retry logic
            match panora.set_token_ai_fees(&token_address).await {
                Ok(_) => log::info!("Successfully updated Panora token AI fees"),
                Err(e) => {
                    log::error!("Failed to update Panora token AI fees: {}", e);
                }
            }
        })
    })
    .expect("Failed to create cron job")
}

pub fn job_active_daos(dao: Dao, bot: Bot) -> Job {
    let base_url = match env::var("APP_URL") {
        Ok(url) => url,
        Err(e) => {
            log::error!("Failed to get APP_URL environment variable: {}", e);
            return Job::new_async("0 */2 * * * *", move |_uuid, _l| {
                Box::pin(async move {
                    log::error!("Cannot run active DAOs job - APP_URL not configured");
                })
            }).expect("Failed to create fallback cron job");
        }
    };
    
    Job::new_async("0 */2 * * * *", move |_uuid, _l| {
        let base_url = base_url.clone();
        let dao = dao.clone();
        let bot = bot.clone();
        log::info!("Running active DAOs job");
        Box::pin(async move {
            let daos = match dao.get_active_daos() {
                Ok(daos) => daos,
                Err(e) => {
                    log::error!("Failed to get active DAOs: {}", e);
                    return;
                }
            };

            log::info!("Active DAOs: {}", daos.len());

            for proposal_entry in daos {
                let group_id = proposal_entry.group_id.clone();

                let account_seed = env::var("ACCOUNT_SEED").map_err(|e| anyhow::anyhow!("Failed to get ACCOUNT_SEED: {}", e)).unwrap_or("".to_string());

                let group_id_trimmed = group_id.trim_end_matches(format!("-{}", account_seed).as_str());

                // Parse chat group ID with error handling
                let chat_group_id = match group_id_trimmed.parse::<i64>() {
                    Ok(id) => ChatId(id),
                    Err(e) => {
                        log::error!("Failed to parse group ID {}: {}", group_id, e);
                        continue;
                    }
                };
                
                // Get admin preferences with error handling
                let admin_preferences = match dao.get_dao_admin_preferences(group_id.clone()) {
                    Ok(prefs) => prefs,
                    Err(e) => {
                        log::error!("Failed to get admin preferences for group {}: {}", group_id, e);
                        continue;
                    }
                };
                let now = Utc::now().timestamp() as u64;
                let time_since_last_notification = now - proposal_entry.last_active_notification;
                let interval_seconds = admin_preferences.interval_active_proposal_notifications;

                log::info!(
                    "DAO {}: Last notification was {} seconds ago, interval is {} seconds ({} hours)",
                    proposal_entry.proposal_id,
                    time_since_last_notification,
                    interval_seconds,
                    interval_seconds / 3600
                );

                // Check if notifications are enabled for this specific proposal
                if !proposal_entry.disabled_notifications && time_since_last_notification >= interval_seconds {
                    // Create inline keyboard with voting options
                    let mut keyboard_buttons = Vec::new();
                    
                    for (index, option) in proposal_entry.options.iter().enumerate() {
                        let base_url = format!(
                            "{}/dao?group_id={}&proposal_id={}&choice_id={}&coin_type={}&coin_version={}&dao_name={}&dao_description={}",
                            base_url,
                            group_id,
                            proposal_entry.proposal_id,
                            index,
                            proposal_entry.coin_type,
                            match proposal_entry.version {
                                CoinVersion::V1 => "V1",
                                CoinVersion::V2 => "V2",
                            },
                            proposal_entry.name,
                            proposal_entry.description
                        );
                        
                        // Parse URL with error handling
                        let parsed_url = match Url::parse(&base_url) {
                            Ok(url) => url,
                            Err(e) => {
                                log::error!("Failed to parse URL for DAO {}: {}", proposal_entry.proposal_id, e);
                                continue;
                            }
                        };
                        
                        // Create a row with URL button for each option
                        // Note: WebApp buttons are not supported in group chats, only in private chats
                        let option_row = vec![
                            InlineKeyboardButton::url(
                                format!("🗳️ Vote: {}", option),
                                parsed_url
                            ),
                        ];
                        
                        keyboard_buttons.push(option_row);
                    }

                    // Add a separator row with voting instructions
                    keyboard_buttons.push(vec![
                        InlineKeyboardButton::callback(
                            "ℹ️ How to Vote",
                            "voting_help"
                        )
                    ]);
                    // Only show disable button if notifications are enabled
                    if !proposal_entry.disabled_notifications {
                        keyboard_buttons.push(vec![
                            InlineKeyboardButton::callback(
                                "❌ Disable Notifications",
                                "disable_notifications"
                            )
                        ]);
                    }

                    let keyboard = InlineKeyboardMarkup::new(keyboard_buttons);

                    // Create rich message text
                    let message_text = format!(
                        "🏛️ {}\n\n📝 {}\n\n⏰ Voting ends: {}\n\n🗳️ Click on your preferred option below to vote:",
                        proposal_entry.name,
                        proposal_entry.description,
                        format_timestamp(proposal_entry.end_date)
                    );

                    log::info!("Sending active proposals notification for: {}", proposal_entry.proposal_id);

                    // Send message with error handling
                    match send_scheduled_message_with_keyboard(
                        &bot,
                        chat_group_id,
                        &message_text,
                        proposal_entry.thread_id,
                        keyboard,
                    ).await {
                        Ok(_) => {
                            log::info!("Successfully sent active proposals notification for: {}", proposal_entry.proposal_id);
                        }
                        Err(e) => {
                            log::error!("Failed to send active proposals notification for {}: {}", proposal_entry.proposal_id, e);
                            continue;
                        }
                    }

                    // Update last active notification with error handling
                    if let Err(e) = dao.update_last_active_notification(proposal_entry.proposal_id.clone()) {
                        log::error!("Failed to update last active notification for proposal {}: {}", proposal_entry.proposal_id, e);
                    }
                } else {
                    log::info!(
                        "Proposal {}: Skipping notification - {} seconds remaining until next notification (interval: {} hours)",
                        proposal_entry.proposal_id,
                        interval_seconds - time_since_last_notification,
                        interval_seconds / 3600
                    );
                }
            }
        })
    })
    .expect("Failed to create cron job")
}

pub fn job_daos_results(panora: Panora, bot: Bot, dao: Dao) -> Job {
    Job::new_async("0 */2 * * * *", move |_uuid, _l| {
        let panora = panora.clone();
        let bot = bot.clone();
        let dao = dao.clone();
        Box::pin(async move {
            log::info!("Proposal results job executed at {}", Utc::now());
            
            // Get finished DAOs that haven't been notified yet
            let daos = match dao.get_dao_results() {
                Ok(daos) => daos,
                Err(e) => {
                    log::error!("Failed to get active DAOs: {}", e);
                    return;
                }
            };

            log::info!("Daos results list: {}", daos.len());

            for proposal_entry in daos {
                let group_id = proposal_entry.group_id.clone();

                let admin_preferences = match dao.get_dao_admin_preferences(group_id.clone()) {
                    Ok(prefs) => prefs,
                    Err(e) => {
                        log::error!("Failed to get admin preferences for group {}: {}", group_id, e);
                        continue;
                    }
                };

                let interval_seconds = admin_preferences.interval_dao_results_notifications;
                let now = Utc::now().timestamp() as u64;

                let time_since_last_notification = now - proposal_entry.last_result_notification;
                
                log::info!("Time since last notification: {}", time_since_last_notification);
                log::info!("Interval seconds: {}", interval_seconds);
                log::info!("Proposal id: {}", proposal_entry.proposal_id);

                
                // Check if notifications are enabled for this specific proposal
                if !proposal_entry.disabled_notifications && time_since_last_notification >= interval_seconds {
                // Check if DAO has ended and results haven't been sent
                    log::info!("Processing finished DAO: {}", proposal_entry.proposal_id);
                    
                    match fetch_and_send_dao_results(&panora, &bot, &proposal_entry).await {
                        Ok(_) => {
                            log::info!("Successfully sent DAO results for: {}", proposal_entry.proposal_id);
                            if let Err(e) = dao.update_last_result_notification(proposal_entry.proposal_id.clone()) {
                                log::error!("Failed to update last result notification for DAO {}: {}", proposal_entry.proposal_id, e);
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to send DAO results for {}: {}", proposal_entry.proposal_id, e);
                            }
                        }
                }
            
            }
        })
    })
    .expect("Failed to create cron job")
}

async fn fetch_and_send_dao_results(
    panora: &Panora,
    bot: &Bot,
    proposal_entry: &ProposalEntry,
) -> anyhow::Result<()> {
    let group_id = proposal_entry.group_id.clone();

    let account_seed = env::var("ACCOUNT_SEED").map_err(|e| anyhow::anyhow!("Failed to get ACCOUNT_SEED: {}", e))?;

    let group_id_trimmed = group_id.trim_end_matches(format!("-{}", account_seed).as_str());

    // Parse chat group ID with error handling
    let chat_group_id = match group_id_trimmed.parse::<i64>() {
        Ok(id) => ChatId(id),
        Err(e) => {
            log::error!("Failed to parse group ID {} for DAO {}: {}", group_id, proposal_entry.proposal_id, e);
            return Err(anyhow::anyhow!("Invalid group ID: {}", e));
        }
    };
    
    // Prepare view request based on DAO version
    let view_function = match proposal_entry.version {
        CoinVersion::V1 => "get_group_dao_v1",
        CoinVersion::V2 => "get_group_dao_v2",
    };
    
    let view_request = ViewRequest {
        function: format!("{}::group::{}", panora.aptos.contract_address, view_function),
        type_arguments: vec![],
        arguments: vec![
            serde_json::to_value(&group_id)?,
            serde_json::to_value(&proposal_entry.proposal_id)?,
        ],
    };
    
    // Call the smart contract with error handling
    let response = match panora.aptos.node.view_function(view_request).await {
        Ok(resp) => resp,
        Err(e) => {
            log::error!("Failed to call smart contract for DAO {}: {}", proposal_entry.proposal_id, e);
            return Err(anyhow::anyhow!("Smart contract call failed: {}", e));
        }
    };
    
    let dao_data = response.into_inner();
    
    // Parse the response (assuming it returns the DAO data)
    if let Some(dao_data_array) = dao_data.as_array() {
        if let Some(dao_info) = dao_data_array.first() {
            let dao_info: serde_json::Value = dao_info.clone();
        
            // Extract data from the response with error handling
            let empty_vec = vec![];
            let choices = dao_info["choices"].as_array().unwrap_or(&empty_vec);
            let choices_weights = dao_info["choices_weights"].as_array().unwrap_or(&empty_vec);
            let mut coin_type = dao_info["coin_type"].as_str().unwrap_or("Unknown");

            if coin_type == "0x1" {
                coin_type = "0x1::aptos_coin::AptosCoin"
            }

            let coin_data = panora.get_panora_token_list().await;

            if coin_data.is_err() {
                log::error!("Failed to get Panora token list: {}", coin_data.err().unwrap());
                return Err(anyhow::anyhow!("Failed to get Panora token list"));
            }

            let coin_data = coin_data.unwrap();

            let coin = coin_data.iter().find(|coin| coin.token_address.as_ref().unwrap_or(&"".to_string()).to_lowercase() == coin_type.to_lowercase() || coin.token_address.as_ref().unwrap_or(&"".to_string()).starts_with(coin_type.to_lowercase().as_str()) || coin.fa_address.to_lowercase() == coin_type.to_lowercase());

            if coin.is_none() {
                log::error!("Failed to find coin data for DAO {}: {}", proposal_entry.proposal_id, coin_type);
                return Err(anyhow::anyhow!("Failed to find coin data for DAO"));
            }

            let coin = coin.unwrap();
            
            let decimals = coin.decimals;
        
            // Find the winning option
            let mut max_votes = 0.0f64;
            let mut winning_index = 0;
            let mut total_votes = 0.0f64;
            
            for (index, weight) in choices_weights.iter().enumerate() {
                let votes = weight.as_str().unwrap_or("0").parse::<u64>().unwrap_or(0);
                let votes = votes as f64 / 10_f64.powi(decimals as i32);
                total_votes += votes;
                if votes > max_votes {
                    max_votes = votes;
                    winning_index = index;
                }
            }
            
            // Create results message (using MarkdownV2 formatting)
            let mut results_text = format!(
                "🏆 *DAO VOTING RESULTS*\n\n🏛️ *{}*\n📝 {}\n\n📊 *Results:*\n",
                escape_markdown_v2(&proposal_entry.name),
                escape_markdown_v2(&proposal_entry.description)
            );
            
            for (index, choice) in choices.iter().enumerate() {
                let choice_name = choice.as_str().unwrap_or("Unknown");
                let votes = choices_weights[index].as_str().unwrap_or("0").parse::<u64>().unwrap_or(0);
                let votes = votes as f64 / 10_f64.powi(decimals as i32);
                let percentage = if total_votes > 0.0f64 {
                    (votes as f64 / total_votes as f64 * 100.0) as f64
                } else {
                    0.0f64
                };
                
                let emoji = if index == winning_index { "🥇" } else { "📊" };
                results_text.push_str(&format!(
                    "{} *{}*: {} {:.2} votes \\({:.2}%\\)\n",
                    emoji, escape_markdown_v2(choice_name), coin.symbol, votes, percentage
                ).replace(".", "\\."));
            }
            
            if total_votes > 0.0f64 {
                let winning_choice = choices[winning_index].as_str().unwrap_or("Unknown");
                results_text.push_str(&format!(
                    "\n🎉 *Winner: {}* with {:.2} {} votes\\!\n📈 Total votes cast: {:.2} {}",
                    escape_markdown_v2(winning_choice), max_votes, coin.symbol, total_votes, coin.symbol
                ).replace(".", "\\."));
            } else {
                results_text.push_str("\n❌ No votes were cast for this DAO\\.");
            }
        
            // Send the results message with error handling
            match send_scheduled_message(
                bot,
                chat_group_id,
                &results_text,
                proposal_entry.thread_id,
            ).await
            {
                Ok(_) => {
                    log::info!("Sent DAO results for {} to group {}", proposal_entry.proposal_id, group_id);
                }
                Err(e) => {
                    log::error!("Failed to send DAO results for {} to group {}: {}", proposal_entry.proposal_id, group_id, e);
                    return Err(anyhow::anyhow!("Failed to send message: {}", e));
                }
            }
        } else {
            log::warn!("No DAO data found in response for DAO: {}", proposal_entry.proposal_id);
            // Send a simple completion message with error handling
            if let Err(e) = send_scheduled_message(
                bot,
                chat_group_id,
                &format!("🏛️ DAO \"{}\" has ended.", proposal_entry.name),
                proposal_entry.thread_id,
            ).await {
                log::error!("Failed to send fallback message for DAO {}: {}", proposal_entry.proposal_id, e);
            }
        }
    } else {
        log::warn!("No data returned from smart contract for DAO: {}", proposal_entry.proposal_id);
        // Send a simple completion message with error handling
        if let Err(e) = send_scheduled_message(
            bot,
            chat_group_id,
            &format!("🏛️ DAO \"{}\" has ended.", proposal_entry.name),
            proposal_entry.thread_id,
        ).await {
            log::error!("Failed to send fallback message for DAO {}: {}", proposal_entry.proposal_id, e);
        }
    }
    
    Ok(())
}

pub fn job_dao_results_cleanup(dao: Dao) -> Job {
    // Run every day at 00:00
    Job::new_async("0 0 0 * * *", move |_uuid, _l| {
        let dao = dao.clone();
        Box::pin(async move {
            log::info!("Starting DAO cleanup job at {}", Utc::now());
            
            match dao.remove_expired_daos() {
                Ok(_) => {
                    log::info!("Successfully completed DAO cleanup job");
                }
                Err(e) => {
                    log::error!("Failed to remove expired DAOs: {}", e);
                }
            }
        })
    })
    .expect("Failed to create cron job")
}

