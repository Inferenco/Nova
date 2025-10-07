use crate::scheduled_payments::dto::{PendingPaymentWizardState, ScheduledPaymentRecord};
use anyhow::Result as anyhowResult;
use sled::{Db, IVec, Tree};

const SCHEDULED_PAYMENTS_TREE: &str = "scheduled_payments";
const SCHEDULED_PAYMENT_PENDING_TREE: &str = "scheduled_payment_pending";

#[derive(Clone)]
pub struct ScheduledPaymentsStorage {
    pub scheduled: Tree,
    pub pending: Tree,
}

impl ScheduledPaymentsStorage {
    pub fn new(db: &Db) -> sled::Result<Self> {
        let scheduled = db.open_tree(SCHEDULED_PAYMENTS_TREE)?;
        let pending = db.open_tree(SCHEDULED_PAYMENT_PENDING_TREE)?;
        Ok(Self { scheduled, pending })
    }

    pub fn put_schedule(&self, record: &ScheduledPaymentRecord) -> anyhowResult<()> {
        let key = record.id.as_bytes();
        let bytes = serde_json::to_vec(record)?;
        self.scheduled.insert(key, bytes)?;
        Ok(())
    }

    pub fn get_schedule(&self, id: &str) -> Option<ScheduledPaymentRecord> {
        self.scheduled
            .get(id.as_bytes())
            .ok()
            .flatten()
            .and_then(|ivec: IVec| {
                serde_json::from_slice(&ivec)
                    .map_err(|e| {
                        log::error!("Failed to decode scheduled payment {}: {}", id, e);
                        e
                    })
                    .ok()
            })
    }

    pub fn list_schedules_for_group(&self, group_id: i64) -> Vec<ScheduledPaymentRecord> {
        let mut out = Vec::new();
        for kv in self.scheduled.iter() {
            if let Ok((_k, ivec)) = kv {
                if let Ok(rec) = serde_json::from_slice::<ScheduledPaymentRecord>(&ivec) {
                    if rec.group_id == group_id && rec.active {
                        out.push(rec);
                    }
                } else {
                    log::error!("Failed to decode scheduled payment");
                }
            }
        }
        out
    }

    pub fn put_pending(
        &self,
        key: (&i64, &i64),
        state: &PendingPaymentWizardState,
    ) -> anyhowResult<()> {
        let k = Self::pending_key_bytes(key);
        let bytes = serde_json::to_vec(state)?;
        self.pending.insert(k, bytes)?;
        Ok(())
    }

    pub fn get_pending(&self, key: (&i64, &i64)) -> Option<PendingPaymentWizardState> {
        let k = Self::pending_key_bytes(key);
        self.pending.get(k).ok().flatten().and_then(|ivec: IVec| {
            serde_json::from_slice(&ivec)
                .map_err(|e| {
                    log::error!(
                        "Failed to decode pending payment wizard state for group {} user {}: {}",
                        key.0,
                        key.1,
                        e
                    );
                    e
                })
                .ok()
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
