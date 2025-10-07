use std::env;

use anyhow::Result;
use sled::Tree;
use teloxide::{
    prelude::*,
    types::{ChatId, ChatPermissions, InlineKeyboardButton, InlineKeyboardMarkup, UserId},
};

use crate::welcome::{
    dto::{PendingVerification, PendingWelcomeWizardState, WelcomeSettings, WelcomeStats},
    helpers::{get_custom_welcome_message, get_verification_expiry_time, is_verification_expired},
};
use crate::utils::{escape_for_markdown_v2};

use rand::{SeedableRng, prelude::*, rngs::StdRng};

#[derive(Clone)]
pub struct WelcomeService {
    settings_db: Tree,
    verifications_db: Tree,
    stats_db: Tree,
    pending_db: Tree,
    account_seed: String,
}

impl WelcomeService {
    pub fn new(db: sled::Db) -> Self {
        let settings_db = db
            .open_tree("welcome_settings")
            .expect("Failed to open welcome settings tree");
        let verifications_db = db
            .open_tree("welcome_verifications")
            .expect("Failed to open welcome verifications tree");
        let stats_db = db
            .open_tree("welcome_stats")
            .expect("Failed to open welcome stats tree");
        let pending_db = db
            .open_tree("welcome_pending")
            .expect("Failed to open welcome pending tree");

        let account_seed: String =
            env::var("ACCOUNT_SEED").expect("ACCOUNT_SEED environment variable not found");

        Self {
            settings_db,
            verifications_db,
            stats_db,
            pending_db,
            account_seed,
        }
    }

    pub fn get_settings(&self, chat_id: ChatId) -> WelcomeSettings {
        let key = format!("{}-{}", chat_id.to_string(), self.account_seed);

        if let Ok(Some(bytes)) = self.settings_db.get(key.as_bytes()) {
            if let Ok(settings) = serde_json::from_slice(&bytes) {
                return settings;
            }
        }

        WelcomeSettings::default()
    }

    pub fn save_settings(&self, chat_id: ChatId, settings: WelcomeSettings) -> Result<()> {
        let key = format!("{}-{}", chat_id.to_string(), self.account_seed);
        let bytes = serde_json::to_vec(&settings)?;

        self.settings_db.insert(key.as_bytes(), bytes)?;
        Ok(())
    }

    pub fn is_enabled(&self, chat_id: ChatId) -> bool {
        self.get_settings(chat_id).enabled
    }

    pub async fn handle_new_member(
        &self,
        bot: &Bot,
        chat_id: ChatId,
        user_id: UserId,
        username: Option<String>,
        first_name: String,
    ) -> Result<()> {
        if !self.is_enabled(chat_id) {
            return Ok(());
        }

        // Check if this user is already being processed to prevent duplicates
        let key = format!(
            "{}-{}:{}",
            chat_id.to_string(),
            self.account_seed,
            user_id.to_string()
        );
        if let Ok(Some(_)) = self.verifications_db.get(key.as_bytes()) {
            log::info!(
                "User {} in chat {} is already being processed, skipping duplicate",
                user_id.to_string(),
                chat_id.to_string()
            );
            return Ok(());
        }

        let settings = self.get_settings(chat_id);

        // Mute the new member immediately
        let restricted_permissions = ChatPermissions::empty();
        bot.restrict_chat_member(chat_id, user_id, restricted_permissions)
            .await?;

        // Get chat title for welcome message
        let chat = bot.get_chat(chat_id).await?;
        let group_name = chat.title().unwrap_or("this group").to_string();

        // Create verification button
        let keyboard = InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
            "✅ Prove You're Human",
            format!(
                "welcome_verify:{}:{}",
                chat_id.to_string(),
                user_id.to_string()
            ),
        )]]);

        // Send welcome message with verification button
        // Prefer the user's actual @username in a safe clickable mention; fall back to first name
        let display_name = if let Some(ref u) = username {
            format!("@{}", u)
        } else {
            first_name.clone()
        };
        // Escape display text for MarkdownV2, then create a tg:// user mention link
        let escaped_display = escape_for_markdown_v2(&display_name);
        let mention_markup = format!("[{}](tg://user?id={})", escaped_display, user_id.0);
        let welcome_text = get_custom_welcome_message(&settings, &mention_markup, &group_name);
        let message = bot
            .send_message(chat_id, welcome_text)
            .parse_mode(teloxide::types::ParseMode::MarkdownV2)
            .reply_markup(keyboard)
            .await?;

        // Store pending verification
        let verification = PendingVerification {
            user_id,
            username,
            first_name,
            chat_id,
            joined_at: chrono::Utc::now().timestamp(),
            expires_at: get_verification_expiry_time(settings.verification_timeout),
            verification_message_id: message.id.0,
        };

        let key = format!("{}-{}:{}", chat_id.0, self.account_seed, user_id.0);
        let bytes = serde_json::to_vec(&verification)?;
        self.verifications_db.insert(key.as_bytes(), bytes)?;

        Ok(())
    }

    pub async fn handle_verification(
        &self,
        bot: &Bot,
        chat_id: ChatId,
        user_id: UserId,
        requester_id: UserId, // Add the ID of the user who clicked the button
    ) -> Result<()> {
        // Verify that the user clicking the button is the same user who joined
        if requester_id != user_id {
            log::warn!(
                "User {} attempted to verify user {} in chat {}",
                requester_id.to_string(),
                user_id.to_string(),
                chat_id.to_string()
            );
            return Err(anyhow::anyhow!("You can only verify yourself"));
        }

        let key = format!(
            "{}-{}:{}",
            chat_id.0,
            self.account_seed,
            user_id.0
        );

        // Get verification record
        let verification = if let Ok(Some(bytes)) = self.verifications_db.get(key.as_bytes()) {
            if let Ok(verification) = serde_json::from_slice::<PendingVerification>(&bytes) {
                log::info!(
                    "Found verification record for user {}: expires at {}",
                    user_id.0,
                    verification.expires_at
                );
                verification
            } else {
                log::error!(
                    "Failed to deserialize verification data for user {}",
                    user_id.0
                );
                return Err(anyhow::anyhow!("Invalid verification data"));
            }
        } else {
            log::error!(
                "No verification record found for user {} in chat {}",
                user_id.0,
                chat_id.0
            );
            return Err(anyhow::anyhow!("Verification not found"));
        };

        // Check if verification is expired
        if is_verification_expired(verification.expires_at) {
            log::warn!(
                "Verification expired for user {} in chat {}",
                user_id.to_string(),
                chat_id.to_string()
            );
            return Err(anyhow::anyhow!("Verification expired"));
        }

        log::info!(
            "Attempting to unmute user {} in chat {}",
            user_id.to_string(),
            chat_id.to_string()
        );

        // Check if bot has admin permissions (only once)
        let bot_info = bot.get_me().await?;
        let chat_member = bot.get_chat_member(chat_id, bot_info.id).await?;

        if !chat_member.is_privileged() {
            log::error!(
                "Bot is not an admin in chat {}, cannot perform verification actions",
                chat_id.to_string()
            );
            return Err(anyhow::anyhow!("Bot is not an admin in this chat"));
        }

        // Unmute the user
        let full_permissions = ChatPermissions::all();
        match bot
            .restrict_chat_member(chat_id, user_id, full_permissions)
            .await
        {
            Ok(_) => log::info!(
                "Successfully unmuted user {} in chat {}",
                user_id.to_string(),
                chat_id.to_string()
            ),
            Err(e) => {
                log::error!(
                    "Failed to unmute user {} in chat {}: {}",
                    user_id.to_string(),
                    chat_id.to_string(),
                    e
                );
                return Err(anyhow::anyhow!("Failed to unmute user: {}", e));
            }
        }

        log::info!(
            "Updating verification message for user {} in chat {}",
            user_id.to_string(),
            chat_id.to_string()
        );

        // Update verification message
        let success_text = format!(
            "✅ Welcome to the group, {}\\! You've been verified and can now participate\\.",
            escape_for_markdown_v2(&verification.first_name)
        );

        match bot
            .edit_message_text(
                chat_id,
                teloxide::types::MessageId(verification.verification_message_id),
                success_text,
            )
            .parse_mode(teloxide::types::ParseMode::MarkdownV2)
            .await
        {
            Ok(_) => log::info!(
                "Successfully updated verification message for user {} in chat {}",
                user_id.to_string(),
                chat_id.to_string()
            ),
            Err(e) => {
                log::error!(
                    "Failed to update verification message for user {} in chat {}: {}",
                    user_id.to_string(),
                    chat_id.to_string(),
                    e
                );
                return Err(anyhow::anyhow!("Failed to update message: {}", e));
            }
        }

        log::info!(
            "Removing verification record for user {} in chat {}",
            user_id.to_string(),
            chat_id.to_string()
        );

        // Remove verification record
        if let Err(e) = self.verifications_db.remove(key.as_bytes()) {
            log::error!(
                "Failed to remove verification record for user {} in chat {}: {}",
                user_id.to_string(),
                chat_id.to_string(),
                e
            );
            return Err(anyhow::anyhow!(
                "Failed to remove verification record: {}",
                e
            ));
        }

        log::info!(
            "Updating statistics for user {} in chat {}",
            user_id.to_string(),
            chat_id.to_string()
        );

        // Update statistics
        if let Err(e) = self.update_stats(chat_id, true) {
            log::error!(
                "Failed to update stats for user {} in chat {}: {}",
                user_id.to_string(),
                chat_id.to_string(),
                e
            );
            return Err(anyhow::anyhow!("Failed to update stats: {}", e));
        }

        log::info!(
            "Verification completed successfully for user {} in chat {}",
            user_id.to_string(),
            chat_id.to_string()
        );
        Ok(())
    }

    fn update_stats(&self, chat_id: ChatId, success: bool) -> Result<()> {
        let key = format!("{}-{}", chat_id.to_string(), self.account_seed);

        let mut stats = if let Ok(Some(bytes)) = self.stats_db.get(key.as_bytes()) {
            if let Ok(stats) = serde_json::from_slice::<WelcomeStats>(&bytes) {
                stats
            } else {
                WelcomeStats::default()
            }
        } else {
            WelcomeStats::default()
        };

        stats.total_verifications += 1;
        if success {
            stats.successful_verifications += 1;
        } else {
            stats.failed_verifications += 1;
        }

        stats.success_rate =
            (stats.successful_verifications as f64 / stats.total_verifications as f64) * 100.0;
        stats.last_verification = Some(chrono::Utc::now().timestamp());

        let bytes = serde_json::to_vec(&stats)?;
        self.stats_db.insert(key.as_bytes(), bytes)?;

        Ok(())
    }

    pub fn get_stats(&self, chat_id: ChatId) -> WelcomeStats {
        let key = format!("{}-{}", chat_id.to_string(), self.account_seed);

        if let Ok(Some(bytes)) = self.stats_db.get(key.as_bytes()) {
            if let Ok(stats) = serde_json::from_slice::<WelcomeStats>(&bytes) {
                return stats;
            }
        }

        WelcomeStats::default()
    }

    pub async fn cleanup_all_expired_verifications(&self, bot: &Bot) -> Result<()> {
        let mut expired_verifications = Vec::new();

        // Collect all expired verifications
        for result in self.verifications_db.iter() {
            if let Ok((key, value)) = result {
                if let Ok(verification) = serde_json::from_slice::<PendingVerification>(&value) {
                    if is_verification_expired(verification.expires_at) {
                        expired_verifications.push((key, verification));
                    }
                }
            }
        }

        let count = expired_verifications.len();

        // Process each expired verification
        for (key, verification) in expired_verifications {
            let mut rng = StdRng::from_seed([0; 32]);
            log::info!(
                "Cleaning up expired verification for user {} in chat {}",
                verification.user_id.to_string(),
                verification.chat_id.to_string()
            );

            let mut range: Vec<i64> = (5..60).collect();
            range.shuffle(&mut rng);

            let mut time_option = range.choose(&mut rng);

            let time = loop {
                if let Some(time) = time_option {
                    break time;
                }
                time_option = range.choose(&mut rng);
            };

            let until_date = chrono::Utc::now() + chrono::Duration::minutes(*time);

            // Remove user from group
            let kick_result = bot
                .kick_chat_member(verification.chat_id, verification.user_id)
                .until_date(until_date)
                .revoke_messages(false)
                .await;

            if let Err(e) = kick_result {
                log::error!(
                    "Failed to kick expired verification user {}: {}",
                    verification.user_id.to_string(),
                    e
                );
            }

            // Update verification message
            let expired_text = format!(
                "⏰ Verification expired for {}\\. User has been removed from the group until {}\\.",
                escape_for_markdown_v2(&verification.first_name),
                escape_for_markdown_v2(&until_date.format("%Y-%m-%d %H:%M:%S").to_string())
            );

            if let Err(e) = bot
                .edit_message_text(
                    verification.chat_id,
                    teloxide::types::MessageId(verification.verification_message_id),
                    expired_text,
                )
                .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                .await
            {
                log::error!(
                    "Failed to update expired verification message for user {} in chat {}: {}",
                    verification.user_id.to_string(),
                    verification.chat_id.to_string(),
                    e
                );
            }

            // Remove verification record
            if let Err(e) = self.verifications_db.remove(&key) {
                log::error!("Failed to remove expired verification record: {}", e);
            }

            // Update statistics
            if let Err(e) = self.update_stats(verification.chat_id, false) {
                log::error!("Failed to update stats for expired verification: {}", e);
            }
        }

        if count > 0 {
            log::info!("Cleaned up {} expired verifications", count);
        }

        Ok(())
    }

    pub fn reset_stats(&self, chat_id: ChatId) -> Result<()> {
        let key = format!("{}-{}", chat_id.to_string(), self.account_seed);
        self.stats_db.remove(key.as_bytes())?;
        Ok(())
    }

    // Wizard state management methods
    pub fn put_pending_wizard(&self, key: String, state: &PendingWelcomeWizardState) -> Result<()> {
        let bytes = serde_json::to_vec(state)?;
        self.pending_db.insert(key.as_bytes(), bytes)?;
        Ok(())
    }

    pub fn get_pending_wizard(&self, key: &str) -> Option<PendingWelcomeWizardState> {
        if let Ok(Some(bytes)) = self.pending_db.get(key.as_bytes()) {
            if let Ok(state) = serde_json::from_slice(&bytes) {
                return Some(state);
            }
        }
        None
    }

    pub fn remove_pending_wizard(&self, key: &str) -> Result<()> {
        self.pending_db.remove(key.as_bytes())?;
        Ok(())
    }

    pub fn account_seed(&self) -> &str {
        &self.account_seed
    }

    pub fn cleanup_expired_wizard_states(&self) -> Result<()> {
        let mut expired_keys = Vec::new();
        let now = chrono::Utc::now().timestamp();

        for result in self.pending_db.iter() {
            if let Ok((key, value)) = result {
                if let Ok(state) = serde_json::from_slice::<PendingWelcomeWizardState>(&value) {
                    // Expire wizard states older than 10 minutes
                    if now - state.created_at > 600 {
                        expired_keys.push(key);
                    }
                }
            }
        }

        for key in expired_keys {
            self.pending_db.remove(&key)?;
        }

        Ok(())
    }
}



