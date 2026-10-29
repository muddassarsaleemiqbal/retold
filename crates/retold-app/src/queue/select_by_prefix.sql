SELECT id, idempotency_key, video_ref, plan_hash, status, priority, created_at, updated_at, error_code, error_message FROM jobs WHERE substr(id, 1, length(?1)) = ?1 ORDER BY id LIMIT 2
