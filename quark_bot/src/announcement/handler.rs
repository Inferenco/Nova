use std::collections::HashSet;

use anyhow::Result;
use futures::stream::{self, StreamExt};
use teloxide::{
    Bot,
    prelude::*,
    types::{Message, ParseMode, UserId},
};

use crate::dependencies::BotDependencies;

use super::announcement::AnnouncerAuth;

pub async fn handle_announcement(
    bot: Bot,
    msg: Message,
    text: String,
    bot_deps: BotDependencies,
) -> Result<()> {
    // Extract sender's username
    let sender = match msg.from.as_ref() {
        Some(user) => user,
        None => {
            bot.send_message(msg.chat.id, "❌ Unable to identify sender.")
                .await?;
            return Ok(());
        }
    };

    let username = match &sender.username {
        Some(username) => username,
        None => {
            bot.send_message(
                msg.chat.id,
                "❌ Username required. Please set a Telegram username to use announcements.",
            )
            .await?;
            return Ok(());
        }
    };

    // Create announcer auth instance
    let config_path = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("config/authorized_announcers.ron");

    let announcer_auth = match AnnouncerAuth::new(&config_path) {
        Ok(auth) => auth,
        Err(e) => {
            log::error!("Failed to load announcer auth: {}", e);
            bot.send_message(
                msg.chat.id,
                "❌ Configuration error. Please contact an administrator.",
            )
            .await?;
            return Ok(());
        }
    };

    // Check authorization
    if !announcer_auth.is_authorized(username) {
        bot.send_message(
            msg.chat.id,
            "❌ You are not authorized to send global announcements.",
        )
        .await?;
        return Ok(());
    }

    // Verify the sender is logged in
    if !bot_deps.auth.verify(msg.clone()).await {
        bot.send_message(
            msg.chat.id,
            "❌ You must be logged in to send announcements. Use /loginuser first.",
        )
        .await?;
        return Ok(());
    }

    // Check if announcement text is empty
    if text.trim().is_empty() {
        bot.send_message(
            msg.chat.id,
            "📢 **Announcement Usage**\n\nTo send a global announcement:\n`/globalannouncement Your message here`\n\nThe announcement will be sent to all logged-in users.",
        )
        .await?;
        return Ok(());
    }

    // Gather recipients
    let recipients = match gather_recipients(&bot_deps).await {
        Ok(users) => users,
        Err(e) => {
            log::error!("Failed to gather recipients: {}", e);
            bot.send_message(msg.chat.id, "❌ Failed to gather recipient list.")
                .await?;
            return Ok(());
        }
    };

    log::info!("Sending announcement to {} recipients", recipients.len());

    // Confirm sending
    bot.send_message(
        msg.chat.id,
        &format!("📢 Sending announcement to {} users...", recipients.len()),
    )
    .await?;

    // Prepare announcement message with header
    let announcement_text = format!("📢 <b>GLOBAL ANNOUNCEMENT</b>\n\n{}", text);

    let recipient_count = recipients.len();

    // Send announcements with rate limiting using concurrent approach
    stream::iter(recipients)
        .for_each_concurrent(10, |user_id| {
            let bot = bot.clone();
            let announcement_text = announcement_text.clone();

            async move {
                // Small delay per task to respect API limits
                tokio::time::sleep(tokio::time::Duration::from_millis(75)).await;

                match send_announcement_to_user(bot, user_id, &announcement_text).await {
                    Ok(_) => {
                        log::debug!("Successfully sent announcement to user {}", user_id);
                    }
                    Err(e) => {
                        log::warn!("Failed to send announcement to user {}: {}", user_id, e);
                    }
                }
            }
        })
        .await;

    // Send completion message
    bot.send_message(
        msg.chat.id,
        &format!("✅ Announcement sent to {} users.", recipient_count),
    )
    .await?;

    Ok(())
}

async fn gather_recipients(bot_deps: &BotDependencies) -> Result<HashSet<UserId>> {
    let mut recipients = HashSet::new();

    // Get all logged-in users from the Auth store only
    let users = bot_deps.auth.get_all_users()?;
    for user in users {
        recipients.insert(user.user_id);
    }

    // Per feedback: recipients should be determined solely from the auth store.
    // No need to iterate group memberships.

    log::info!("Gathered {} unique recipients", recipients.len());
    Ok(recipients)
}

async fn send_announcement_to_user(bot: Bot, user_id: UserId, text: &str) -> Result<()> {
    // Handle long messages by splitting them
    const TELEGRAM_MESSAGE_LIMIT: usize = 4096;
    let safe = crate::utils::sanitize_telegram_html(text);

    if safe.len() > TELEGRAM_MESSAGE_LIMIT {
        let chunks = split_text(&safe, TELEGRAM_MESSAGE_LIMIT);
        for chunk in chunks {
            bot.send_message(user_id, chunk)
                .parse_mode(ParseMode::Html)
                .await?;
        }
    } else {
        bot.send_message(user_id, safe)
            .parse_mode(ParseMode::Html)
            .await?;
    }

    Ok(())
}

fn split_text(text: &str, limit: usize) -> Vec<String> {
    if text.len() <= limit {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut current_chunk = String::new();

    for line in text.lines() {
        if current_chunk.len() + line.len() + 1 > limit {
            if !current_chunk.is_empty() {
                chunks.push(current_chunk.trim().to_string());
                current_chunk.clear();
            }
        }

        if !current_chunk.is_empty() {
            current_chunk.push('\n');
        }
        current_chunk.push_str(line);
    }

    if !current_chunk.is_empty() {
        chunks.push(current_chunk.trim().to_string());
    }

    chunks
}
