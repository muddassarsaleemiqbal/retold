SELECT COALESCE(MAX(sequence), 0) FROM job_events WHERE job_id = ?1
