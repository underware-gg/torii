//! Periodic sweep and reporting of subscription state.
//!
//! Every [`SWEEP_INTERVAL`] this task prunes subscribers whose client has hung up, then publishes
//! the live counts as gauges so a leak shows up as a number that only ever climbs:
//!
//! - `torii_grpc_subscribers{kind}`: registered gRPC subscribers per subscription kind.
//! - `torii_broker_subscribers{kind}`: streams attached to the in-memory broker per update type.
//!   Each gRPC dispatch service holds exactly one, and every GraphQL subscription holds its own,
//!   so anything above one per kind while no GraphQL client is connected is a leaked stream.
//! - `torii_grpc_subscribers_dropped_total{kind,reason}`: subscribers removed by dispatch
//!   (`full`, `closed`) or by this sweep (`pruned`).

use std::sync::Arc;
use std::time::Duration;

use metrics::{describe_counter, describe_gauge, gauge};
use tokio::time::MissedTickBehavior;
use torii_broker::types::{
    AchievementProgressionUpdate, ActivityUpdate, AggregationUpdate, ContractUpdate, EntityUpdate,
    EventMessageUpdate, EventUpdate, ModelUpdate, TokenBalanceUpdate, TokenTransferUpdate,
    TokenUpdate, TransactionUpdate,
};
use torii_broker::MemoryBroker;
use tracing::{debug, info};

use super::SubscriberBookkeeping;

pub(crate) const LOG_TARGET: &str = "torii::grpc::server::subscriptions::monitor";

/// How often closed subscribers are swept and gauges refreshed.
pub const SWEEP_INTERVAL: Duration = Duration::from_secs(30);

/// Live stream counts on the in-memory broker, one entry per update type.
pub fn broker_subscriber_counts() -> [(&'static str, usize); 12] {
    [
        ("entity", MemoryBroker::<EntityUpdate>::subscriber_count()),
        (
            "event_message",
            MemoryBroker::<EventMessageUpdate>::subscriber_count(),
        ),
        (
            "contract",
            MemoryBroker::<ContractUpdate>::subscriber_count(),
        ),
        ("model", MemoryBroker::<ModelUpdate>::subscriber_count()),
        ("token", MemoryBroker::<TokenUpdate>::subscriber_count()),
        (
            "token_balance",
            MemoryBroker::<TokenBalanceUpdate>::subscriber_count(),
        ),
        (
            "token_transfer",
            MemoryBroker::<TokenTransferUpdate>::subscriber_count(),
        ),
        ("event", MemoryBroker::<EventUpdate>::subscriber_count()),
        (
            "transaction",
            MemoryBroker::<TransactionUpdate>::subscriber_count(),
        ),
        (
            "aggregation",
            MemoryBroker::<AggregationUpdate>::subscriber_count(),
        ),
        (
            "activity",
            MemoryBroker::<ActivityUpdate>::subscriber_count(),
        ),
        (
            "achievement_progression",
            MemoryBroker::<AchievementProgressionUpdate>::subscriber_count(),
        ),
    ]
}

/// One sweep: prune closed subscribers, refresh gauges, and return
/// `(grpc subscribers, broker streams, pruned)` totals.
pub fn sweep(managers: &[Arc<dyn SubscriberBookkeeping>]) -> (usize, usize, usize) {
    let mut grpc_total = 0;
    let mut pruned_total = 0;
    for manager in managers {
        let pruned = manager.prune_closed();
        let count = manager.subscriber_count();
        gauge!("torii_grpc_subscribers", "kind" => manager.kind()).set(count as f64);
        if pruned > 0 {
            debug!(target: LOG_TARGET, kind = manager.kind(), pruned, remaining = count, "Pruned closed subscribers.");
        }
        grpc_total += count;
        pruned_total += pruned;
    }

    let mut broker_total = 0;
    for (kind, count) in broker_subscriber_counts() {
        gauge!("torii_broker_subscribers", "kind" => kind).set(count as f64);
        broker_total += count;
    }

    (grpc_total, broker_total, pruned_total)
}

/// Run the sweep forever. Spawn this once per gRPC server.
pub async fn run(managers: Vec<Arc<dyn SubscriberBookkeeping>>) {
    describe_gauge!(
        "torii_grpc_subscribers",
        "Registered gRPC subscribers per subscription kind."
    );
    describe_gauge!(
        "torii_broker_subscribers",
        "Streams attached to the in-memory broker per update type (one per gRPC dispatch service plus one per GraphQL subscription)."
    );
    describe_counter!(
        "torii_grpc_subscribers_dropped_total",
        "gRPC subscribers removed, by kind and reason (full, closed, pruned)."
    );

    let mut ticker = tokio::time::interval(SWEEP_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut last = (usize::MAX, usize::MAX);

    loop {
        ticker.tick().await;
        let (grpc_subscribers, broker_streams, pruned) = sweep(&managers);
        let current = (grpc_subscribers, broker_streams);
        if pruned > 0 || current != last {
            info!(target: LOG_TARGET, grpc_subscribers, broker_streams, pruned, "Subscription sweep.");
        } else {
            debug!(target: LOG_TARGET, grpc_subscribers, broker_streams, "Subscription sweep.");
        }
        last = current;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subscriptions::entity::EntityManager;
    use crate::GrpcConfig;

    #[tokio::test]
    async fn prune_removes_only_subscribers_whose_receiver_is_gone() {
        let manager = Arc::new(EntityManager::new(GrpcConfig::default()));

        let live = manager.add_subscriber(None, vec![]).await;
        let dead = manager.add_subscriber(None, vec![]).await;
        assert_eq!(manager.subscriber_count(), 2);

        // Nothing to prune while both clients are attached.
        assert_eq!(manager.prune_closed(), 0);
        assert_eq!(manager.subscriber_count(), 2);

        drop(dead);
        assert_eq!(manager.prune_closed(), 1);
        assert_eq!(manager.subscriber_count(), 1);

        drop(live);
        let managers: Vec<Arc<dyn SubscriberBookkeeping>> = vec![manager.clone()];
        let (grpc_subscribers, _, pruned) = sweep(&managers);
        assert_eq!(pruned, 1);
        assert_eq!(grpc_subscribers, 0);
        assert_eq!(manager.subscriber_count(), 0);
    }
}
