use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

// Legacy structs for backward compatibility during migration
#[derive(Clone, Debug, Serialize, Deserialize, Encode, Decode)]
pub struct LegacyScheduledPromptRecord {
    pub id: String,
    pub group_id: i64,
    pub creator_user_id: i64,
    pub creator_username: String,
    pub prompt: String,
    // image_url field missing - this is the old format
    pub start_hour_utc: u8,
    pub start_minute_utc: u8,
    pub repeat: RepeatPolicy,
    pub active: bool,
    pub created_at: i64,
    pub last_run_at: Option<i64>,
    pub next_run_at: Option<i64>,
    pub run_count: u64,
    pub locked_until: Option<i64>,
    pub scheduler_job_id: Option<String>,
    pub conversation_response_id: Option<String>,
    pub thread_id: Option<i32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Encode, Decode)]
pub struct LegacyPendingWizardState {
    pub group_id: i64,
    pub creator_user_id: i64,
    pub creator_username: String,
    pub step: PendingStep,
    pub prompt: Option<String>,
    // image_url field missing - this is the old format
    pub hour_utc: Option<u8>,
    pub minute_utc: Option<u8>,
    pub repeat: Option<RepeatPolicy>,
    pub thread_id: Option<i32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Encode, Decode)]
pub enum RepeatPolicy {
    None,
    Every5m,
    Every15m,
    Every30m,
    Every45m,
    Every1h,
    Every3h,
    Every6h,
    Every12h,
    Daily,
    Weekly,
    Monthly,
}

#[derive(Clone, Debug, Serialize, Deserialize, Encode, Decode)]
pub struct ScheduledPromptRecord {
    pub id: String,
    pub group_id: i64,
    pub creator_user_id: i64,
    pub creator_username: String,
    pub prompt: String,
    pub image_url: Option<String>,
    pub start_hour_utc: u8,
    pub start_minute_utc: u8,
    pub repeat: RepeatPolicy,
    pub active: bool,
    pub created_at: i64,
    pub last_run_at: Option<i64>,
    pub next_run_at: Option<i64>,
    pub run_count: u64,
    pub locked_until: Option<i64>,
    pub scheduler_job_id: Option<String>,
    pub conversation_response_id: Option<String>,
    pub thread_id: Option<i32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Encode, Decode)]
pub enum PendingStep {
    AwaitingPrompt,
    AwaitingImage,
    AwaitingHour,
    AwaitingMinute,
    AwaitingRepeat,
    AwaitingConfirm,
}

#[derive(Clone, Debug, Serialize, Deserialize, Encode, Decode)]
pub struct PendingWizardState {
    pub group_id: i64,
    pub creator_user_id: i64,
    pub creator_username: String,
    pub step: PendingStep,
    pub prompt: Option<String>,
    pub image_url: Option<String>,
    pub hour_utc: Option<u8>,
    pub minute_utc: Option<u8>,
    pub repeat: Option<RepeatPolicy>,
    pub thread_id: Option<i32>,
}
