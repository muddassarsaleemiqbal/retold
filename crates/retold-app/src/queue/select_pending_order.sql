SELECT id FROM jobs WHERE status = 'queued' ORDER BY priority DESC, created_at ASC, id ASC
