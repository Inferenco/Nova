use crate::scheduled_prompts::dto::{
    decode_pending_wizard_state, decode_scheduled_prompt_record, encode_pending_wizard_state,
    encode_scheduled_prompt_record, PendingWizardState, ScheduledPromptRecord,
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
        let bytes = encode_scheduled_prompt_record(record);
        self.scheduled.insert(key, bytes)?;
        Ok(())
    }

    pub fn get_schedule(&self, id: &str) -> Option<ScheduledPromptRecord> {
        self.scheduled
            .get(id.as_bytes())
            .ok()
            .flatten()
            .and_then(|ivec: IVec| decode_scheduled_prompt_record(&ivec).ok())
    }

    #[allow(dead_code)]
    pub fn delete_schedule(&self, id: &str) -> sled::Result<()> {
        self.scheduled.remove(id.as_bytes())?;
        Ok(())
    }

    pub fn list_schedules_for_group(&self, group_id: i64) -> Vec<ScheduledPromptRecord> {
        let mut out = Vec::new();
        for kv in self.scheduled.iter() {
            if let Ok((_k, ivec)) = kv {
                if let Ok(rec) = decode_scheduled_prompt_record(&ivec) {
                    if rec.group_id == group_id && rec.active {
                        out.push(rec);
                    } else {
                        log::error!("Scheduled prompt is not active: {:?}", rec);
                    }
                } else {
                    log::error!("Error decoding scheduled prompt: {:?}", ivec);
                }
            } else {
                log::error!("Error getting scheduled prompt: {:?}", kv);
            }
        }
        out
    }

    pub fn put_pending(&self, key: (&i64, &i64), state: &PendingWizardState) -> sled::Result<()> {
        let k = Self::pending_key_bytes(key);
        let bytes = encode_pending_wizard_state(state);
        self.pending.insert(k, bytes)?;
        Ok(())
    }

    pub fn get_pending(&self, key: (&i64, &i64)) -> Option<PendingWizardState> {
        let k = Self::pending_key_bytes(key);
        self.pending
            .get(k)
            .ok()
            .flatten()
            .and_then(|ivec: IVec| decode_pending_wizard_state(&ivec).ok())
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
