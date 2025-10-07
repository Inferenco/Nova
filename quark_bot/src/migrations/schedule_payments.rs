use anyhow::Result as anyhowResult;

use crate::scheduled_payments::{dto::ScheduledPaymentRecord, storage::ScheduledPaymentsStorage};

pub fn update_bincode_to_serder(scheduled_payments: ScheduledPaymentsStorage) -> anyhowResult<()> {
    log::info!("Migrating scheduled payments to serde");
    let scheduled_payments_tree = &scheduled_payments.scheduled;

    for item in scheduled_payments_tree.iter() {
        let (key, value) = item?;
        let scheduled_payment_record = bincode::decode_from_slice::<ScheduledPaymentRecord, _>(
            &value,
            bincode::config::standard(),
        );

        match scheduled_payment_record {
            Err(_) => {
                let record = serde_json::from_slice::<ScheduledPaymentRecord>(&value);
                if record.is_err() {
                    scheduled_payments_tree.remove(&key)?;
                    log::warn!("Removed scheduled payment for corrupted record");
                }
            }
            Ok((record, _)) => {
                scheduled_payments.put_schedule(&record)?;
                log::info!("Migrated scheduled payment: {}", record.id);
            }
        }
    }

    log::info!("Finished migrating scheduled payments to serde");
    Ok(())
}
