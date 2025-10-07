use anyhow::Result as anyhowResult;

use crate::scheduled_prompts::{dto::ScheduledPromptRecord, storage::ScheduledStorage};

pub fn update_bincode_to_serder(scheduled_prompts: ScheduledStorage) -> anyhowResult<()> {
    log::info!("Migrating scheduled prompts to serde");
    let scheduled_prompts_tree = &scheduled_prompts.scheduled;

    for item in scheduled_prompts_tree.iter() {
        let (key, value) = item?;
        let scheduled_prompt_record = bincode::decode_from_slice::<ScheduledPromptRecord, _>(
            &value,
            bincode::config::standard(),
        );

        match scheduled_prompt_record {
            Err(_) => {
                let record = serde_json::from_slice::<ScheduledPromptRecord>(&value);
                if record.is_err() {
                    scheduled_prompts_tree.remove(&key)?;
                    log::warn!("Removed scheduled prompt for corrupted record");
                } else {
                    log::info!("Nothing to do for scheduled prompt: {}", record.unwrap().id);
                }
            }
            Ok((record, _)) => {
                scheduled_prompts.put_schedule(&record)?;
                log::info!("Migrated scheduled prompt: {}", record.id);
            }
        }
    }

    log::info!("Finished migrating scheduled prompts to serde");
    Ok(())
}
