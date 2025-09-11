use super::dto::{ChatModel, VerbosityLevel};
use super::handler::UserModelPreferences;
use anyhow::Result;

use teloxide::prelude::*;
use teloxide::types::{CallbackQuery, InlineKeyboardButton, InlineKeyboardMarkup, ParseMode};

pub async fn handle_model_preferences_callback(
    bot: Bot,
    query: CallbackQuery,
    user_model_prefs: UserModelPreferences,
) -> Result<()> {
    let data = query.data.as_ref().unwrap();
    let user = &query.from;

    let username = user.username.as_ref();
    if username.is_none() {
        bot.answer_callback_query(query.id)
            .text("❌ Username not found, required for this feature")
            .await?;
        return Ok(());
    }
    let username = username.unwrap();

    if data.starts_with("select_chat_model:") {
        let model_str = data.strip_prefix("select_chat_model:").unwrap();
        let model = match model_str {
            "GPT5" => ChatModel::GPT5,
            "GPT5Mini" => ChatModel::GPT5Mini,
            _ => {
                bot.answer_callback_query(query.id)
                    .text("❌ Invalid model selection")
                    .await?;
                return Ok(());
            }
        };

        // Save model and ask for reasoning preference
        let mut prefs = user_model_prefs.get_preferences(username);
        prefs.chat_model = model.clone();
        user_model_prefs.set_preferences(username, &prefs)?;

        // Ask for reasoning preference with single toggle button
        let current_reasoning = prefs.reasoning_enabled;
        let button_text = if current_reasoning {
            "⚡ Reasoning Off"
        } else {
            "🧠 Reasoning On"
        };
        let button_data = if current_reasoning {
            "set_reasoning:false"
        } else {
            "set_reasoning:true"
        };
        
        let keyboard = InlineKeyboardMarkup::new(vec![
            vec![InlineKeyboardButton::callback(button_text, button_data)],
            vec![InlineKeyboardButton::callback(
                "➡️ Continue to Verbosity",
                "continue_to_verbosity",
            )],
            vec![InlineKeyboardButton::callback(
                "↩️ Back to Model Selection",
                "back_to_model_selection",
            )],
        ]);

        if let Some(message) = query.message {
            if let teloxide::types::MaybeInaccessibleMessage::Regular(msg) = message {
                let reasoning_status = if current_reasoning {
                    "<b>🟢 ON</b>"
                } else {
                    "<b>🔴 OFF</b>"
                };
                let text = crate::utils::sanitize_telegram_html(&format!(
                    "✅ <b>Model selected:</b> {}\n\n🧠 <b>Reasoning Setting:</b> {}\nChoose whether to enable reasoning for more detailed responses.",
                    model.to_display_string(),
                    reasoning_status
                ));
                bot.edit_message_text(msg.chat.id, msg.id, text)
                .reply_markup(keyboard)
                .parse_mode(ParseMode::Html)
                .await?;
            }
        }

        bot.answer_callback_query(query.id)
            .text(format!("Model: {}", model.to_display_string()))
            .await?;
    } else if data.starts_with("set_reasoning:") {
        let reasoning_str = data.strip_prefix("set_reasoning:").unwrap();
        let reasoning_enabled = reasoning_str == "true";

        let mut prefs = user_model_prefs.get_preferences(username);
        prefs.reasoning_enabled = reasoning_enabled;
        user_model_prefs.set_preferences(username, &prefs)?;

        // Show updated reasoning setting with toggle button
        let button_text = if reasoning_enabled {
            "⚡ Reasoning Off"
        } else {
            "🧠 Reasoning On"
        };
        let button_data = if reasoning_enabled {
            "set_reasoning:false"
        } else {
            "set_reasoning:true"
        };
        
        let keyboard = InlineKeyboardMarkup::new(vec![
            vec![InlineKeyboardButton::callback(button_text, button_data)],
            vec![InlineKeyboardButton::callback(
                "➡️ Continue to Verbosity",
                "continue_to_verbosity",
            )],
            vec![InlineKeyboardButton::callback(
                "↩️ Back to Model Selection",
                "back_to_model_selection",
            )],
        ]);

        if let Some(message) = query.message {
            if let teloxide::types::MaybeInaccessibleMessage::Regular(msg) = message {
                let reasoning_status = if reasoning_enabled {
                    "<b>🟢 ON</b>"
                } else {
                    "<b>🔴 OFF</b>"
                };
                let text = crate::utils::sanitize_telegram_html(&format!(
                    "✅ <b>Model selected:</b> {}\n\n🧠 <b>Reasoning Setting:</b> {}\nChoose whether to enable reasoning for more detailed responses.",
                    prefs.chat_model.to_display_string(),
                    reasoning_status
                ));
                bot.edit_message_text(msg.chat.id, msg.id, text)
                .reply_markup(keyboard)
                .parse_mode(ParseMode::Html)
                .await?;
            }
        }

        bot.answer_callback_query(query.id)
            .text(format!("Reasoning: {}", if reasoning_enabled { "On" } else { "Off" }))
            .await?;
    } else if data == "continue_to_verbosity" {
        // Show verbosity selection
        let prefs = user_model_prefs.get_preferences(username);
        let keyboard = InlineKeyboardMarkup::new(vec![
            vec![InlineKeyboardButton::callback(
                "📝 Normal",
                "set_verbosity:Normal",
            )],
            vec![InlineKeyboardButton::callback(
                "💬 Chatty",
                "set_verbosity:Chatty",
            )],
            vec![InlineKeyboardButton::callback(
                "↩️ Back to Reasoning",
                "back_to_reasoning",
            )],
        ]);

        if let Some(message) = query.message {
            if let teloxide::types::MaybeInaccessibleMessage::Regular(msg) = message {
                let text = crate::utils::sanitize_telegram_html(&format!(
                    "✅ <b>Model selected:</b> {}\n✅ <b>Reasoning:</b> {}\n\n🗣️ <b>Verbosity Setting:</b>\nChoose the response verbosity level.",
                    prefs.chat_model.to_display_string(),
                    if prefs.reasoning_enabled { "On" } else { "Off" }
                ));
                bot.edit_message_text(msg.chat.id, msg.id, text)
                .reply_markup(keyboard)
                .parse_mode(ParseMode::Html)
                .await?;
            }
        }

        bot.answer_callback_query(query.id)
            .text("Continue to verbosity")
            .await?;
    } else if data == "back_to_model_selection" {
        // Return to model selection
        let keyboard = InlineKeyboardMarkup::new(vec![
            vec![InlineKeyboardButton::callback(
                "GPT-5 (💸 Smart & Creative)",
                "select_chat_model:GPT5",
            )],
            vec![InlineKeyboardButton::callback(
                "GPT-5-Mini (💵 Cheapest & Fastest)",
                "select_chat_model:GPT5Mini",
            )],
            vec![InlineKeyboardButton::callback(
                "↩️ Back to Settings",
                "back_to_user_settings",
            )],
        ]);

        if let Some(message) = query.message {
            if let teloxide::types::MaybeInaccessibleMessage::Regular(msg) = message {
                let text = crate::utils::sanitize_telegram_html(
                    "🤖 <b>Select your chat model:</b>\n\nChoose which model to use for regular chat commands (/c):",
                );
                bot.edit_message_text(msg.chat.id, msg.id, text)
                .reply_markup(keyboard)
                .parse_mode(ParseMode::Html)
                .await?;
            }
        }

        bot.answer_callback_query(query.id)
            .text("Back to model selection")
            .await?;
    } else if data == "back_to_reasoning" {
        // Return to reasoning settings
        let prefs = user_model_prefs.get_preferences(username);
        let button_text = if prefs.reasoning_enabled {
            "⚡ Reasoning Off"
        } else {
            "🧠 Reasoning On"
        };
        let button_data = if prefs.reasoning_enabled {
            "set_reasoning:false"
        } else {
            "set_reasoning:true"
        };
        
        let keyboard = InlineKeyboardMarkup::new(vec![
            vec![InlineKeyboardButton::callback(button_text, button_data)],
            vec![InlineKeyboardButton::callback(
                "➡️ Continue to Verbosity",
                "continue_to_verbosity",
            )],
            vec![InlineKeyboardButton::callback(
                "↩️ Back to Model Selection",
                "back_to_model_selection",
            )],
        ]);

        if let Some(message) = query.message {
            if let teloxide::types::MaybeInaccessibleMessage::Regular(msg) = message {
                let reasoning_status = if prefs.reasoning_enabled {
                    "<b>🟢 ON</b>"
                } else {
                    "<b>🔴 OFF</b>"
                };
                let text = crate::utils::sanitize_telegram_html(&format!(
                    "✅ <b>Model selected:</b> {}\n\n🧠 <b>Reasoning Setting:</b> {}\nChoose whether to enable reasoning for more detailed responses.",
                    prefs.chat_model.to_display_string(),
                    reasoning_status
                ));
                bot.edit_message_text(msg.chat.id, msg.id, text)
                .reply_markup(keyboard)
                .parse_mode(ParseMode::Html)
                .await?;
            }
        }

        bot.answer_callback_query(query.id)
            .text("Back to reasoning settings")
            .await?;
    } else if data.starts_with("set_verbosity:") {
        let verbosity_str = data.strip_prefix("set_verbosity:").unwrap();
        let verbosity = match verbosity_str {
            "Normal" => VerbosityLevel::Normal,
            "Chatty" => VerbosityLevel::Chatty,
            _ => VerbosityLevel::Normal,
        };

        let mut prefs = user_model_prefs.get_preferences(username);
        prefs.verbosity = verbosity.clone();
        user_model_prefs.set_preferences(username, &prefs)?;

        if let Some(message) = query.message {
            if let teloxide::types::MaybeInaccessibleMessage::Regular(msg) = message {
                // Show popup notification
                bot.answer_callback_query(query.id.clone())
                    .text("Preferences saved!")
                    .await?;

                // Return to user settings menu
                let keyboard = InlineKeyboardMarkup::new(vec![
                    vec![InlineKeyboardButton::callback(
                        "🧠 Select Model",
                        "open_select_model",
                    )],
                    vec![InlineKeyboardButton::callback(
                        "💳 Payment Settings",
                        "open_payment_settings",
                    )],
                    vec![InlineKeyboardButton::callback(
                        "📁 Document Library",
                        "open_document_library",
                    )],
                    vec![InlineKeyboardButton::callback(
                        "📋 View My Settings",
                        "open_my_settings",
                    )],
                    vec![InlineKeyboardButton::callback(
                        "🧾 Summarization Settings",
                        "open_summarization_settings",
                    )],
                    vec![InlineKeyboardButton::callback(
                        "↩️ Close",
                        "user_settings_close",
                    )],
                ]);

                let text = crate::utils::sanitize_telegram_html(
                    "⚙️ <b>User Settings</b>\n\n• Manage your model, view current settings, and configure payment.\n\n💡 If no payment token is selected, the on-chain default will be used.",
                );
                bot.edit_message_text(msg.chat.id, msg.id, text)
                .parse_mode(ParseMode::Html)
                .reply_markup(keyboard)
                .await?;
            }
        }
    }

    Ok(())
}
