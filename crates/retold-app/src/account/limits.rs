//! Targeted limit changes. One scope and window changes inside the same transaction as
//! reservations; incurred charges and holds are never touched, so lowering a limit below what is
//! already used blocks new paid work instead of erasing history.
use super::report::{self, Ceiling};
use super::types::{Limit, Resources, Scope, Unit, Usd, Window};
use super::{Account, Error, Result, database, policy, state::State};
use crate::protocol::{Outcome, Payload};
use serde::Serialize;

/// Raw command inputs; parsing errors are input errors, never partial changes.
#[derive(Debug)]
pub struct Input<'a> {
    pub scope: &'a str,
    pub window: Window,
    pub usd: Option<&'a str>,
    pub requests: Option<u64>,
    /// Further dimensions as `UNIT=AMOUNT`, such as `local_millis=600000`.
    pub resources: &'a [String],
    pub remove: bool,
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
pub struct Change {
    pub scope: Scope,
    pub window: Window,
    /// Units to set, merged over an existing limit; `None` removes the limit.
    pub resources: Option<Resources>,
}

#[derive(Debug, Serialize)]
pub struct Changed {
    pub scope: Scope,
    pub window: Window,
    pub before: Option<Resources>,
    pub after: Option<Resources>,
    pub dry_run: bool,
    /// What admission would count against the new limit now.
    pub headroom: Option<Ceiling>,
    /// Units with nothing left: new paid work in this scope is blocked until the limit rises
    /// or, for day and month limits, the period ends.
    pub exhausted: Vec<Unit>,
}

impl Account {
    pub fn change_limit(&self, change: &Change) -> Result<Changed> {
        self.change(|state| {
            let (state, changed) = apply(state, change)?;
            changed.finish(&state).map(|changed| (state, changed))
        })
    }

    /// The same change computed on a read-only copy; nothing is written.
    pub fn preview_limit(&self, change: &Change) -> Result<Changed> {
        let mut state = database::read(&self.directory)?;
        state.last_seen = state.last_seen.max(self.clock.now());
        let (state, changed) = apply(state, change)?;
        let preview = changed.finish(&state)?;
        Ok(Changed {
            dry_run: true,
            ..preview
        })
    }
}

struct Transition {
    change: Change,
    before: Option<Resources>,
    after: Option<Resources>,
}

impl Transition {
    fn finish(self, state: &State) -> Result<Changed> {
        let headroom = report::headroom(state, &self.change.scope, self.change.window)?;
        let exhausted = headroom
            .iter()
            .flatten()
            .filter(|(_, room)| room.available == 0)
            .map(|(&unit, _)| unit)
            .collect();
        Ok(Changed {
            scope: self.change.scope,
            window: self.change.window,
            before: self.before,
            after: self.after,
            dry_run: false,
            headroom,
            exhausted,
        })
    }
}

fn apply(mut state: State, change: &Change) -> Result<(State, Transition)> {
    let position = state
        .limits
        .iter()
        .position(|limit| limit.scope == change.scope && limit.window == change.window);
    let before = position.map(|index| state.limits.remove(index).resources);
    let after = merged(before.as_ref(), change.resources.as_ref())?;
    if let Some(resources) = &after {
        state.limits.push(Limit {
            scope: change.scope.clone(),
            window: change.window,
            resources: resources.clone(),
        });
    }
    let scope = serde_json::to_string(&change.scope).map_err(|_| Error::Corrupt)?;
    state.note("limit_changed", &scope);
    let transition = Transition {
        change: change.clone(),
        before,
        after,
    };
    Ok((state, transition))
}

/// New units over the existing ones. A new limit must bound money and requests, because every
/// paid quote carries both and a missing unit would silently block all work in the scope.
fn merged(before: Option<&Resources>, update: Option<&Resources>) -> Result<Option<Resources>> {
    let Some(update) = update else {
        return before
            .map(|_| None)
            .ok_or(Error::Invalid("No limit is set for that scope and window."));
    };
    let mut resources = before.cloned().unwrap_or_default();
    resources.extend(update.iter().map(|(&unit, &amount)| (unit, amount)));
    let bounded = [Unit::UsdMicros, Unit::Requests]
        .iter()
        .all(|unit| resources.contains_key(unit));
    if !bounded {
        return Err(Error::Invalid(
            "A new limit must set both --usd and --requests.",
        ));
    }
    Ok(Some(resources))
}

/// Builds a scope from the identifier after its kind.
type ScopeOf = fn(String) -> Scope;
const SCOPE_KINDS: [(&str, ScopeOf); 4] = [
    ("workspace", Scope::Workspace),
    ("video", Scope::Video),
    ("series", Scope::Series),
    ("provider", Scope::Provider),
];
const SCOPES: &str = "Scope must be account, workspace:ID, video:ID, series:ID or provider:ID.";

/// `account`, or `workspace:ID`, `video:ID`, `series:ID` and `provider:ID`. Provider account IDs
/// may contain further colons, as in `provider:openrouter:OPENROUTER_API_KEY`.
pub fn parse_scope(text: &str) -> Result<Scope> {
    if text == "account" {
        return Ok(Scope::Account);
    }
    let (kind, id) = text.split_once(':').ok_or(Error::Invalid(SCOPES))?;
    policy::identifier(id)?;
    let (_, scope) = SCOPE_KINDS
        .iter()
        .find(|(name, _)| *name == kind)
        .ok_or(Error::Invalid(SCOPES))?;
    Ok(scope(id.into()))
}

impl Input<'_> {
    fn change(&self) -> Result<Change> {
        let scope = parse_scope(self.scope)?;
        let resources = if self.remove {
            if self.sets_anything() {
                return Err(Error::Invalid("--remove takes no amounts."));
            }
            None
        } else {
            Some(self.resources()?)
        };
        Ok(Change {
            scope,
            window: self.window,
            resources,
        })
    }

    const fn sets_anything(&self) -> bool {
        let amounts = self.usd.is_some() || self.requests.is_some();
        amounts || !self.resources.is_empty()
    }

    fn resources(&self) -> Result<Resources> {
        let mut resources = Resources::new();
        for text in self.resources {
            let (unit, amount) = parse_resource(text)?;
            let duplicate = resources.insert(unit, amount).is_some();
            if duplicate {
                return Err(Error::Invalid("Each unit may be set once."));
            }
        }
        let usd = self.usd.map(parse_usd).transpose()?;
        let named = [(Unit::UsdMicros, usd), (Unit::Requests, self.requests)];
        for (unit, amount) in named {
            let Some(amount) = amount else { continue };
            let duplicate = resources.insert(unit, amount).is_some();
            if duplicate {
                return Err(Error::Invalid("Each unit may be set once."));
            }
        }
        if resources.is_empty() {
            return Err(Error::Invalid(
                "Set at least one of --usd, --requests or --resource UNIT=AMOUNT, or use --remove.",
            ));
        }
        Ok(resources)
    }
}

fn parse_usd(text: &str) -> Result<u64> {
    let invalid = || Error::Invalid("--usd needs a non-negative amount such as 5 or 0.50.");
    let usd: Usd = text.parse().map_err(|_| invalid())?;
    u64::try_from(usd.micros()).map_err(|_| invalid())
}

fn parse_resource(text: &str) -> Result<(Unit, u64)> {
    let invalid =
        || Error::Invalid("Expected UNIT=AMOUNT with a known unit and a whole, finite amount.");
    let (unit, amount) = text.split_once('=').ok_or_else(invalid)?;
    let unit: Unit =
        serde_json::from_value(serde_json::Value::String(unit.into())).map_err(|_| invalid())?;
    let amount = amount.parse().map_err(|_| invalid())?;
    Ok((unit, amount))
}

#[must_use]
pub fn respond(input: &Input<'_>) -> Payload {
    let result = input.change().and_then(|change| {
        let directory = super::canonical_directory()?;
        let account = Account::open(&directory)?;
        if input.dry_run {
            account.preview_limit(&change)
        } else {
            account.change_limit(&change)
        }
    });
    match result {
        Ok(changed) => Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::json!({"limit_changed": changed}),
        },
        Err(error) => super::commands::diagnostic(&error).into_payload(),
    }
}
