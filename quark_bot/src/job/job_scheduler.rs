use crate::dao::dao::Dao;
use crate::job::handler::{
    job_active_daos, job_dao_results_cleanup, job_daos_results, job_token_ai_fees, job_token_list, job_welcome_service_cleanup,
};
use crate::panora::handler::Panora;

use anyhow::Result;
use teloxide::Bot;
use tokio_cron_scheduler::JobScheduler;

pub async fn schedule_jobs(panora: Panora, bot: Bot, dao: Dao, welcome_service: crate::welcome::welcome_service::WelcomeService) -> Result<JobScheduler> {
    log::info!("Initializing job scheduler...");

    let scheduler = match JobScheduler::new().await {
        Ok(scheduler) => scheduler,
        Err(e) => {
            log::error!("Failed to create job scheduler: {}", e);
            return Err(anyhow::anyhow!("Failed to create job scheduler: {}", e));
        }
    };

    // Create all jobs
    let job_token_list = job_token_list(panora.clone());
    let job_token_ai_fees = job_token_ai_fees(panora.clone());
    let job_dao_results = job_daos_results(panora.clone(), bot.clone(), dao.clone());
    let job_active_daos = job_active_daos(dao.clone(), bot.clone());
    let job_dao_results_cleanup = job_dao_results_cleanup(dao.clone());
    let job_welcome_service_cleanup = job_welcome_service_cleanup(welcome_service.clone(), bot.clone());

    // Add jobs to scheduler with error handling
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

    // Start the scheduler
    if let Err(e) = scheduler.start().await {
        log::error!("Failed to start job scheduler: {}", e);
        return Err(anyhow::anyhow!("Failed to start scheduler: {}", e));
    }

    log::info!("Job scheduler started successfully");

    // Add cleanup jobs after scheduler is started
    if let Err(e) = scheduler.add(job_dao_results_cleanup).await {
        log::error!("Failed to add DAO cleanup job to scheduler: {}", e);
        return Err(anyhow::anyhow!("Failed to add DAO cleanup job: {}", e));
    }

    if let Err(e) = scheduler.add(job_welcome_service_cleanup).await {
        log::error!("Failed to add welcome service cleanup job to scheduler: {}", e);
        return Err(anyhow::anyhow!("Failed to add welcome service cleanup job: {}", e));
    }

    log::info!("All jobs scheduled successfully");
    Ok(scheduler)
}
