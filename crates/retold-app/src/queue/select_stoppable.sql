SELECT id, idempotency_key, video_ref, plan_hash, status, priority, created_at, updated_at, error_code, error_message
FROM jobs WHERE status IN ('queued', 'paused', 'running', 'pausing', 'stopping') ORDER BY id
