use serde::{Deserialize, Serialize};

use crate::scheduled_prompts::dto::RepeatPolicy;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum PendingPaymentStep {
    AwaitingRecipient,
    AwaitingToken,
    AwaitingAmount,
    AwaitingDate,
    AwaitingHour,
    AwaitingMinute,
    AwaitingRepeat,
    AwaitingConfirm,
}

#[derive(Clone, Debug, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub struct ScheduledPaymentRecord {
    pub id: String,
    pub group_id: i64,
    pub creator_user_id: i64,
    pub creator_username: String,
    pub recipient_username: Option<String>,
    pub recipient_address: Option<String>,
    pub symbol: Option<String>,
    pub token_type: Option<String>,
    pub decimals: Option<u8>,
    pub amount_smallest_units: Option<u64>,
    pub start_timestamp_utc: Option<i64>,
    pub repeat: RepeatPolicy,
    // For weekly cadence variants (1, 2, or 4 weeks). None means not weekly-based
    pub weekly_weeks: Option<u8>,
    pub active: bool,
    pub created_at: i64,
    pub last_run_at: Option<i64>,
    pub next_run_at: Option<i64>,
    pub run_count: u64,
    pub locked_until: Option<i64>,
    pub scheduler_job_id: Option<String>,
    pub last_error: Option<String>,
    pub last_attempt_status: Option<String>,
    pub notify_on_success: bool,
    pub notify_on_failure: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub struct PendingPaymentWizardState {
    pub group_id: i64,
    pub creator_user_id: i64,
    pub creator_username: String,
    pub step: PendingPaymentStep,
    pub schedule_id: Option<String>,
    pub recipient_username: Option<String>,
    pub recipient_address: Option<String>,
    pub symbol: Option<String>,
    pub token_type: Option<String>,
    pub decimals: Option<u8>,
    pub amount_display: Option<f64>,
    pub date: Option<String>,
    pub hour_utc: Option<u8>,
    pub minute_utc: Option<u8>,
    pub repeat: Option<RepeatPolicy>,
    pub weekly_weeks: Option<u8>,
    #[serde(default)]
    pub current_bot_message_id: Option<i32>,
    #[serde(default)]
    pub user_message_ids: Vec<i32>,
}


