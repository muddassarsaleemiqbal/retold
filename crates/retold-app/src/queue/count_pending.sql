SELECT count(*) FROM jobs WHERE status IN ('queued','running','pausing','paused','stopping')
