use std::sync::Arc;

use crate::{
    ai::{
        handler::AI, moderation::ModerationService,
        schedule_guard::schedule_guard_service::ScheduleGuardService,
        sentinel::sentinel::SentinelService, summarizer::handler::SummarizerService,
    },
    assets::{
        group_file_upload_state::GroupFileUploadState, media_aggregator::MediaGroupAggregator,
    },
    command_settings::CommandSettingsManager,
    credentials::handler::Auth,
    dao::dao::Dao,
    filters::filters::Filters,
    group::{document_library::GroupDocuments, handler::Group},
    message_history::handler::HistoryStorage,
    panora::handler::Panora,
    payment::dto::PaymentPrefs,
    payment::payment::Payment,
    pending_transactions::handler::PendingTransactions,
    scheduled_payments::storage::ScheduledPaymentsStorage,
    scheduled_prompts::storage::ScheduledStorage,
    services::handler::Services,
    sponsor::sponsor::Sponsor,
    summarization_settings::SummarizationSettings,
    user_conversation::handler::UserConversations,
    welcome::welcome_service::WelcomeService,
    yield_ai::yield_ai::YieldAI,
};
use inf_circle_sdk::circle_view::circle_view::CircleView;
use tokio_cron_scheduler::JobScheduler;

#[derive(Clone)]
pub struct BotDependencies {
    pub db: sled::Db,
    pub auth: Auth,
    pub service: Services,
    pub user_convos: UserConversations,
    pub user_model_prefs: crate::user_model_preferences::handler::UserModelPreferences,
    pub ai: AI,
    pub cmd_collector:
        std::sync::Arc<crate::assets::command_image_collector::CommandImageCollector>,
    pub panora: Panora,
    pub group: Group,
    #[allow(dead_code)]
    pub group_docs: GroupDocuments,
    pub group_file_upload_state: GroupFileUploadState,
    pub dao: Dao,
    pub filters: Filters,
    pub command_settings: CommandSettingsManager,
    pub scheduled_storage: ScheduledStorage,
    pub scheduled_payments: ScheduledPaymentsStorage,
    pub media_aggregator: Arc<MediaGroupAggregator>,
    pub history_storage: HistoryStorage,
    pub pending_transactions: PendingTransactions,
    pub yield_ai: YieldAI,
    pub scheduler: JobScheduler,
    pub payment: Payment,
    pub default_payment_prefs: PaymentPrefs,
    pub schedule_guard: ScheduleGuardService,
    pub moderation: ModerationService,
    pub sentinel: SentinelService,
    pub sponsor: Sponsor,
    pub summarization_settings: SummarizationSettings,
    pub welcome_service: WelcomeService,
    pub summarizer: SummarizerService,
    pub circle_view: CircleView,
}
