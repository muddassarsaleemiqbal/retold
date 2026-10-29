SELECT COUNT(*) FROM jobs WHERE (?1 IS NULL OR status = ?1)
