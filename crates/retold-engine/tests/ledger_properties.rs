//! Randomised sequences of reserve / settle / reconcile against a simple model of the ledger.
//! The ledger is the money record, so these properties must hold for every interleaving:
//! totals never go negative, a new reservation never pushes spend past the budget, an
//! unresolved hold can never be dispatched twice, and everything survives a restart.

use std::collections::BTreeMap;

use proptest::prelude::*;
use retold_core::Usd;
use retold_engine::ledger::{Charge, Ledger, LedgerError};

const BUDGET: i64 = 1_000;

#[derive(Debug, Clone)]
enum LedgerAction {
    Reserve(u8, i64),
    Settle(u8, Option<i64>),
    Reconcile(u8, i64),
    Reopen,
}

fn ledger_action() -> impl Strategy<Value = LedgerAction> {
    let id = 0_u8..6;
    prop_oneof![
        (id.clone(), 0_i64..600).prop_map(|(i, c)| LedgerAction::Reserve(i, c)),
        (id.clone(), proptest::option::of(0_i64..600))
            .prop_map(|(i, c)| LedgerAction::Settle(i, c)),
        (id, 0_i64..300).prop_map(|(i, c)| LedgerAction::Reconcile(i, c)),
        Just(LedgerAction::Reopen),
    ]
}

fn sum(entries: &BTreeMap<String, Charge>) -> i64 {
    entries.values().map(|c| c.amount().micros()).sum()
}

proptest! {
    // Every operation fsyncs the ledger; 48 cases of up to 24 steps stay fast on macOS.
    #![proptest_config(ProptestConfig::with_cases(48))]
    #[test]
    fn ledger_invariants_hold_for_any_sequence(ops in proptest::collection::vec(ledger_action(), 1..24)) {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("ledger.json");
        let budget = Usd::from_micros(BUDGET);
        let mut ledger = Ledger::open(&path, budget).unwrap();
        for op in ops {
            let before = ledger.entries().unwrap();
            match op {
                LedgerAction::Reserve(id, ceiling) => {
                    let key = format!("r{id}");
                    let held = before.get(&key).is_some_and(|c| !matches!(c, Charge::Released));
                    match ledger.reserve(&key, Usd::from_micros(ceiling)) {
                        Ok(()) => {
                            prop_assert!(!held, "an unresolved hold was dispatched twice");
                            prop_assert!(ledger.total().unwrap().micros() <= BUDGET);
                        }
                        Err(LedgerError::Uncertain(_)) => prop_assert!(held),
                        Err(LedgerError::Budget { .. }) => {
                            prop_assert!(sum(&before) + ceiling > BUDGET);
                            prop_assert_eq!(ledger.entries().unwrap().len(), before.len());
                        }
                        Err(other) => prop_assert!(false, "unexpected {other}"),
                    }
                }
                LedgerAction::Settle(id, cost) => {
                    let _ = ledger.settle(&format!("r{id}"), cost.map(Usd::from_micros), None);
                }
                LedgerAction::Reconcile(id, cost) => {
                    let _ = ledger.reconcile(&format!("r{id}"), Usd::from_micros(cost));
                }
                LedgerAction::Reopen => {
                    drop(ledger);
                    ledger = Ledger::open(&path, budget).unwrap();
                    prop_assert_eq!(sum(&ledger.entries().unwrap()), sum(&before));
                }
            }
            let entries = ledger.entries().unwrap();
            prop_assert!(sum(&entries) >= 0);
            prop_assert_eq!(ledger.total().unwrap().micros(), sum(&entries));
            // Settling never forgets a request, and only reconcile may release one.
            for key in before.keys() {
                prop_assert!(entries.contains_key(key));
            }
        }
    }
}
