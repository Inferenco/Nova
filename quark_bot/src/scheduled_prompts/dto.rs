use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

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

#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Encode, Decode)]
struct ScheduledPromptRecordV1 {
    id: String,
    group_id: i64,
    creator_user_id: i64,
    creator_username: String,
    prompt: String,
    start_hour_utc: u8,
    start_minute_utc: u8,
    repeat: RepeatPolicy,
    active: bool,
    created_at: i64,
    last_run_at: Option<i64>,
    next_run_at: Option<i64>,
    run_count: u64,
    locked_until: Option<i64>,
    scheduler_job_id: Option<String>,
    conversation_response_id: Option<String>,
    thread_id: Option<i32>,
}

#[derive(Encode, Decode)]
struct ScheduledPromptRecordV2 {
    id: String,
    group_id: i64,
    creator_user_id: i64,
    creator_username: String,
    prompt: String,
    image_url: Option<String>,
    start_hour_utc: u8,
    start_minute_utc: u8,
    repeat: RepeatPolicy,
    active: bool,
    created_at: i64,
    last_run_at: Option<i64>,
    next_run_at: Option<i64>,
    run_count: u64,
    locked_until: Option<i64>,
    scheduler_job_id: Option<String>,
    conversation_response_id: Option<String>,
    thread_id: Option<i32>,
}

impl From<ScheduledPromptRecordV1> for ScheduledPromptRecord {
    fn from(value: ScheduledPromptRecordV1) -> Self {
        Self {
            id: value.id,
            group_id: value.group_id,
            creator_user_id: value.creator_user_id,
            creator_username: value.creator_username,
            prompt: value.prompt,
            image_url: None,
            start_hour_utc: value.start_hour_utc,
            start_minute_utc: value.start_minute_utc,
            repeat: value.repeat,
            active: value.active,
            created_at: value.created_at,
            last_run_at: value.last_run_at,
            next_run_at: value.next_run_at,
            run_count: value.run_count,
            locked_until: value.locked_until,
            scheduler_job_id: value.scheduler_job_id,
            conversation_response_id: value.conversation_response_id,
            thread_id: value.thread_id,
        }
    }
}

impl From<ScheduledPromptRecordV2> for ScheduledPromptRecord {
    fn from(value: ScheduledPromptRecordV2) -> Self {
        Self {
            id: value.id,
            group_id: value.group_id,
            creator_user_id: value.creator_user_id,
            creator_username: value.creator_username,
            prompt: value.prompt,
            image_url: value.image_url,
            start_hour_utc: value.start_hour_utc,
            start_minute_utc: value.start_minute_utc,
            repeat: value.repeat,
            active: value.active,
            created_at: value.created_at,
            last_run_at: value.last_run_at,
            next_run_at: value.next_run_at,
            run_count: value.run_count,
            locked_until: value.locked_until,
            scheduler_job_id: value.scheduler_job_id,
            conversation_response_id: value.conversation_response_id,
            thread_id: value.thread_id,
        }
    }
}

impl From<&ScheduledPromptRecord> for ScheduledPromptRecordV2 {
    fn from(value: &ScheduledPromptRecord) -> Self {
        Self {
            id: value.id.clone(),
            group_id: value.group_id,
            creator_user_id: value.creator_user_id,
            creator_username: value.creator_username.clone(),
            prompt: value.prompt.clone(),
            image_url: value.image_url.clone(),
            start_hour_utc: value.start_hour_utc,
            start_minute_utc: value.start_minute_utc,
            repeat: value.repeat.clone(),
            active: value.active,
            created_at: value.created_at,
            last_run_at: value.last_run_at,
            next_run_at: value.next_run_at,
            run_count: value.run_count,
            locked_until: value.locked_until,
            scheduler_job_id: value.scheduler_job_id.clone(),
            conversation_response_id: value.conversation_response_id.clone(),
            thread_id: value.thread_id,
        }
    }
}

#[derive(Encode, Decode)]
struct PendingWizardStateV1 {
    group_id: i64,
    creator_user_id: i64,
    creator_username: String,
    step: PendingStep,
    prompt: Option<String>,
    hour_utc: Option<u8>,
    minute_utc: Option<u8>,
    repeat: Option<RepeatPolicy>,
    thread_id: Option<i32>,
}

#[derive(Encode, Decode)]
struct PendingWizardStateV2 {
    group_id: i64,
    creator_user_id: i64,
    creator_username: String,
    step: PendingStep,
    prompt: Option<String>,
    image_url: Option<String>,
    hour_utc: Option<u8>,
    minute_utc: Option<u8>,
    repeat: Option<RepeatPolicy>,
    thread_id: Option<i32>,
}

impl From<PendingWizardStateV1> for PendingWizardState {
    fn from(value: PendingWizardStateV1) -> Self {
        Self {
            group_id: value.group_id,
            creator_user_id: value.creator_user_id,
            creator_username: value.creator_username,
            step: value.step,
            prompt: value.prompt,
            image_url: None,
            hour_utc: value.hour_utc,
            minute_utc: value.minute_utc,
            repeat: value.repeat,
            thread_id: value.thread_id,
        }
    }
}

impl From<PendingWizardStateV2> for PendingWizardState {
    fn from(value: PendingWizardStateV2) -> Self {
        Self {
            group_id: value.group_id,
            creator_user_id: value.creator_user_id,
            creator_username: value.creator_username,
            step: value.step,
            prompt: value.prompt,
            image_url: value.image_url,
            hour_utc: value.hour_utc,
            minute_utc: value.minute_utc,
            repeat: value.repeat,
            thread_id: value.thread_id,
        }
    }
}

impl From<&PendingWizardState> for PendingWizardStateV2 {
    fn from(value: &PendingWizardState) -> Self {
        Self {
            group_id: value.group_id,
            creator_user_id: value.creator_user_id,
            creator_username: value.creator_username.clone(),
            step: value.step.clone(),
            prompt: value.prompt.clone(),
            image_url: value.image_url.clone(),
            hour_utc: value.hour_utc,
            minute_utc: value.minute_utc,
            repeat: value.repeat.clone(),
            thread_id: value.thread_id,
        }
    }
}

pub fn encode_scheduled_prompt_record(record: &ScheduledPromptRecord) -> Vec<u8> {
    let stored: ScheduledPromptRecordV2 = record.into();
    bincode::encode_to_vec(stored, bincode::config::standard())
        .expect("scheduled prompt record should encode")
}

pub fn decode_scheduled_prompt_record(
    bytes: &[u8],
) -> Result<ScheduledPromptRecord, bincode::error::DecodeError> {
    let config = bincode::config::standard();
    match bincode::decode_from_slice::<ScheduledPromptRecordV2, _>(bytes, config) {
        Ok((rec, _)) => Ok(rec.into()),
        Err(err_v2) => {
            match bincode::decode_from_slice::<ScheduledPromptRecordV1, _>(bytes, config) {
                Ok((rec, _)) => Ok(rec.into()),
                Err(_) => Err(err_v2),
            }
        }
    }
}

pub fn encode_pending_wizard_state(state: &PendingWizardState) -> Vec<u8> {
    let stored: PendingWizardStateV2 = state.into();
    bincode::encode_to_vec(stored, bincode::config::standard())
        .expect("pending wizard state should encode")
}

pub fn decode_pending_wizard_state(
    bytes: &[u8],
) -> Result<PendingWizardState, bincode::error::DecodeError> {
    let config = bincode::config::standard();
    match bincode::decode_from_slice::<PendingWizardStateV2, _>(bytes, config) {
        Ok((state, _)) => Ok(state.into()),
        Err(err_v2) => match bincode::decode_from_slice::<PendingWizardStateV1, _>(bytes, config) {
            Ok((state, _)) => Ok(state.into()),
            Err(_) => Err(err_v2),
        },
    }
}
