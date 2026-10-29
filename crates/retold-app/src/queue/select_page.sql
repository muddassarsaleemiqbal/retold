SELECT id, idempotency_key, video_ref, plan_hash, status, priority, created_at, updated_at, error_code, error_message
FROM jobs WHERE (?1 IS NULL OR status = ?1)
ORDER BY priority DESC, created_at ASC, id ASC LIMIT ?2 OFFSET ?3
