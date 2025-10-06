use crate::scheduled_prompts::dto::{
    LegacyPendingWizardState, LegacyScheduledPromptRecord, PendingWizardState,
    ScheduledPromptRecord,
};
use sled::{Db, IVec, Tree};

const SCHEDULED_PROMPTS_TREE: &str = "scheduled_prompts";
const SCHEDULED_PROMPT_PENDING_TREE: &str = "scheduled_prompt_pending";

#[derive(Clone)]
pub struct ScheduledStorage {
    pub scheduled: Tree,
    pub pending: Tree,
}

impl ScheduledStorage {
    pub fn new(db: &Db) -> sled::Result<Self> {
        let scheduled = db.open_tree(SCHEDULED_PROMPTS_TREE)?;
        let pending = db.open_tree(SCHEDULED_PROMPT_PENDING_TREE)?;
        Ok(Self { scheduled, pending })
    }

    pub fn put_schedule(&self, record: &ScheduledPromptRecord) -> sled::Result<()> {
        let key = record.id.as_bytes();
        let bytes = bincode::encode_to_vec(record, bincode::config::standard()).unwrap();
        self.scheduled.insert(key, bytes)?;
        Ok(())
    }

    fn migrate_legacy_scheduled_prompt(
        &self,
        legacy: LegacyScheduledPromptRecord,
    ) -> ScheduledPromptRecord {
        ScheduledPromptRecord {
            id: legacy.id,
            group_id: legacy.group_id,
            creator_user_id: legacy.creator_user_id,
            creator_username: legacy.creator_username,
            prompt: legacy.prompt,
            image_url: None, // Default for migrated records
            start_hour_utc: legacy.start_hour_utc,
            start_minute_utc: legacy.start_minute_utc,
            repeat: legacy.repeat,
            active: legacy.active,
            created_at: legacy.created_at,
            last_run_at: legacy.last_run_at,
            next_run_at: legacy.next_run_at,
            run_count: legacy.run_count,
            locked_until: legacy.locked_until,
            scheduler_job_id: legacy.scheduler_job_id,
            conversation_response_id: legacy.conversation_response_id,
            thread_id: legacy.thread_id,
        }
    }

    fn migrate_legacy_pending_wizard_state(
        &self,
        legacy: LegacyPendingWizardState,
    ) -> PendingWizardState {
        PendingWizardState {
            group_id: legacy.group_id,
            creator_user_id: legacy.creator_user_id,
            creator_username: legacy.creator_username,
            step: legacy.step,
            prompt: legacy.prompt,
            image_url: None, // Default for migrated records
            hour_utc: legacy.hour_utc,
            minute_utc: legacy.minute_utc,
            repeat: legacy.repeat,
            thread_id: legacy.thread_id,
        }
    }

    pub fn get_schedule(&self, id: &str) -> Option<ScheduledPromptRecord> {
        self.scheduled
            .get(id.as_bytes())
            .ok()
            .flatten()
            .and_then(|ivec: IVec| {
                // Try to deserialize as new format first
                if let Ok((record, _)) = bincode::decode_from_slice::<ScheduledPromptRecord, _>(
                    &ivec,
                    bincode::config::standard(),
                ) {
                    Some(record)
                } else {
                    // Try to deserialize as legacy format and migrate
                    if let Ok((legacy_record, _)) =
                        bincode::decode_from_slice::<LegacyScheduledPromptRecord, _>(
                            &ivec,
                            bincode::config::standard(),
                        )
                    {
                        let migrated_record = self.migrate_legacy_scheduled_prompt(legacy_record);
                        // Save migrated record back to database
                        if let Err(e) = self.put_schedule(&migrated_record) {
                            log::error!(
                                "Failed to save migrated scheduled prompt {}: {}",
                                migrated_record.id,
                                e
                            );
                        } else {
                            log::info!("Migrated legacy scheduled prompt: {}", migrated_record.id);
                        }
                        Some(migrated_record)
                    } else {
                        log::error!(
                            "Failed to decode scheduled prompt {} in both new and legacy formats",
                            id
                        );
                        None
                    }
                }
            })
    }

    #[allow(dead_code)]
    pub fn delete_schedule(&self, id: &str) -> sled::Result<()> {
        self.scheduled.remove(id.as_bytes())?;
        Ok(())
    }

    pub fn list_schedules_for_group(&self, group_id: i64) -> Vec<ScheduledPromptRecord> {
        let mut out = Vec::new();
        for kv in self.scheduled.iter() {
            if let Ok((k, ivec)) = kv {
                // Try to deserialize as new format first
                let rec_string =
                    bincode::decode_from_slice::<String, _>(&ivec, bincode::config::standard());

                let id = bincode::decode_from_slice::<String, _>(&k, bincode::config::standard());

                if let Ok(id) = id {
                    log::info!("id: {}", id.0);
                } else {
                    log::error!("Error decoding scheduled prompt id: {:?}", id.err());
                };

                if let Ok(rec_string) = rec_string {
                    log::info!("rec_string: {}", rec_string.0);
                } else {
                    log::error!(
                        "Error decoding scheduled prompt in both new and legacy formats: {:?}",
                        rec_string.err()
                    );
                };

                if let Ok((rec, _)) = bincode::decode_from_slice::<ScheduledPromptRecord, _>(
                    &ivec,
                    bincode::config::standard(),
                ) {
                    if rec.group_id == group_id && rec.active {
                        out.push(rec);
                    }
                } else {
                    // Try to deserialize as legacy format and migrate
                    if let Ok((legacy_rec, _)) =
                        bincode::decode_from_slice::<LegacyScheduledPromptRecord, _>(
                            &ivec,
                            bincode::config::standard(),
                        )
                    {
                        let migrated_rec = self.migrate_legacy_scheduled_prompt(legacy_rec);
                        // Save migrated record back to database
                        if let Err(e) = self.put_schedule(&migrated_rec) {
                            log::error!(
                                "Failed to save migrated scheduled prompt {}: {}",
                                migrated_rec.id,
                                e
                            );
                        } else {
                            log::info!("Migrated legacy scheduled prompt: {}", migrated_rec.id);
                        }
                        if migrated_rec.group_id == group_id && migrated_rec.active {
                            out.push(migrated_rec);
                        }
                    } else {
                        if let Err(e) = String::from_utf8(ivec.to_vec()) {
                            log::error!(
                                "Error decoding scheduled prompt in both new and legacy formats: {:?}",
                                e
                            );
                        } else {
                            log::info!("erro to decode");
                        }
                    }
                }
            } else {
                log::error!("Error getting scheduled prompt: {:?}", kv);
            }
        }
        out
    }

    pub fn put_pending(&self, key: (&i64, &i64), state: &PendingWizardState) -> sled::Result<()> {
        let k = Self::pending_key_bytes(key);
        let bytes = bincode::encode_to_vec(state, bincode::config::standard()).unwrap();
        self.pending.insert(k, bytes)?;
        Ok(())
    }

    pub fn get_pending(&self, key: (&i64, &i64)) -> Option<PendingWizardState> {
        let k = Self::pending_key_bytes(key);
        self.pending.get(k).ok().flatten().and_then(|ivec: IVec| {
            // Try to deserialize as new format first
            if let Ok((state, _)) = bincode::decode_from_slice::<PendingWizardState, _>(&ivec, bincode::config::standard()) {
                Some(state)
            } else {
                // Try to deserialize as legacy format and migrate
                if let Ok((legacy_state, _)) = bincode::decode_from_slice::<LegacyPendingWizardState, _>(&ivec, bincode::config::standard()) {
                    let migrated_state = self.migrate_legacy_pending_wizard_state(legacy_state);
                    // Save migrated state back to database
                    if let Err(e) = self.put_pending(key, &migrated_state) {
                        log::error!("Failed to save migrated pending wizard state for group {} user {}: {}", key.0, key.1, e);
                    } else {
                        log::info!("Migrated legacy pending wizard state for group {} user {}", key.0, key.1);
                    }
                    Some(migrated_state)
                } else {
                    log::error!("Failed to decode pending wizard state in both new and legacy formats for group {} user {}", key.0, key.1);
                    None
                }
            }
        })
    }

    pub fn delete_pending(&self, key: (&i64, &i64)) -> sled::Result<()> {
        let k = Self::pending_key_bytes(key);
        self.pending.remove(k)?;
        Ok(())
    }

    fn pending_key_bytes(key: (&i64, &i64)) -> Vec<u8> {
        let mut v = Vec::with_capacity(16);
        v.extend_from_slice(&key.0.to_be_bytes());
        v.extend_from_slice(&key.1.to_be_bytes());
        v
    }
}
