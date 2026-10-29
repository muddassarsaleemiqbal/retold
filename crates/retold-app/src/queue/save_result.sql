INSERT INTO job_outcomes(job_id, result) VALUES (?1, ?2)
ON CONFLICT(job_id) DO UPDATE SET result = excluded.result
