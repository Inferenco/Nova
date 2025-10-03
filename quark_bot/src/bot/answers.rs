use anyhow::Result;
use quark_core::helpers::bot_commands::Command;
use teloxide::{
    Bot,
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, Message},
};

use super::handler::{
    handle_chat, handle_help, handle_login_group, handle_login_user, handle_mod, handle_new_chat,
    handle_prices, handle_rules, handle_start,
};
use crate::utils::{self, KeyboardMarkupType, send_markdown_message_with_keyboard};
use crate::yield_ai::handler as yield_ai_handler;
use crate::{announcement::handle_announcement, utils::send_message};

use crate::bot::handler::{
    handle_aptos_connect, handle_balance, handle_group_balance, handle_group_wallet_address,
    handle_wallet_address,
};
use crate::dependencies::BotDependencies;
use crate::scheduled_payments::handler::{
    handle_listscheduledpayments_command, handle_schedulepayment_command,
};
use crate::scheduled_prompts::handler::{
    handle_listscheduled_command, handle_scheduleprompt_command,
};

pub async fn answers(
    bot: Bot,
    msg: Message,
    cmd: Command,
    bot_deps: BotDependencies,
) -> Result<()> {
    match cmd {
        Command::Start => handle_start(bot, msg).await?,
        Command::AptosConnect => handle_aptos_connect(bot, msg).await?,
        Command::Help => handle_help(bot, msg).await?,
        Command::WalletAddress => handle_wallet_address(bot, msg, bot_deps.clone()).await?,
        Command::Balance(symbol) => {
            if symbol.trim().is_empty() {
                yield_ai_handler::handle_balance(bot, msg, bot_deps.clone(), false).await?
            } else {
                handle_balance(bot, msg, &symbol, bot_deps.clone()).await?
            }
        }
        Command::Prices => handle_prices(bot, msg).await?,
        Command::Tutorial => {
            send_message(
                msg,
                bot,
                "https://youtu.be/QAXWWXpzH-Q?si=Gslz7yS9l6BqC02E".to_string(),
            )
            .await?;
        }
        Command::LoginUser => handle_login_user(bot, msg).await?,
        Command::LoginGroup => handle_login_group(bot, msg, bot_deps.clone()).await?,
        Command::NewChat => handle_new_chat(bot, msg, bot_deps.clone()).await?,
        Command::C(prompt) => {
            // Check if chat commands are enabled for this group (skip check for private chats)
            if !msg.chat.is_private() {
                let group_id = msg.chat.id.to_string();
                if !bot_deps.command_settings.is_chat_commands_enabled(group_id) {
                    send_message(
                        msg,
                        bot,
                        "❌ Chat commands (/c, /chat) are disabled in this group. Contact an administrator to enable them.".to_string(),
                    )
                    .await?;
                    return Ok(());
                }
            }

            let cmd_collector = bot_deps.cmd_collector.clone();

            if prompt.trim().is_empty() && msg.photo().is_some() {
                cmd_collector.add_command(msg, bot_deps.clone(), None).await;
            } else if prompt.trim().is_empty() {
                send_message(
                    msg,
                    bot,
                    "Please include a message after /c, e.g. /c What is the weather today?"
                        .to_string(),
                )
                .await?;
            } else {
                handle_chat(bot, msg, prompt, None, false, bot_deps).await?;
            }
        }
        Command::G(prompt) => {
            let cmd_collector = bot_deps.cmd_collector.clone();

            let users_admin = bot.get_chat_administrators(msg.chat.id).await?;

            let group_id = msg.clone().chat.id.to_string();

            let multimedia_message = msg.clone();

            if prompt.trim().is_empty() && multimedia_message.photo().is_none() {
                send_message(
                    msg,
                    bot,
                    "Please include a message after /g, e.g. /g What is the weather today?"
                        .to_string(),
                )
                .await?;
                return Ok(());
            }

            if !msg.chat.is_group() && !msg.chat.is_supergroup() {
                send_message(
                    msg,
                    bot,
                    "This command can only be used in a group.".to_string(),
                )
                .await?;
                return Ok(());
            }

            let user = msg.from.clone();

            if user.is_none() {
                send_message(msg, bot, "❌ User not found".to_string()).await?;
                return Ok(());
            }

            let user = user.unwrap();

            let is_admin = users_admin.iter().any(|member| member.user.id == user.id);

            let sponsor = bot_deps
                .sponsor
                .can_make_request(msg.chat.id.to_string(), user.id.to_string());

            let is_sponsor = sponsor.is_ok() && sponsor.unwrap();

            if !is_admin && !is_sponsor {
                send_message(msg, bot, "Only group admins can use this command or requests allowed to members reached the limit.".to_string())
                    .await?;
                return Ok(());
            }

            if prompt.trim().is_empty() && multimedia_message.photo().is_some() {
                cmd_collector
                    .add_command(multimedia_message, bot_deps.clone(), Some(group_id))
                    .await;
            } else {
                handle_chat(
                    bot,
                    multimedia_message,
                    prompt,
                    Some(group_id),
                    is_sponsor,
                    bot_deps.clone(),
                )
                .await?;
            }
        }

        Command::Report => {
            handle_mod(bot, msg, bot_deps.clone()).await?;
        }
        Command::Rules => {
            handle_rules(bot, msg, bot_deps.clone()).await?;
        }

        Command::PromptExamples => {
            send_message(msg, bot, "Here are some example prompts you can use:\n\n💰 Wallet & Balance:\n- /prompt \"What's my wallet address?\" or /p \"What's my wallet address?\"\n- /prompt \"Show my balance\" or /p \"Show my balance\"\n- /prompt \"Check my SUI balance\" or /p \"Check my SUI balance\"\n- /prompt \"How much do I have?\" or /p \"How much do I have?\"\n\n💸 Transactions:\n- /prompt \"Send 10 SUI to @username\" or /p \"Send 10 SUI to @username\"\n- /prompt \"Withdraw 5 SUI\" or /p \"Withdraw 5 SUI\"\n- /prompt \"Send 100 SUI to everyone\" or /p \"Send 100 SUI to everyone\"\n\n❓ General:\n- /prompt \"What can you help me with?\" or /p \"What can you help me with?\"\n- /prompt \"Explain how this bot works\" or /p \"Explain how this bot works\"\n\n💡 Tip: Use /p as a shortcut for /prompt!".to_string()).await?;
            ()
        }
        // SelectModel and MySettings are now accessible from /usersettings menu
        Command::Usersettings => {
            // Keep menu assembly in bot layer per request; present model prefs, my settings, and payment submenu
            use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};
            if !msg.chat.is_private() {
                send_message(
                    msg,
                    bot,
                    "❌ This command can only be used in a private chat.".to_string(),
                )
                .await?;
            } else {
                let kb = InlineKeyboardMarkup::new(vec![
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
                send_markdown_message_with_keyboard(
                    bot,
                    msg,
                    KeyboardMarkupType::InlineKeyboardType(kb),
                    "⚙️ <b>User Settings</b>\n\n• Manage your model, view current settings, and configure payment.\n\n💡 If no payment token is selected, the on-chain default will be used.",
                )
                .await?;
            }
        }
        Command::GroupWalletAddress => {
            handle_group_wallet_address(bot, msg, bot_deps.clone()).await?;
        }
        Command::GroupBalance(symbol) => {
            if symbol.trim().is_empty() {
                yield_ai_handler::handle_balance(bot, msg, bot_deps.clone(), true).await?
            } else {
                handle_group_balance(bot, msg, bot_deps.clone(), &symbol).await?
            }
        }
        Command::Announcement(text) => {
            handle_announcement(bot, msg, text, bot_deps.clone()).await?;
        }
        Command::Groupsettings => {
            if msg.chat.is_private() {
                send_message(
                    msg,
                    bot,
                    "❌ This command must be used in a group chat.".to_string(),
                )
                .await?;
            } else {
                let uid = msg.from.as_ref().map(|u| u.id);

                if uid.is_none() {
                    send_message(msg, bot, "❌ User not found".to_string()).await?;
                    return Ok(());
                }

                let uid = uid.unwrap();

                let is_admin = utils::is_admin(&bot, msg.chat.id, uid).await;
                if !is_admin {
                    send_message(
                        msg,
                        bot,
                        "❌ Only group administrators can open group settings.".to_string(),
                    )
                    .await?;
                } else {
                    let kb = InlineKeyboardMarkup::new(vec![
                        vec![InlineKeyboardButton::callback(
                            "💳 Payment Settings",
                            "open_group_payment_settings",
                        )],
                        vec![InlineKeyboardButton::callback(
                            "🏛️ DAO Preferences",
                            "open_dao_preferences",
                        )],
                        vec![InlineKeyboardButton::callback(
                            "🛡️ Moderation",
                            "open_moderation_settings",
                        )],
                        vec![InlineKeyboardButton::callback(
                            "🎯 Sponsor Settings",
                            "open_sponsor_settings",
                        )],
                        vec![InlineKeyboardButton::callback(
                            "👋 Welcome Settings",
                            "welcome_settings",
                        )],
                        vec![InlineKeyboardButton::callback("🔍 Filters", "filters_main")],
                        vec![InlineKeyboardButton::callback(
                            "📁 Group Document Library",
                            "open_group_document_library",
                        )],
                        vec![InlineKeyboardButton::callback(
                            "⚙️ Command Settings",
                            "open_command_settings",
                        )],
                        vec![InlineKeyboardButton::callback(
                            "📋 Summarization Settings",
                            "open_group_summarization_settings",
                        )],
                        vec![InlineKeyboardButton::callback(
                            "🔄 Migrate Group ID",
                            "open_migrate_group_id",
                        )],
                        vec![InlineKeyboardButton::callback(
                            "↩️ Close",
                            "group_settings_close",
                        )],
                    ]);
                    send_markdown_message_with_keyboard(
                        bot,
                        msg,
                        KeyboardMarkupType::InlineKeyboardType(kb),
                        "⚙️ <b>Group Settings</b>\n\n• Configure payment token, DAO preferences, moderation, sponsor settings, welcome settings, filters, and group migration.\n\n💡 Only group administrators can access these settings.",
                    )
                    .await?;
                }
            }
        }
        Command::SchedulePrompt => {
            handle_scheduleprompt_command(bot, msg, bot_deps.clone()).await?;
        }
        Command::ListScheduled => {
            handle_listscheduled_command(bot, msg, bot_deps.clone()).await?;
        }
        Command::SchedulePayment => {
            handle_schedulepayment_command(bot, msg, bot_deps.clone()).await?;
        }
        Command::ListScheduledPayments => {
            handle_listscheduledpayments_command(bot, msg, bot_deps.clone()).await?;
        }
    };
    Ok(())
}
