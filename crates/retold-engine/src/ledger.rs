//! Durable paid-call reservations. A crash or ambiguous network error retains its hold.
//! The caller must recover a committed result or explicitly reconcile billing, not retry.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use retold_core::Usd;
use serde::{Deserialize, Serialize};

use crate::gate::{GateError, PaidCall, PaidCapability, PaidEvidence, PaidGate};
use crate::{EngineError, fsx};

const MAX_RETRIES: usize = 1_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum Charge {
    Reserved {
        ceiling: Usd,
    },
    Exact {
        cost: Usd,
    },
    /// The provider returned a result but no exact cost. The full quote remains charged
    /// to the budget until an exact amount can be recovered.
    Conservative {
        ceiling: Usd,
        generation: Option<String>,
    },
    Released,
}

impl Charge {
    #[must_use]
    pub const fn amount(&self) -> Usd {
        match self {
            Self::Reserved { ceiling } | Self::Conservative { ceiling, .. } => *ceiling,
            Self::Exact { cost } => *cost,
            Self::Released => Usd::ZERO,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error("budget reached: {used} charged or reserved, {requested} requested, limit {budget}")]
    Budget {
        used: Usd,
        requested: Usd,
        budget: Usd,
    },
    #[error(
        "paid request {0} has an unresolved charge; restore its saved result or reconcile billing before retrying"
    )]
    Uncertain(String),
    #[error(transparent)]
    Gate(#[from] GateError),
}

/// A paid call before it has an attempt slot.
#[derive(Debug, Clone, Copy)]
pub struct Purchase<'a> {
    pub logical: &'a str,
    pub stage: &'a str,
    pub provider: &'a str,
    pub capability: PaidCapability,
    pub ceiling: Usd,
}

impl Purchase<'_> {
    const fn call<'a>(&'a self, request: &'a str) -> PaidCall<'a> {
        PaidCall {
            request,
            logical: self.logical,
            stage: self.stage,
            provider: self.provider,
            capability: self.capability,
            ceiling: self.ceiling,
        }
    }
}

#[derive(Debug)]
pub struct Ledger {
    path: PathBuf,
    budget: Usd,
    entries: Mutex<BTreeMap<String, Charge>>,
    gate: Option<Arc<dyn PaidGate>>,
}

impl Ledger {
    pub fn open(path: &Path, budget: Usd) -> Result<Self, LedgerError> {
        let entries = match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|err| EngineError::Config {
                path: path.into(),
                message: err.to_string(),
            })?,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(err) => return Err(EngineError::io(path, err).into()),
        };
        Ok(Self {
            path: path.into(),
            budget,
            entries: Mutex::new(entries),
            gate: None,
        })
    }

    /// Every later paid dispatch must also pass this external accounting authority.
    #[must_use]
    pub fn with_gate(mut self, gate: Arc<dyn PaidGate>) -> Self {
        self.gate = Some(gate);
        self
    }

    /// Takes the next free slot for `purchase` and crosses the dispatch boundary. Call it
    /// immediately before network I/O; nothing local may fail between the two.
    pub fn begin_paid(&self, purchase: &Purchase<'_>) -> Result<String, LedgerError> {
        let request = self.next_attempt_id(purchase.logical)?;
        self.hold(&request, purchase.ceiling)?;
        if let Some(gate) = &self.gate {
            self.cross(gate.as_ref(), &purchase.call(&request))?;
        }
        Ok(request)
    }

    fn cross(&self, gate: &dyn PaidGate, call: &PaidCall<'_>) -> Result<(), LedgerError> {
        let request = call.request;
        if let Err(error) = gate.reserve(call) {
            let possibly_dispatched = matches!(error, GateError::Uncertain(_));
            if !possibly_dispatched {
                self.write_charge(request, None)?;
            }
            return Err(error.into());
        }
        if let Err(error) = gate.dispatch(request) {
            // A withdrawn account attempt is never reused: later work takes the next slot.
            let withdrawn = gate.withdraw(request).is_ok();
            if withdrawn {
                self.write_charge(request, Some(Charge::Exact { cost: Usd::ZERO }))?;
            }
            return Err(error.into());
        }
        Ok(())
    }

    fn write_charge(&self, request: &str, charge: Option<Charge>) -> Result<(), LedgerError> {
        let mut entries = self.lock()?;
        match charge {
            Some(charge) => entries.insert(request.into(), charge),
            None => entries.remove(request),
        };
        fsx::write_json_atomic(&self.path, &*entries)?;
        Ok(())
    }

    fn require_ungated(&self) -> Result<(), LedgerError> {
        let gated = self.gate.is_some();
        if gated {
            return Err(EngineError::Setup(
                "account-gated work reserves with begin_paid and settles with artifact evidence"
                    .into(),
            )
            .into());
        }
        Ok(())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, BTreeMap<String, Charge>>, LedgerError> {
        self.entries
            .lock()
            .map_err(|_| EngineError::Setup("billing ledger lock failed".into()).into())
    }

    /// Picks the ledger key for a new attempt of `base`. Any unresolved charge (`Reserved` or
    /// `Conservative`) blocks with [`LedgerError::Uncertain`]; a prior failed attempt that was
    /// reconciled to `Exact` stays in the ledger (counting toward the budget) while the retry
    /// reserves the next slot (`{base}-retry-1`, …).
    pub fn next_attempt_id(&self, base: &str) -> Result<String, LedgerError> {
        let entries = self.lock()?;
        for retry in 0..MAX_RETRIES {
            let candidate = if retry == 0 {
                base.to_owned()
            } else {
                format!("{base}-retry-{retry}")
            };
            match entries.get(&candidate) {
                None | Some(Charge::Released) => return Ok(candidate),
                Some(Charge::Reserved { .. } | Charge::Conservative { .. }) => {
                    return Err(LedgerError::Uncertain(candidate));
                }
                Some(Charge::Exact { .. }) => {}
            }
        }
        Err(LedgerError::Uncertain(base.into()))
    }

    /// Ungated per-run reservation; gated ledgers refuse it so no call bypasses the account.
    pub fn reserve(&self, request: &str, ceiling: Usd) -> Result<(), LedgerError> {
        self.require_ungated()?;
        self.hold(request, ceiling)
    }

    fn hold(&self, request: &str, ceiling: Usd) -> Result<(), LedgerError> {
        if ceiling.micros() < 0 {
            return Err(EngineError::Setup("negative reservation".into()).into());
        }
        let mut entries = self.lock()?;
        let blocked = entries
            .get(request)
            .is_some_and(|charge| !matches!(charge, Charge::Released));
        if blocked {
            return Err(LedgerError::Uncertain(request.into()));
        }
        let used: Usd = entries.values().map(Charge::amount).sum();
        if used + ceiling > self.budget {
            return Err(LedgerError::Budget {
                used,
                requested: ceiling,
                budget: self.budget,
            });
        }
        entries.insert(request.into(), Charge::Reserved { ceiling });
        fsx::write_json_atomic(&self.path, &*entries)?;
        Ok(())
    }

    /// Call only after the output and its response metadata have been committed.
    pub fn settle(
        &self,
        request: &str,
        cost: Option<Usd>,
        generation: Option<String>,
    ) -> Result<(), LedgerError> {
        self.require_ungated()?;
        self.record_settlement(request, cost, generation)
    }

    /// Settles with the committed artifact's hash, which a gated ledger forwards first.
    pub fn settle_with_evidence(
        &self,
        request: &str,
        generation: Option<String>,
        evidence: &PaidEvidence<'_>,
    ) -> Result<(), LedgerError> {
        if let Some(gate) = &self.gate {
            let charge = self.lock()?.get(request).cloned();
            let unresolved = matches!(
                charge,
                Some(Charge::Reserved { .. } | Charge::Conservative { .. })
            );
            if unresolved {
                gate.settle(request, evidence)?;
            }
        }
        self.record_settlement(request, evidence.cost, generation)
    }

    fn record_settlement(
        &self,
        request: &str,
        cost: Option<Usd>,
        generation: Option<String>,
    ) -> Result<(), LedgerError> {
        let mut entries = self.lock()?;
        let original = entries
            .get(request)
            .ok_or_else(|| EngineError::Setup("cannot settle an unreserved request".into()))?;
        if matches!(original, Charge::Exact { .. } | Charge::Released) {
            return Ok(());
        }
        let ceiling = original.amount();
        let negative = cost.is_some_and(|amt| amt.micros() < 0);
        if negative {
            return Err(EngineError::Setup("negative charge".into()).into());
        }
        let charge = cost.map_or_else(
            || Charge::Conservative {
                ceiling,
                generation,
            },
            |exact| Charge::Exact { cost: exact },
        );
        entries.insert(request.into(), charge);
        fsx::write_json_atomic(&self.path, &*entries)?;
        let used: Usd = entries.values().map(Charge::amount).sum();
        if used > self.budget {
            return Err(LedgerError::Budget {
                used,
                requested: Usd::ZERO,
                budget: self.budget,
            });
        }
        Ok(())
    }

    /// Explicit operator reconciliation (by full id, unique prefix, or generation id), or a
    /// provider response known to be unbilled.
    pub fn reconcile(&self, request: &str, cost: Usd) -> Result<(), LedgerError> {
        if cost.micros() < 0 {
            return Err(EngineError::Setup("negative reconciliation".into()).into());
        }
        let mut entries = self.lock()?;
        let resolved = resolve_request_key(&entries, request)?;
        if let Some(gate) = &self.gate {
            gate.reconcile(&resolved, cost)?;
        }
        // A gated slot is never reused: its account attempt keeps its own history.
        let reusable = cost == Usd::ZERO && self.gate.is_none();
        entries.insert(
            resolved.clone(),
            if reusable {
                Charge::Released
            } else {
                Charge::Exact { cost }
            },
        );
        fsx::write_json_atomic(&self.path, &*entries)?;
        clear_billing_error(&self.path, &resolved);
        Ok(())
    }

    pub fn record_failure(&self, request: &str, error: &str) -> Result<(), LedgerError> {
        let _guard = self.lock()?;
        let path = self.path.with_file_name("billing-errors.json");
        let mut errors: BTreeMap<String, String> = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|err| EngineError::Config {
                path: path.clone(),
                message: err.to_string(),
            })?,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(err) => return Err(EngineError::io(&path, err).into()),
        };
        errors.insert(request.into(), error.into());
        fsx::write_json_atomic(&path, &errors)?;
        Ok(())
    }
    pub fn total(&self) -> Result<Usd, LedgerError> {
        Ok(self.lock()?.values().map(Charge::amount).sum())
    }
    pub fn entries(&self) -> Result<BTreeMap<String, Charge>, LedgerError> {
        Ok(self.lock()?.clone())
    }
}

fn clear_billing_error(ledger_path: &Path, resolved: &str) {
    let errors_path = ledger_path.with_file_name("billing-errors.json");
    let Ok(bytes) = std::fs::read(&errors_path) else {
        return;
    };
    let Ok(mut errors) = serde_json::from_slice::<BTreeMap<String, String>>(&bytes) else {
        return;
    };
    if errors.remove(resolved).is_some() {
        let _ = fsx::write_json_atomic(&errors_path, &errors);
    }
}

fn resolve_request_key(
    entries: &BTreeMap<String, Charge>,
    query: &str,
) -> Result<String, EngineError> {
    let exact_match = entries.contains_key(query);
    if exact_match {
        return Ok(query.to_owned());
    }
    let matches: Vec<String> = entries
        .iter()
        .filter(|(key, charge)| {
            let non_empty = !query.is_empty();
            let prefix_match = key.starts_with(query);
            let by_prefix = non_empty && prefix_match;
            let by_gen = match charge {
                Charge::Conservative {
                    generation: Some(gen_id),
                    ..
                } => gen_id == query,
                _ => false,
            };
            by_prefix || by_gen
        })
        .map(|(key, _)| key.clone())
        .collect();
    match matches.as_slice() {
        [only] => Ok(only.clone()),
        [] => Err(EngineError::Setup("unknown paid request".into())),
        _ => Err(EngineError::Setup(
            "request prefix matches multiple charges; pass more characters".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_unknown_cost_cannot_overwrite_confirmed_reconciliation() {
        let tmp = tempfile::tempdir().unwrap();
        let ledger = Ledger::open(&tmp.path().join("ledger.json"), Usd::from_micros(100)).unwrap();
        ledger.reserve("a", Usd::from_micros(70)).unwrap();
        ledger.reconcile("a", Usd::from_micros(10)).unwrap();
        ledger
            .settle("a", None, Some("old-generation".into()))
            .unwrap();
        assert_eq!(ledger.total().unwrap(), Usd::from_micros(10));
        ledger.reconcile("a", Usd::ZERO).unwrap();
        ledger
            .settle("a", Some(Usd::from_micros(70)), None)
            .unwrap();
        assert_eq!(ledger.total().unwrap(), Usd::ZERO);
    }
    #[test]
    fn holds_survive_restart_and_exact_reconciliation_releases_only_the_difference() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("ledger.json");
        let ledger = Ledger::open(&path, Usd::from_micros(100)).unwrap();
        ledger.reserve("a", Usd::from_micros(70)).unwrap();
        assert!(matches!(
            ledger.reserve("b", Usd::from_micros(31)),
            Err(LedgerError::Budget { .. })
        ));
        drop(ledger);
        let ledger = Ledger::open(&path, Usd::from_micros(100)).unwrap();
        assert!(matches!(
            ledger.reserve("a", Usd::from_micros(70)),
            Err(LedgerError::Uncertain(_))
        ));
        ledger
            .settle("a", Some(Usd::from_micros(40)), None)
            .unwrap();
        ledger.reserve("b", Usd::from_micros(60)).unwrap();
        assert_eq!(ledger.total().unwrap(), Usd::from_micros(100));
    }
    #[test]
    fn missing_cost_keeps_the_full_quote_instead_of_becoming_free() {
        let tmp = tempfile::tempdir().unwrap();
        let ledger = Ledger::open(&tmp.path().join("ledger.json"), Usd::from_micros(100)).unwrap();
        ledger.reserve("a", Usd::from_micros(70)).unwrap();
        ledger.settle("a", None, Some("generation".into())).unwrap();
        assert_eq!(ledger.total().unwrap(), Usd::from_micros(70));
        ledger.reconcile("gen", Usd::from_micros(10)).unwrap_err();
        ledger
            .reconcile("generation", Usd::from_micros(10))
            .unwrap();
        assert_eq!(ledger.total().unwrap(), Usd::from_micros(10));
    }
    #[test]
    fn reconciled_failed_attempt_allows_retry_in_next_slot_while_keeping_spend() {
        let tmp = tempfile::tempdir().unwrap();
        let ledger = Ledger::open(&tmp.path().join("ledger.json"), Usd::from_micros(100)).unwrap();
        let first = ledger.next_attempt_id("req123456").unwrap();
        assert_eq!(first, "req123456");
        ledger.reserve(&first, Usd::from_micros(60)).unwrap();
        assert!(matches!(
            ledger.next_attempt_id("req123456"),
            Err(LedgerError::Uncertain(_))
        ));
        ledger.reconcile("req123", Usd::from_micros(25)).unwrap();
        let second = ledger.next_attempt_id("req123456").unwrap();
        assert_eq!(second, "req123456-retry-1");
        ledger.reserve(&second, Usd::from_micros(60)).unwrap();
        assert_eq!(ledger.total().unwrap(), Usd::from_micros(85));
    }
}
