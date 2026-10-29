//! A job and its saved execution outcome are read from one consistent SQLite snapshot.
use serde::{Deserialize, Serialize};

use super::{ExecutionIntent, JobRecord, QueueError, QueueStore};
use crate::protocol::Payload;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobDetails {
    pub job: JobRecord,
    pub result: Option<Payload>,
    pub execution: Option<ExecutionIntent>,
}

impl QueueStore {
    pub fn details(&self, reference: &str) -> Result<Option<JobDetails>, QueueError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        let found = super::store::read_by_reference(&transaction, reference)?;
        let details = found
            .map(|job| {
                Ok::<_, QueueError>(JobDetails {
                    result: super::durable::read_result(&transaction, &job.id)?,
                    execution: super::durable::read_intent(&transaction, &job.id)?,
                    job,
                })
            })
            .transpose()?;
        transaction.commit()?;
        Ok(details)
    }
}
