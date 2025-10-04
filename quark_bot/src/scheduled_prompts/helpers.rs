use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};
use crate::scheduled_prompts::dto::{PendingStep, PendingWizardState, RepeatPolicy};

// Removed non-nav keyboard builders to avoid dead_code warnings. Using nav variants below.

pub fn summarize(state: &PendingWizardState) -> String {
    let prompt = state.prompt.as_deref().unwrap_or("");
    let image_info = if let Some(url) = &state.image_url {
        format!("\n\n📷 Image: <a href=\"{}\">View Image</a>", url)
    } else {
        String::new()
    };
    let hour = state.hour_utc.map(|h| format!("{:02}", h)).unwrap_or("--".into());
    let minute = state.minute_utc.map(|m| format!("{:02}", m)).unwrap_or("--".into());
    let repeat = match state.repeat {
        Some(RepeatPolicy::None) => "No repeat".to_string(),
        Some(RepeatPolicy::Every5m) => "Every 5 min".to_string(),
        Some(RepeatPolicy::Every15m) => "Every 15 min".to_string(),
        Some(RepeatPolicy::Every30m) => "Every 30 min".to_string(),
        Some(RepeatPolicy::Every45m) => "Every 45 min".to_string(),
        Some(RepeatPolicy::Every1h) => "Every 1 hour".to_string(),
        Some(RepeatPolicy::Every3h) => "Every 3 hours".to_string(),
        Some(RepeatPolicy::Every6h) => "Every 6 hours".to_string(),
        Some(RepeatPolicy::Every12h) => "Every 12 hours".to_string(),
        Some(RepeatPolicy::Daily) => "Daily".to_string(),
        Some(RepeatPolicy::Weekly) => "Weekly".to_string(),
        Some(RepeatPolicy::Monthly) => "Monthly".to_string(),
        None => "--".to_string(),
    };
    format!(
        "🗓️ Schedule summary (UTC)\n\nPrompt: \n{}{}\n\nStart: {}:{} UTC\nRepeat: {}",
        prompt, image_info, hour, minute, repeat
    )
}

fn nav_row(back_enabled: bool) -> Vec<InlineKeyboardButton> {
    let mut row: Vec<InlineKeyboardButton> = Vec::new();
    if back_enabled {
        row.push(InlineKeyboardButton::callback(
            "↩️ Back".to_string(),
            "sched_back".to_string(),
        ));
    }
    row.push(InlineKeyboardButton::callback(
        "❌ Cancel".to_string(),
        "sched_cancel".to_string(),
    ));
    row
}

pub fn build_nav_keyboard_prompt(back_enabled: bool) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![nav_row(back_enabled)])
}

pub fn build_image_keyboard_with_nav_prompt(back_enabled: bool) -> InlineKeyboardMarkup {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    rows.push(vec![InlineKeyboardButton::callback(
        "⏭️ Skip Image".to_string(),
        "sched_skip_image".to_string(),
    )]);
    rows.push(nav_row(back_enabled));
    InlineKeyboardMarkup::new(rows)
}

pub fn build_hours_keyboard_with_nav_prompt(back_enabled: bool) -> InlineKeyboardMarkup {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    let mut row: Vec<InlineKeyboardButton> = Vec::new();
    for h in 0..24u8 {
        row.push(InlineKeyboardButton::callback(format!("{:02}", h), format!("sched_hour:{}", h)));
        if row.len() == 6 {
            rows.push(row);
            row = Vec::new();
        }
    }
    if !row.is_empty() {
        rows.push(row);
    }
    rows.push(nav_row(back_enabled));
    InlineKeyboardMarkup::new(rows)
}

pub fn build_minutes_keyboard_with_nav_prompt(back_enabled: bool) -> InlineKeyboardMarkup {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    let mut row: Vec<InlineKeyboardButton> = Vec::new();
    for m in (0..=55).step_by(5) {
        let m_u8 = m as u8;
        row.push(InlineKeyboardButton::callback(
            format!("{:02}", m_u8),
            format!("sched_min:{}", m_u8),
        ));
        if row.len() == 6 {
            rows.push(row);
            row = Vec::new();
        }
    }
    if !row.is_empty() {
        rows.push(row);
    }
    rows.push(nav_row(back_enabled));
    InlineKeyboardMarkup::new(rows)
}

pub fn build_repeat_keyboard_with_nav_prompt(back_enabled: bool) -> InlineKeyboardMarkup {
    let mut rows = vec![
        vec![InlineKeyboardButton::callback(
            "No repeat".to_string(),
            "sched_repeat:none".to_string(),
        )],
        vec![
            InlineKeyboardButton::callback("Every 5 min".to_string(), "sched_repeat:5m".to_string()),
            InlineKeyboardButton::callback("15 min".to_string(), "sched_repeat:15m".to_string()),
            InlineKeyboardButton::callback("30 min".to_string(), "sched_repeat:30m".to_string()),
        ],
        vec![
            InlineKeyboardButton::callback("45 min".to_string(), "sched_repeat:45m".to_string()),
            InlineKeyboardButton::callback("1 hour".to_string(), "sched_repeat:1h".to_string()),
            InlineKeyboardButton::callback("3 hours".to_string(), "sched_repeat:3h".to_string()),
        ],
        vec![
            InlineKeyboardButton::callback("6 hours".to_string(), "sched_repeat:6h".to_string()),
            InlineKeyboardButton::callback("12 hours".to_string(), "sched_repeat:12h".to_string()),
            InlineKeyboardButton::callback("Daily".to_string(), "sched_repeat:1d".to_string()),
        ],
        vec![
            InlineKeyboardButton::callback("Weekly".to_string(), "sched_repeat:1w".to_string()),
            InlineKeyboardButton::callback("Monthly".to_string(), "sched_repeat:1mo".to_string()),
        ],
    ];
    rows.push(nav_row(back_enabled));
    InlineKeyboardMarkup::new(rows)
}

pub fn reset_from_step_prompts(state: &mut PendingWizardState, step: PendingStep) {
    match step {
        PendingStep::AwaitingPrompt => {
            state.prompt = None;
            state.image_url = None;
            state.hour_utc = None;
            state.minute_utc = None;
            state.repeat = None;
        }
        PendingStep::AwaitingImage => {
            state.image_url = None;
            state.hour_utc = None;
            state.minute_utc = None;
            state.repeat = None;
        }
        PendingStep::AwaitingHour => {
            state.hour_utc = None;
            state.minute_utc = None;
            state.repeat = None;
        }
        PendingStep::AwaitingMinute => {
            state.minute_utc = None;
            state.repeat = None;
        }
        PendingStep::AwaitingRepeat => {
            state.repeat = None;
        }
        PendingStep::AwaitingConfirm => { /* no-op */ }
    }
}


