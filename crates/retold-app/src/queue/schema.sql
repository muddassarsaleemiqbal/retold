CREATE TABLE IF NOT EXISTS jobs (
    id TEXT PRIMARY KEY,
    idempotency_key TEXT UNIQUE NOT NULL,
    video_ref TEXT NOT NULL,
    plan_hash TEXT NOT NULL,
    status TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    error_code TEXT,
    error_message TEXT
);
CREATE INDEX IF NOT EXISTS idx_jobs_status_priority ON jobs(status, priority DESC, created_at ASC);
CREATE TABLE IF NOT EXISTS job_execution (
    job_id TEXT PRIMARY KEY REFERENCES jobs(id),
    intent TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS job_outcomes (
    job_id TEXT PRIMARY KEY REFERENCES jobs(id),
    result TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS job_events (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id TEXT NOT NULL REFERENCES jobs(id),
    body TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_job_events ON job_events(job_id, sequence);
CREATE INDEX IF NOT EXISTS idx_jobs_order ON jobs(priority DESC, created_at ASC, id ASC);
CREATE TABLE IF NOT EXISTS queue_revision (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    identity TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK(revision >= 0)
);
INSERT OR IGNORE INTO queue_revision(singleton, identity, revision) VALUES(1, lower(hex(randomblob(16))), 0);
CREATE TRIGGER IF NOT EXISTS queue_revision_insert AFTER INSERT ON jobs BEGIN
    UPDATE queue_revision SET revision = revision + 1 WHERE singleton = 1;
END;
CREATE TRIGGER IF NOT EXISTS queue_revision_update AFTER UPDATE ON jobs BEGIN
    UPDATE queue_revision SET revision = revision + 1 WHERE singleton = 1;
END;
CREATE TRIGGER IF NOT EXISTS queue_revision_delete AFTER DELETE ON jobs BEGIN
    UPDATE queue_revision SET revision = revision + 1 WHERE singleton = 1;
END;
CREATE TABLE IF NOT EXISTS queue_controls (
    control_id TEXT PRIMARY KEY,
    receipt TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS queue_dispatch (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    control_id TEXT NOT NULL REFERENCES queue_controls(control_id)
);
-- Older writers must also respect a hold installed by a newer application.
CREATE TRIGGER IF NOT EXISTS queue_dispatch_insert BEFORE INSERT ON jobs
WHEN NEW.status IN ('queued', 'running') AND EXISTS (SELECT 1 FROM queue_dispatch) BEGIN
    SELECT RAISE(ABORT, 'Workspace dispatch is inhibited by Stop all');
END;
CREATE TRIGGER IF NOT EXISTS queue_dispatch_update BEFORE UPDATE OF status ON jobs
WHEN NEW.status IN ('queued', 'running') AND EXISTS (SELECT 1 FROM queue_dispatch) BEGIN
    SELECT RAISE(ABORT, 'Workspace dispatch is inhibited by Stop all');
END;
