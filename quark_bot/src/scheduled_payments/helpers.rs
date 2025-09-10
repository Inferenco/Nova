use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

use crate::scheduled_payments::dto::{PendingPaymentStep, PendingPaymentWizardState};
use crate::scheduled_prompts::dto::RepeatPolicy;

pub fn build_repeat_keyboard_payments() -> InlineKeyboardMarkup {
    let rows = vec![
        vec![InlineKeyboardButton::callback(
            "Daily".to_string(),
            "schedpay_repeat:1d".to_string(),
        )],
        vec![InlineKeyboardButton::callback(
            "Weekly".to_string(),
            "schedpay_repeat:1w".to_string(),
        )],
        vec![
            InlineKeyboardButton::callback(
                "2-Weekly".to_string(),
                "schedpay_repeat:2w".to_string(),
            ),
            InlineKeyboardButton::callback(
                "4-Weekly".to_string(),
                "schedpay_repeat:4w".to_string(),
            ),
        ],
    ];
    InlineKeyboardMarkup::new(rows)
}

fn nav_row(back_enabled: bool) -> Vec<InlineKeyboardButton> {
    let mut row: Vec<InlineKeyboardButton> = Vec::new();
    if back_enabled {
        row.push(InlineKeyboardButton::callback(
            "↩️ Back".to_string(),
            "schedpay_back".to_string(),
        ));
    }
    row.push(InlineKeyboardButton::callback(
        "❌ Cancel".to_string(),
        "schedpay_cancel".to_string(),
    ));
    row
}

pub fn build_nav_keyboard_payments(back_enabled: bool) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![nav_row(back_enabled)])
}

pub fn build_hours_keyboard_with_nav_payments(back_enabled: bool) -> InlineKeyboardMarkup {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    let mut row: Vec<InlineKeyboardButton> = Vec::new();
    for h in 0..24u8 {
        row.push(InlineKeyboardButton::callback(
            format!("{:02}", h),
            format!("schedpay_hour:{}", h),
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

pub fn build_minutes_keyboard_with_nav_payments(back_enabled: bool) -> InlineKeyboardMarkup {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    let mut row: Vec<InlineKeyboardButton> = Vec::new();
    for m in (0..=55).step_by(5) {
        let mu = m as u8;
        row.push(InlineKeyboardButton::callback(
            format!("{:02}", mu),
            format!("schedpay_min:{}", mu),
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

pub fn build_repeat_keyboard_with_nav_payments(back_enabled: bool) -> InlineKeyboardMarkup {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    rows.push(vec![InlineKeyboardButton::callback(
        "Daily".to_string(),
        "schedpay_repeat:1d".to_string(),
    )]);
    rows.push(vec![InlineKeyboardButton::callback(
        "Weekly".to_string(),
        "schedpay_repeat:1w".to_string(),
    )]);
    rows.push(vec![
        InlineKeyboardButton::callback("2-Weekly".to_string(), "schedpay_repeat:2w".to_string()),
        InlineKeyboardButton::callback("4-Weekly".to_string(), "schedpay_repeat:4w".to_string()),
    ]);
    rows.push(nav_row(back_enabled));
    InlineKeyboardMarkup::new(rows)
}

pub fn summarize(state: &PendingPaymentWizardState) -> String {
    let recipient = state
        .recipient_username
        .as_deref()
        .map(|u| format!("@{}", u))
        .unwrap_or("(recipient not set)".to_string());
    let symbol = state.symbol.as_deref().unwrap_or("(symbol not set)");
    let amount = state
        .amount_display
        .map(|v| format!("{:.4}", v))
        .unwrap_or("(amount not set)".to_string());
    let date = state.date.clone().unwrap_or("(date not set)".to_string());
    let hour = state
        .hour_utc
        .map(|h| format!("{:02}", h))
        .unwrap_or("--".into());
    let minute = state
        .minute_utc
        .map(|m| format!("{:02}", m))
        .unwrap_or("--".into());
    let repeat = match (state.repeat.clone(), state.weekly_weeks) {
        (Some(RepeatPolicy::Daily), _) => "Daily".to_string(),
        (Some(RepeatPolicy::Weekly), Some(1)) => "Weekly / 1w".to_string(),
        (Some(RepeatPolicy::Weekly), Some(2)) => "2-Weekly / 2w".to_string(),
        (Some(RepeatPolicy::Weekly), Some(4)) => "4-Weekly / 4w".to_string(),
        (Some(RepeatPolicy::Weekly), Some(w)) => format!("Every {}w", w),
        (Some(RepeatPolicy::Weekly), None) => "Weekly".to_string(),
        (Some(_), _) => "(unsupported)".to_string(),
        (None, _) => "(not set)".to_string(),
    };
    format!(
        "💸 Payment schedule (UTC)\nRecipient: {}\nAmount: {} {}\nFirst run: {} {}:{}\nRepeat: {}",
        recipient, amount, symbol, date, hour, minute, repeat
    )
}

pub fn reset_from_step_payments(state: &mut PendingPaymentWizardState, step: PendingPaymentStep) {
    match step {
        PendingPaymentStep::AwaitingRecipient => {
            state.recipient_username = None;
            state.recipient_address = None;
            state.symbol = None;
            state.token_type = None;
            state.decimals = None;
            state.amount_display = None;
            state.date = None;
            state.hour_utc = None;
            state.minute_utc = None;
            state.repeat = None;
            state.weekly_weeks = None;
        }
        PendingPaymentStep::AwaitingToken => {
            state.symbol = None;
            state.token_type = None;
            state.decimals = None;
            state.amount_display = None;
            state.date = None;
            state.hour_utc = None;
            state.minute_utc = None;
            state.repeat = None;
            state.weekly_weeks = None;
        }
        PendingPaymentStep::AwaitingAmount => {
            // keep token data
            state.amount_display = None;
            state.date = None;
            state.hour_utc = None;
            state.minute_utc = None;
            state.repeat = None;
            state.weekly_weeks = None;
        }
        PendingPaymentStep::AwaitingDate => {
            state.date = None;
            state.hour_utc = None;
            state.minute_utc = None;
            state.repeat = None;
            state.weekly_weeks = None;
        }
        PendingPaymentStep::AwaitingHour => {
            state.hour_utc = None;
            state.minute_utc = None;
            state.repeat = None;
            state.weekly_weeks = None;
        }
        PendingPaymentStep::AwaitingMinute => {
            state.minute_utc = None;
            state.repeat = None;
            state.weekly_weeks = None;
        }
        PendingPaymentStep::AwaitingRepeat => {
            state.repeat = None;
            state.weekly_weeks = None;
        }
        PendingPaymentStep::AwaitingConfirm => {
            // no-op when stepping back to confirm (not used)
        }
    }
}
