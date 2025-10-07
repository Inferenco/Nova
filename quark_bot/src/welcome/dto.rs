use serde::{Deserialize, Serialize};
use teloxide::types::{ChatId, UserId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WelcomeSettings {
    pub enabled: bool,
    pub custom_message: Option<String>,
    pub verification_timeout: u64, // in seconds
    pub verification_success_count: u64,
    pub verification_failure_count: u64,
    pub last_updated: i64, // unix timestamp
}

impl Default for WelcomeSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            custom_message: None,
            verification_timeout: 300, // 5 minutes default
            verification_success_count: 0,
            verification_failure_count: 0,
            last_updated: chrono::Utc::now().timestamp(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingVerification {
    pub user_id: UserId,
    pub username: Option<String>,
    pub first_name: String,
    pub chat_id: ChatId,
    pub joined_at: i64, // unix timestamp
    pub expires_at: i64, // unix timestamp
    pub verification_message_id: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WelcomeStats {
    pub total_verifications: u64,
    pub successful_verifications: u64,
    pub failed_verifications: u64,
    pub success_rate: f64,
    pub last_verification: Option<i64>, // unix timestamp
}

impl Default for WelcomeStats {
    fn default() -> Self {
        Self {
            total_verifications: 0,
            successful_verifications: 0,
            failed_verifications: 0,
            success_rate: 0.0,
            last_verification: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WelcomeMessageTemplate {
    pub message: String,
    pub placeholders: Vec<String>,
}

impl Default for WelcomeMessageTemplate {
    fn default() -> Self {
        Self {
            message: "👋 Welcome to {group_name}, @{username}!\n\n🔒 Please verify you're human by clicking the button below within {timeout} minutes.\n\n⚠️ You'll be automatically removed if you don't verify in time.".to_string(),
            placeholders: vec!["{username}".to_string(), "{group_name}".to_string(), "{timeout}".to_string()],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PendingWelcomeStep {
    AwaitingCustomMessage,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingWelcomeWizardState {
    pub group_id: i64,
    pub initiator_user_id: i64,
    pub step: PendingWelcomeStep,
    pub custom_message: Option<String>,
    #[serde(default)]
    pub current_bot_message_id: Option<i32>,
    #[serde(default)]
    pub user_message_ids: Vec<i32>,
}
