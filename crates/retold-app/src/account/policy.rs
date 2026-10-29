//! Authority validation and resource aggregation share the same transaction as reservations.
use super::types::{Action, Capability, Job, Limit, Workspace};
use super::{Error, Result, state::State};

pub fn identifier(value: &str) -> Result<()> {
    const MAX_ID_BYTES: usize = 256;
    let valid_characters = value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:".contains(&byte));
    let bounded = !value.is_empty() && value.len() <= MAX_ID_BYTES;
    if !bounded || !valid_characters {
        return Err(Error::Invalid(
            "Expected a bounded identifier, not a path or secret.",
        ));
    }
    Ok(())
}

pub fn digest(value: &str) -> Result<()> {
    const HASH_BYTES: usize = 64;
    let valid = value.len() == HASH_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if !valid {
        return Err(Error::Invalid(
            "Expected a canonical lowercase content/plan hash.",
        ));
    }
    Ok(())
}

pub fn set_limits(mut state: State, limits: Vec<Limit>) -> Result<(State, ())> {
    for (index, limit) in limits.iter().enumerate() {
        let duplicate = limits[..index]
            .iter()
            .any(|prior| prior.scope == limit.scope && prior.window == limit.window);
        if duplicate {
            return Err(Error::Invalid(
                "Duplicate scope/window limits are not allowed.",
            ));
        }
    }
    state.limits = limits;
    state.note("limits_changed", "account");
    Ok((state, ()))
}

pub fn register_workspace(mut state: State, mut workspace: Workspace) -> Result<(State, ())> {
    identifier(&workspace.id)?;
    workspace.root = workspace
        .root
        .canonicalize()
        .map_err(|_| Error::Invalid("Workspace root must exist."))?;
    require_active(&workspace.root)?;
    let directory = workspace.root.is_dir();
    if !directory {
        return Err(Error::Invalid("Workspace root must be a directory."));
    }
    let conflict = state.workspaces.values().any(|prior| {
        (prior.id == workspace.id || prior.root == workspace.root) && *prior != workspace
    });
    if conflict {
        return Err(Error::Invalid(
            "Workspace ID/root binding conflicts with a registered workspace.",
        ));
    }
    state.note("workspace_registered", &workspace.id);
    state.workspaces.insert(workspace.id.clone(), workspace);
    Ok((state, ()))
}

pub fn register_route(mut state: State, route: &str, provider: &str) -> Result<(State, ())> {
    identifier(route)?;
    identifier(provider)?;
    let rebound = state
        .routes
        .get(route)
        .is_some_and(|prior| prior != provider);
    if rebound {
        return Err(Error::Invalid(
            "A registered route cannot silently change its canonical provider account.",
        ));
    }
    state.routes.insert(route.into(), provider.into());
    state.note("route_registered", route);
    Ok((state, ()))
}

pub fn admit(mut state: State, job: Job) -> Result<(State, ())> {
    validate_job(&state, &job)?;
    let prior = state.jobs.get(&job.id);
    if let Some(prior) = prior {
        return if *prior == job {
            Ok((state, ()))
        } else {
            Err(Error::Invalid(
                "Job IDs cannot replace historical authority.",
            ))
        };
    }
    state.note("job_admitted", &job.id);
    state.jobs.insert(job.id.clone(), job);
    Ok((state, ()))
}

pub fn validate_job(state: &State, job: &Job) -> Result<()> {
    identifier(&job.id)?;
    identifier(&job.video_id)?;
    digest(&job.plan_hash)?;
    validate_binding(state, job)?;
    validate_versions(job)?;
    validate_action(job)
}

fn validate_binding(state: &State, job: &Job) -> Result<()> {
    let root = job
        .workspace
        .root
        .canonicalize()
        .map_err(|_| Error::Invalid("Workspace root is unavailable."))?;
    require_active(&root)?;
    let registered = state
        .workspaces
        .get(&job.workspace.id)
        .is_some_and(|binding| *binding == job.workspace && binding.root == root);
    if !registered {
        return Err(Error::Invalid(
            "Workspace is not bound to this account/root.",
        ));
    }
    let expired = job
        .deadline
        .is_some_and(|deadline| deadline <= state.last_seen);
    if expired {
        return Err(Error::Invalid("Job authority expired."));
    }
    for route in &job.routes {
        let registered = state.routes.contains_key(route);
        if !registered {
            return Err(Error::Invalid("Unknown canonical route binding."));
        }
    }
    Ok(())
}

fn validate_versions(job: &Job) -> Result<()> {
    for operation in job.operations.keys() {
        digest(operation)?;
    }
    for (name, hash) in &job.input_versions {
        identifier(name)?;
        digest(hash)?;
    }
    if let Some(series) = &job.series_id {
        identifier(series)?;
    }
    let empty = job.input_versions.is_empty();
    if empty {
        return Err(Error::Invalid("Fresh authority must pin input versions."));
    }
    Ok(())
}

fn validate_action(job: &Job) -> Result<()> {
    let export_outside_export = job.capabilities.contains(&Capability::Export)
        && !matches!(job.action, Action::Export { .. });
    if export_outside_export {
        return Err(Error::Invalid(
            "Only export authority may create projects or export.",
        ));
    }
    let untargeted_test =
        matches!(&job.action, Action::ProviderTest { providers } if providers.is_empty());
    if untargeted_test {
        return Err(Error::Invalid("Provider tests must name their providers."));
    }
    let incomplete_export = matches!(&job.action, Action::Export { editions, formats } if editions.is_empty() || formats.is_empty());
    if incomplete_export {
        return Err(Error::Invalid(
            "Export authority must name editions and formats.",
        ));
    }
    Ok(())
}

fn require_active(root: &std::path::Path) -> Result<()> {
    retold_engine::recovery::ensure_active(root).map_err(|_| Error::Invalid(
        "Restored workspace is inactive; ownership and accounting reconciliation are required before activation.",
    ))
}
