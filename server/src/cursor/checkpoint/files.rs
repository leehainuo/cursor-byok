//! Persists complete file snapshots for Cursor's change review.
use crate::{cursor::protocol::proto::agent::v1 as pb, Result};

use super::{CheckpointBuilder, PendingSteps};

impl CheckpointBuilder {
    pub(super) async fn project_files(&mut self, presentation: &PendingSteps) -> Result<()> {
        for snapshot in &presentation.edit_snapshots {
            let initial_content = match self.base.file_states_v2.get(&snapshot.path) {
                Some(state) => state.initial_content.clone(),
                None => Some(
                    self.sync
                        .persist(snapshot.before.as_bytes(), &[])
                        .await?
                        .as_bytes()
                        .to_vec(),
                ),
            };
            let content = self.sync.persist(snapshot.after.as_bytes(), &[]).await?;
            self.base.file_states_v2.insert(
                snapshot.path.clone(),
                pb::FileStateStructure {
                    content: Some(content.as_bytes().to_vec()),
                    initial_content,
                },
            );
        }
        Ok(())
    }
}
