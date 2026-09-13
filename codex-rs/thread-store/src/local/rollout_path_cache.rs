use std::collections::VecDeque;
use std::io;
use std::path::Path;
use std::path::PathBuf;

use codex_protocol::ThreadId;
use tokio::sync::Mutex;

const MAX_CACHED_ROLLOUT_PATHS: usize = 256;

/// Reuses locations of immutable ancestor rollouts across history pages.
///
/// Only locations are cached: readers still load metadata and enforce lineage boundaries.
/// Every hit checks the file, and moved or deleted files fall back to normal discovery.
/// A thread's current rollout is deliberately resolved separately, since revert can change it.
#[derive(Default)]
pub(super) struct RolloutPathCache {
    entries: Mutex<VecDeque<(ThreadId, PathBuf)>>,
}

impl RolloutPathCache {
    pub(super) async fn resolve(
        &self,
        codex_home: &Path,
        rollout_id: ThreadId,
    ) -> io::Result<Option<PathBuf>> {
        let cached = {
            let mut entries = self.entries.lock().await;
            let entry = entries
                .iter()
                .position(|(id, _)| *id == rollout_id)
                .and_then(|index| entries.remove(index));
            if let Some(entry) = &entry {
                entries.push_front(entry.clone());
            }
            entry.map(|(_, path)| path)
        };
        if let Some(path) = cached
            && tokio::fs::symlink_metadata(&path)
                .await
                .is_ok_and(|metadata| metadata.is_file())
        {
            return Ok(Some(path));
        }

        // Do not cache misses or hold the cache lock across filesystem operations.
        let path = codex_rollout::find_rollout_path_by_rollout_id(codex_home, rollout_id).await?;
        let mut entries = self.entries.lock().await;
        entries.retain(|(id, _)| *id != rollout_id);
        if let Some(path) = &path {
            entries.push_front((rollout_id, path.clone()));
            entries.truncate(MAX_CACHED_ROLLOUT_PATHS);
        }
        Ok(path)
    }
}
