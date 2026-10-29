WITH RECURSIVE entries(n) AS (
    VALUES(1) UNION ALL SELECT n + 1 FROM entries WHERE n < ?1
)
INSERT INTO jobs(id, idempotency_key, video_ref, plan_hash, status, created_at, updated_at)
SELECT printf('job-%05d', n), printf('key-%05d', n), 'video', 'hash', 'completed',
       '2026-10-20T00:00:00Z', '2026-10-20T00:00:00Z' FROM entries
