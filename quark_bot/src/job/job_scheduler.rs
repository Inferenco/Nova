use crate::dependencies::BotDependencies;
use crate::job::handler::{
    job_active_daos,
    job_dao_results_cleanup,
    job_daos_results,
    job_token_ai_fees,
    job_token_list,
    job_welcome_service_cleanup,
};
use crate::scheduled_payments::runner as scheduled_payments_runner;
use crate::scheduled_prompts::runner as scheduled_prompts_runner;
use crate::welcome::welcome_service::WelcomeService;

use anyhow::Result;
use teloxide::Bot;
use tokio_cron_scheduler::JobScheduler;

pub async fn schedule_jobs(bot: Bot, bot_deps: &BotDependencies) -> Result<()> {
    log::info!("Initializing job scheduler...");

    let scheduler: JobScheduler = bot_deps.scheduler.clone();
    let panora = bot_deps.panora.clone();
    let dao = bot_deps.dao.clone();
    let welcome_service: WelcomeService = bot_deps.welcome_service.clone();

    let job_token_list = job_token_list(panora.clone());
    let job_token_ai_fees = job_token_ai_fees(panora.clone());
    let job_dao_results = job_daos_results(panora.clone(), bot.clone(), dao.clone());
    let job_active_daos = job_active_daos(dao.clone(), bot.clone());
    let job_dao_results_cleanup = job_dao_results_cleanup(dao.clone());
    let job_welcome_service_cleanup = job_welcome_service_cleanup(welcome_service.clone(), bot.clone());

    if let Err(e) = scheduler.add(job_token_list).await {
        log::error!("Failed to add token list job to scheduler: {}", e);
        return Err(anyhow::anyhow!("Failed to add token list job: {}", e));
    }

    if let Err(e) = scheduler.add(job_token_ai_fees).await {
        log::error!("Failed to add token AI fees job to scheduler: {}", e);
        return Err(anyhow::anyhow!("Failed to add token AI fees job: {}", e));
    }

    if let Err(e) = scheduler.add(job_dao_results).await {
        log::error!("Failed to add DAO results job to scheduler: {}", e);
        return Err(anyhow::anyhow!("Failed to add DAO results job: {}", e));
    }

    if let Err(e) = scheduler.add(job_active_daos).await {
        log::error!("Failed to add DAO active job to scheduler: {}", e);
        return Err(anyhow::anyhow!("Failed to add DAO active job: {}", e));
    }

    if let Err(e) = scheduler.add(job_dao_results_cleanup).await {
        log::error!("Failed to add DAO cleanup job to scheduler: {}", e);
        return Err(anyhow::anyhow!("Failed to add DAO cleanup job: {}", e));
    }

    if let Err(e) = scheduler.add(job_welcome_service_cleanup).await {
        log::error!("Failed to add welcome service cleanup job to scheduler: {}", e);
        return Err(anyhow::anyhow!("Failed to add welcome service cleanup job: {}", e));
    }

    scheduled_prompts_runner::register_all_schedules(bot.clone(), bot_deps.clone())
        .await
        .map_err(|e| {
            log::error!("Failed to bootstrap scheduled prompts: {}", e);
            anyhow::anyhow!("Failed to bootstrap scheduled prompts")
        })?;

    scheduled_payments_runner::register_all_schedules(bot.clone(), bot_deps.clone())
        .await
        .map_err(|e| {
            log::error!("Failed to bootstrap scheduled payments: {}", e);
            anyhow::anyhow!("Failed to bootstrap scheduled payments")
        })?;

    if let Err(e) = scheduler.start().await {
        log::error!("Failed to start job scheduler: {}", e);
        return Err(anyhow::anyhow!("Failed to start scheduler: {}", e));
    }

    log::info!("Job scheduler started successfully");
    log::info!("All jobs scheduled successfully");
    Ok(())
}
