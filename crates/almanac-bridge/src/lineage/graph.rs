//! Derive consumer→producer edges from contracts.
//!
//! Used by the Phase 3 web view (DAG of schedules → contracts → manifests).
//! The ICS feed does not need this — it gets its [`Dependency`] list from
//! [`crate::lineage::check_inputs`].

use crate::model::{Contract, ContractRole, Schedule};

/// A directed edge: `consumer` schedule depends on `producer` schedule for `schema_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    pub consumer: String,
    pub producer: String,
    pub schema_id: String,
}

/// Derive the producer↔consumer edge list from a set of schedules + contracts.
///
/// For each `schema_id`, every consumer schedule is connected to every
/// producer schedule. Real deployments usually have exactly one producer per
/// schema; the N:N form handles the general case.
pub fn derive_edges(schedules: &[Schedule], contracts: &[Contract]) -> Vec<Edge> {
    use std::collections::HashMap;
    let mut producers: HashMap<String, Vec<String>> = HashMap::new();
    let mut consumers: HashMap<String, Vec<String>> = HashMap::new();

    for c in contracts {
        match c.role {
            ContractRole::Produce => producers
                .entry(c.schema_id.clone())
                .or_default()
                .push(c.schedule_id.clone()),
            ContractRole::Consume => consumers
                .entry(c.schema_id.clone())
                .or_default()
                .push(c.schedule_id.clone()),
        }
    }

    let mut edges = Vec::new();
    for (schema, consumer_ids) in &consumers {
        if let Some(producer_ids) = producers.get(schema) {
            for consumer in consumer_ids {
                for producer in producer_ids {
                    edges.push(Edge {
                        consumer: consumer.clone(),
                        producer: producer.clone(),
                        schema_id: schema.clone(),
                    });
                }
            }
        }
    }

    // Sort for deterministic output (useful in tests + web view).
    edges.sort_by(|a, b| {
        a.schema_id
            .cmp(&b.schema_id)
            .then(a.consumer.cmp(&b.consumer))
            .then(a.producer.cmp(&b.producer))
    });
    let _ = schedules; // schedules are context for the web view; not needed for edges.
    edges
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Contract, ContractRole};

    fn contract(id: &str, sched: &str, role: ContractRole, schema: &str) -> Contract {
        Contract {
            contract_id: id.into(),
            schedule_id: sched.into(),
            role,
            schema_id: schema.into(),
            min_version: 1,
            any_version: false,
            freshness_window: 86_400,
        }
    }

    #[test]
    fn one_producer_one_consumer_one_edge() {
        let contracts = vec![
            contract("p", "daily-brief", ContractRole::Produce, "research-brief"),
            contract("c", "weekly-strat", ContractRole::Consume, "research-brief"),
        ];
        let edges = derive_edges(&[], &contracts);
        assert_eq!(
            edges,
            vec![Edge {
                consumer: "weekly-strat".into(),
                producer: "daily-brief".into(),
                schema_id: "research-brief".into()
            }]
        );
    }

    #[test]
    fn no_edge_when_no_producer() {
        let contracts = vec![contract("c", "weekly-strat", ContractRole::Consume, "x")];
        let edges = derive_edges(&[], &contracts);
        assert!(edges.is_empty());
    }

    #[test]
    fn n_to_n_matching() {
        let contracts = vec![
            contract("p1", "prod1", ContractRole::Produce, "s"),
            contract("p2", "prod2", ContractRole::Produce, "s"),
            contract("c1", "cons1", ContractRole::Consume, "s"),
            contract("c2", "cons2", ContractRole::Consume, "s"),
        ];
        let edges = derive_edges(&[], &contracts);
        // 2 producers × 2 consumers = 4 edges
        assert_eq!(edges.len(), 4);
    }
}
