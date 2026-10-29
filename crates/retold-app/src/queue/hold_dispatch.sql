INSERT INTO queue_dispatch(singleton, control_id) VALUES (1, ?1)
ON CONFLICT(singleton) DO UPDATE SET control_id = excluded.control_id
