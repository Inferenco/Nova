mod ai;
mod announcement;
mod aptos;
mod assets;
mod bot;
mod callbacks;
mod command_settings;
mod credentials;
mod dao;
mod db;
mod filters;
mod group;
mod job;
mod message_history;
mod migrations;
mod panora;
mod payment;
mod pending_transactions;
mod scheduled_payments;
mod scheduled_prompts;
mod services;
mod sponsor;
mod summarization_settings;
mod user_conversation;
mod user_model_preferences;
mod utils;
mod welcome;
mod yield_ai;

mod dependencies;

use crate::{
    ai::{
        gcs::GcsImageUploader, handler::AI, moderation::ModerationService,
        schedule_guard::schedule_guard_service::ScheduleGuardService,
        sentinel::sentinel::SentinelService, summarizer::handler::SummarizerService,
    },
    aptos::handler::Aptos,
    assets::{command_image_collector, media_aggregator},
    bot::handler_tree::handler_tree,
    command_settings::CommandSettingsManager,
    credentials::handler::Auth,
    dao::dao::Dao,
    dependencies::BotDependencies,
    filters::filters::Filters,
    group::{document_library::GroupDocuments, handler::Group},
    job::job_scheduler::schedule_jobs,
    message_history::handler::MessageHistory,
    panora::handler::Panora,
    payment::{dto::PaymentPrefs, payment::Payment},
    pending_transactions::handler::PendingTransactions,
    scheduled_payments::storage::ScheduledPaymentsStorage,
    scheduled_prompts::storage::ScheduledStorage,
    services::handler::Services,
    sponsor::sponsor::Sponsor,
    user_conversation::handler::UserConversations,
    user_model_preferences::handler::UserModelPreferences,
    yield_ai::yield_ai::YieldAI,
};
use inf_circle_sdk::circle_view::circle_view::CircleView;
use quark_core::helpers::{bot_commands::QuarkState, dto::CoinVersion};
use std::env;
use std::sync::Arc;
use teloxide::dispatching::dialogue::InMemStorage;
use teloxide::prelude::*;
use teloxide::types::BotCommand;
use tokio_cron_scheduler::JobScheduler;

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();
    log::info!("Starting quark_bot...");

    let bot = Bot::from_env();
    let db = db::init_tree();
    let auth_db = db.open_tree("auth").expect("Failed to open auth tree");
    let group_db = db.open_tree("group").expect("Failed to open group tree");

    let openai_api_key = env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not set");
    let gcs_creds = env::var("STORAGE_CREDENTIALS").expect("STORAGE_CREDENTIALS not set");
    let bucket_name = env::var("GCS_BUCKET_NAME").expect("GCS_BUCKET_NAME not set");
    let aptos_network = env::var("APTOS_NETWORK").expect("APTOS_NETWORK not set");
    let contract_address = env::var("CONTRACT_ADDRESS").expect("CONTRACT_ADDRESS not set");
    let aptos_api_key = env::var("APTOS_API_KEY").unwrap_or_default();
    let default_symbol = env::var("DEFAULT_SYMBOL").expect("DEFAULT_SYMBOL not set");

    let google_cloud = GcsImageUploader::new(&gcs_creds, bucket_name)
        .await
        .expect("Failed to create GCS image uploader");

    let aptos = Aptos::new(aptos_network, contract_address, aptos_api_key);
    let circle_view = CircleView::new()
        .map_err(|e| log::error!("Failed to create CircleView: {}", e))
        .unwrap();

    let min_deposit = env::var("MIN_DEPOSIT")
        .expect("MIN_DEPOSIT not set")
        .parse::<f64>()
        .expect("MIN_DEPOSIT must be a number");

    let panora = Panora::new(&db, aptos, min_deposit).expect("Failed to create Panora");

    // Create clone for dispatcher early to avoid move issues
    let panora_for_dispatcher = panora.clone();

    let auth = Auth::new(auth_db);
    let group = Group::new(group_db);
    let filters = Filters::new(&db);

    // Execute token list updates immediately on startup
    let panora_startup = panora.clone();
    log::info!("Executing initial token list update on startup...");
    match panora_startup.set_panora_token_list().await {
        Ok(_) => log::info!("Successfully updated Panora token list on startup"),
        Err(e) => log::error!("Failed to update Panora token list on startup: {}", e),
    }

    // Execute token AI fees update immediately on startup
    let panora_startup2 = panora.clone();
    let token_address = panora_startup2.aptos.get_token_address().await;
    match token_address {
        Ok(token_address) => match panora_startup2.set_token_ai_fees(&token_address).await {
            Ok(_) => log::info!("Successfully updated Panora token AI fees on startup"),
            Err(e) => log::error!("Failed to update Panora token AI fees on startup: {}", e),
        },
        Err(e) => log::error!("Failed to get token address: {}", e),
    }

    let dao_db = db.open_tree("dao").expect("Failed to open dao tree");
    let dao = Dao::new(dao_db);
    let scheduled_storage = ScheduledStorage::new(&db).expect("Failed to open scheduled storage");
    let scheduled_payments =
        ScheduledPaymentsStorage::new(&db).expect("Failed to open scheduled payments storage");

    let payment = Payment::new(&db).unwrap();

    let ai = AI::new(openai_api_key.clone(), google_cloud);
    let summarizer = SummarizerService::new(db.clone(), ai.get_client().clone());
    let schedule_guard = ScheduleGuardService::new(openai_api_key.clone())
        .expect("Failed to create ScheduleGuardService");
    let moderation = ModerationService::new(openai_api_key.clone(), db.clone())
        .expect("Failed to create ModerationService");
    let sentinel = SentinelService::new(db.clone());
    let sponsor = Sponsor::new(db.clone());

    let user_convos = UserConversations::new(&db).unwrap();
    let user_model_prefs = UserModelPreferences::new(&db).unwrap();
    let group_docs = GroupDocuments::new(&db).unwrap();
    let group_file_upload_state = assets::group_file_upload_state::GroupFileUploadState::new();
    let pending_transactions = PendingTransactions::new(&db).unwrap();
    let yield_ai = YieldAI::new();
    let welcome_service = welcome::welcome_service::WelcomeService::new(db.clone());
    let summarization_settings = summarization_settings::SummarizationSettings::new(&db)
        .expect("Failed to create SummarizationSettings");
    let command_settings = CommandSettingsManager::new(db.clone());

    let scheduler = JobScheduler::new()
        .await
        .expect("Failed to create job scheduler");

    let service = Services::new();

    let cmd_collector = Arc::new(command_image_collector::CommandImageCollector::new(
        bot.clone(),
    ));

    let media_aggregator = Arc::new(media_aggregator::MediaGroupAggregator::new(
        bot.clone(),
        ai.clone(),
        auth.clone(),
        user_model_prefs.clone(),
    ));

    let commands = vec![
        BotCommand::new("start", "Start interacting with the bot."),
        BotCommand::new("aptosconnect", "Open the Aptos Connect app."),
        BotCommand::new("help", "Display this text."),
        BotCommand::new("loginuser", "Log in as a user (DM only)."),
        BotCommand::new("logingroup", "Group login (under development)."),
        BotCommand::new("newchat", "Start a new conversation thread."),
        BotCommand::new("c", "prompt to chat AI with the bot."),
        BotCommand::new(
            "g",
            "prompt to chat AI with the bot in a group. (only admins can use this command)",
        ),
        BotCommand::new(
            "scheduleprompt",
            "Schedule a recurring or one-shot group prompt (admins only).",
        ),
        BotCommand::new(
            "listscheduled",
            "List active scheduled prompts (admins only).",
        ),
        BotCommand::new(
            "schedulepayment",
            "Schedule a token payment to a user (admins only).",
        ),
        BotCommand::new(
            "listscheduledpayments",
            "List scheduled token payments (admins only).",
        ),
        BotCommand::new("walletaddress", "Get your wallet address."),
        // Removed selectreasoningmodel (unified under selectmodel)
        // selectmodel and mysettings entries merged under /usersettings
        BotCommand::new("usersettings", "Open user settings menu (DM only)."),
        BotCommand::new(
            "report",
            "Moderate content (reply to message) and send a report to the admin if content is found to be inappropriate, muting the user in this case.",
        ),
        BotCommand::new("rules", "Show core and custom rules for this group."),
        BotCommand::new("balance", "Get your balance of a token."),
        BotCommand::new("groupwalletaddress", "Get the group's wallet address."),
        BotCommand::new("groupbalance", "Get the group's balance of a token."),
        BotCommand::new("prices", "Display model pricing information."),
        BotCommand::new("tutorial", "Watch the quick start tutorial video."),
        BotCommand::new(
            "globalannouncement",
            "Send a global announcement (authorized only).",
        ),
        BotCommand::new("groupsettings", "Open group settings menu (admins only)."),
    ];

    let history_storage = InMemStorage::<MessageHistory>::new();

    bot.set_my_commands(commands).await.unwrap();

    let default_currency = panora
        .aptos
        .get_token_address()
        .await
        .expect("Failed to get token address");
    let default_version = CoinVersion::V1;

    let default_payment_prefs =
        PaymentPrefs::from((default_symbol, default_currency, default_version));

    migrations::schedule_prompts::update_bincode_to_serder(scheduled_storage.clone())
        .expect("Failed to update scheduled prompts");

    migrations::schedule_payments::update_bincode_to_serder(scheduled_payments.clone())
        .expect("Failed to update scheduled payments");

    let bot_deps = BotDependencies {
        db,
        auth,
        service,
        user_convos,
        user_model_prefs,
        ai,
        cmd_collector,
        panora: panora_for_dispatcher,
        group,
        group_docs,
        group_file_upload_state,
        dao,
        filters,
        command_settings,
        scheduled_storage,
        scheduled_payments,
        media_aggregator,
        history_storage,
        pending_transactions,
        yield_ai,
        scheduler,
        payment,
        default_payment_prefs,
        schedule_guard,
        moderation,
        sentinel,
        sponsor,
        summarization_settings,
        welcome_service,
        summarizer,
        circle_view,
    };

    // Bootstrap user-defined schedules (load and register)
    schedule_jobs(bot.clone(), &bot_deps)
        .await
        .expect("Failed to schedule jobs");

    Dispatcher::builder(bot.clone(), handler_tree())
        .dependencies(dptree::deps![InMemStorage::<QuarkState>::new(), bot_deps])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}
