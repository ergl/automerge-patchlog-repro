use automerge::{ActorId, Automerge, PatchLog, transaction::Transactable as _};

fn main() {
    // Actor IDs chosen so sort order is: actor_a < actor_b < actor_c.
    // actor_b is the local actor on doc_b (never successfully committed).
    // actor_c arrives later in a remote change, triggering the mismatch.
    let actor_a = ActorId::from(b"a".to_vec());
    let actor_b = ActorId::from(b"b".to_vec());
    let actor_c = ActorId::from(b"c".to_vec());

    let mut doc_a = Automerge::new().with_actor(actor_a.clone());
    {
        let mut tx = doc_a.transaction();
        tx.put(&automerge::ROOT, "from_a", "hello").unwrap();
        tx.commit();
    }

    let mut doc_c = doc_a.fork().with_actor(actor_c);
    {
        let mut tx = doc_c.transaction();
        tx.put(&automerge::ROOT, "from_c", "world").unwrap();
        tx.commit();
    }

    let changes = doc_c.get_changes(&doc_a.get_heads());
    let mut doc_b = doc_a.fork().with_actor(actor_b);

    // Start a transaction with an inactive patchlog.
    //
    // When the transaction starts, we call PatchLog::migrate_actors,
    // which includes all the actors in the doc (actor_a and actor_b), and
    // sets the doc's actor to Actor::Cached.
    //
    // Then we perform a no-op (delete non-existent key), and commit.
    //
    // Upon commit, we find `self.pending_ops == 0 && self.seq == 1`,
    // so we remove actor from the doc, and the skip the commit.
    //
    // Notably, the actor is no longer in the ops for the doc, but it
    // still present in the patchlog.
    let patch_log = PatchLog::inactive();
    let mut tx = doc_b.transaction_log_patches(patch_log);
    tx.delete(&automerge::ROOT, "from_b").unwrap();
    let (_, mut patch_log) = tx.commit();

    // Re-use the same patchlog for apply_changes. It contains both [a] and [b]
    // Upon migrate_actors, we try to insert [a, c] into it.
    //
    // When migrating actors again, we will not find [b] in the doc's op-log, so
    // we try to incorporate [a, c] into a patchlog that contains [a, b].
    //
    // We compare [b] and [c], such that b < c, so we throw `PatchLogMismatch`
    let result = doc_b.apply_changes_log_patches(changes, &mut patch_log);
    assert!(result.is_ok(), "apply_changes failed: {:?}", result.err());
}

#[cfg(test)]
mod tests {
    use super::*;
    use automerge::AutoCommit;

    // Same as above, but using Autocommit, which re-uses a patchlog.
    #[test]
    fn noop_op_corrupts_patchlog_autocommit() {
        let actor_a = ActorId::from(b"a".to_vec());
        let actor_b = ActorId::from(b"b".to_vec());
        let actor_c = ActorId::from(b"c".to_vec());

        let mut doc_a = AutoCommit::new().with_actor(actor_a.clone());
        doc_a.put(&automerge::ROOT, "from_a", "hello").unwrap();

        let mut doc_c = doc_a.fork().with_actor(actor_c);
        doc_c.put(&automerge::ROOT, "from_c", "world").unwrap();
        let changes = doc_c.get_changes(&doc_a.get_heads());

        let mut doc_b = doc_a.fork().with_actor(actor_b);
        doc_b.delete(&automerge::ROOT, "nonexistent").unwrap();

        let result = doc_b.apply_changes(changes);
        assert!(result.is_ok(), "apply_changes failed: {:?}", result.err());
    }
}
