SELECT sequence, body FROM job_events WHERE job_id = ?1 AND sequence > ?2 ORDER BY sequence LIMIT ?3
