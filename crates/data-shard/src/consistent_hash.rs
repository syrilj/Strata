//! Consistent hashing implementation for shard distribution
//!
//! Uses virtual nodes to ensure even distribution and minimize data movement
//! when workers join or leave the cluster.

use fnv::FnvHasher;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

/// Number of virtual nodes per physical node for better distribution
const DEFAULT_VIRTUAL_NODES: usize = 150;

/// Consistent hash ring for distributing shards across workers
#[derive(Debug)]
pub struct ConsistentHash {
    /// Ring mapping hash values to node IDs
    ring: RwLock<BTreeMap<u64, String>>,

    /// Number of virtual nodes per physical node
    virtual_nodes: usize,

    /// Track physical nodes for management
    nodes: RwLock<Vec<String>>,
}

impl Default for ConsistentHash {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsistentHash {
    /// Create a new consistent hash ring with default virtual nodes
    pub fn new() -> Self {
        Self::with_virtual_nodes(DEFAULT_VIRTUAL_NODES)
    }

    /// Create a new consistent hash ring with specified virtual nodes
    pub fn with_virtual_nodes(virtual_nodes: usize) -> Self {
        Self {
            ring: RwLock::new(BTreeMap::new()),
            virtual_nodes,
            nodes: RwLock::new(Vec::new()),
        }
    }

    /// Add a node to the hash ring
    pub fn add_node(&self, node_id: &str) {
        let mut ring = self.ring.write();
        let mut nodes = self.nodes.write();

        if nodes.contains(&node_id.to_string()) {
            return; // Node already exists
        }

        // Add virtual nodes
        for i in 0..self.virtual_nodes {
            let virtual_key = format!("{}:{}", node_id, i);
            let hash = self.hash(&virtual_key);
            ring.insert(hash, node_id.to_string());
        }

        nodes.push(node_id.to_string());
        tracing::debug!(node = node_id, virtual_nodes = self.virtual_nodes, "Added node to hash ring");
    }

    /// Remove a node from the hash ring
    pub fn remove_node(&self, node_id: &str) {
        let mut ring = self.ring.write();
        let mut nodes = self.nodes.write();

        // Remove virtual nodes
        for i in 0..self.virtual_nodes {
            let virtual_key = format!("{}:{}", node_id, i);
            let hash = self.hash(&virtual_key);
            ring.remove(&hash);
        }

        nodes.retain(|n| n != node_id);
        tracing::debug!(node = node_id, "Removed node from hash ring");
    }

    /// Get the node responsible for a given key
    pub fn get_node(&self, key: &str) -> Option<String> {
        let ring = self.ring.read();

        if ring.is_empty() {
            return None;
        }

        let hash = self.hash(key);

        // Find the first node with hash >= key hash (clockwise search)
        match ring.range(hash..).next() {
            Some((_, node)) => Some(node.clone()),
            // Wrap around to the beginning of the ring
            None => ring.values().next().cloned(),
        }
    }

    /// Get the node responsible for a shard ID
    pub fn get_node_for_shard(&self, dataset_id: &str, shard_id: u64) -> Option<String> {
        let key = format!("{}:{}", dataset_id, shard_id);
        self.get_node(&key)
    }

    /// Get all shards assigned to a specific node
    pub fn get_shards_for_node(&self, node_id: &str, dataset_id: &str, total_shards: u64) -> Vec<u64> {
        (0..total_shards)
            .filter(|&shard_id| {
                self.get_node_for_shard(dataset_id, shard_id)
                    .as_deref() == Some(node_id)
            })
            .collect()
    }

    /// Get the number of nodes in the ring
    pub fn node_count(&self) -> usize {
        self.nodes.read().len()
    }

    /// Get all node IDs
    pub fn nodes(&self) -> Vec<String> {
        self.nodes.read().clone()
    }

    /// Check if a node exists in the ring
    pub fn contains_node(&self, node_id: &str) -> bool {
        self.nodes.read().contains(&node_id.to_string())
    }

    /// Clear all nodes from the ring
    pub fn clear(&self) {
        self.ring.write().clear();
        self.nodes.write().clear();
    }

    /// Compute hash using FNV for speed
    fn hash<T: Hash + ?Sized>(&self, key: &T) -> u64 {
        let mut hasher = FnvHasher::default();
        key.hash(&mut hasher);
        hasher.finish()
    }
}

/// Snapshot of hash ring state for serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsistentHashState {
    /// List of node IDs in the ring
    pub nodes: Vec<String>,

    /// Number of virtual nodes per physical node
    pub virtual_nodes: usize,
}

impl From<&ConsistentHash> for ConsistentHashState {
    fn from(hash: &ConsistentHash) -> Self {
        Self {
            nodes: hash.nodes(),
            virtual_nodes: hash.virtual_nodes,
        }
    }
}

impl From<ConsistentHashState> for ConsistentHash {
    fn from(state: ConsistentHashState) -> Self {
        let hash = ConsistentHash::with_virtual_nodes(state.virtual_nodes);
        for node in state.nodes {
            hash.add_node(&node);
        }
        hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_get_node() {
        let ring = ConsistentHash::new();
        ring.add_node("worker-1");
        ring.add_node("worker-2");
        ring.add_node("worker-3");

        assert_eq!(ring.node_count(), 3);

        // Should always return a node
        let node = ring.get_node("some-key").unwrap();
        assert!(["worker-1", "worker-2", "worker-3"].contains(&node.as_str()));
    }

    #[test]
    fn test_consistent_mapping() {
        let ring = ConsistentHash::new();
        ring.add_node("worker-1");
        ring.add_node("worker-2");

        // Same key should always map to same node
        let key = "dataset-1:shard-42";
        let node1 = ring.get_node(key).unwrap();
        let node2 = ring.get_node(key).unwrap();
        assert_eq!(node1, node2);
    }

    #[test]
    fn test_remove_node_minimal_movement() {
        let ring = ConsistentHash::new();
        ring.add_node("worker-1");
        ring.add_node("worker-2");
        ring.add_node("worker-3");

        // Record assignments before removal
        let assignments_before: Vec<_> = (0..100)
            .map(|i| ring.get_node(&format!("key-{}", i)).unwrap())
            .collect();

        // Remove one node
        ring.remove_node("worker-2");
        assert_eq!(ring.node_count(), 2);

        // Count how many keys stayed with their original node
        let assignments_after: Vec<_> = (0..100)
            .map(|i| ring.get_node(&format!("key-{}", i)).unwrap())
            .collect();

        let unchanged = assignments_before
            .iter()
            .zip(assignments_after.iter())
            .filter(|(before, after)| {
                // Keys originally on worker-2 will move, others should stay
                *before != "worker-2" && before == after
            })
            .count();

        // Most keys not on the removed node should stay put
        let originally_on_others = assignments_before
            .iter()
            .filter(|n| *n != "worker-2")
            .count();
        
        let retention_rate = unchanged as f64 / originally_on_others as f64;
        assert!(
            retention_rate > 0.8,
            "Retention rate should be > 80%, got {}%",
            retention_rate * 100.0
        );
    }

    #[test]
    fn test_distribution_evenness() {
        let ring = ConsistentHash::new();
        ring.add_node("worker-1");
        ring.add_node("worker-2");
        ring.add_node("worker-3");

        let mut counts = std::collections::HashMap::new();
        let num_shards = 1000;

        for i in 0..num_shards {
            let node = ring.get_node_for_shard("dataset-1", i).unwrap();
            *counts.entry(node).or_insert(0) += 1;
        }

        // Each worker should get roughly 1/3 of shards
        // Note: Consistent hashing has variance; we use 50% tolerance
        let expected = num_shards / 3;
        let tolerance = expected / 2; // 50% tolerance

        for (node, count) in counts {
            assert!(
                (count as i64 - expected as i64).unsigned_abs() < tolerance as u64,
                "Node {} got {} shards, expected ~{} (±{})",
                node,
                count,
                expected,
                tolerance
            );
        }
    }

    #[test]
    fn test_get_shards_for_node() {
        let ring = ConsistentHash::new();
        ring.add_node("worker-1");
        ring.add_node("worker-2");

        let total_shards = 100;
        let shards_for_w1 = ring.get_shards_for_node("worker-1", "dataset-1", total_shards);
        let shards_for_w2 = ring.get_shards_for_node("worker-2", "dataset-1", total_shards);

        // All shards should be assigned to one of the workers
        assert_eq!(
            shards_for_w1.len() + shards_for_w2.len(),
            total_shards as usize
        );

        // No overlap
        for shard in &shards_for_w1 {
            assert!(!shards_for_w2.contains(shard));
        }
    }

    #[test]
    fn test_state_serialization() {
        let ring = ConsistentHash::new();
        ring.add_node("worker-1");
        ring.add_node("worker-2");

        let state = ConsistentHashState::from(&ring);
        let json = serde_json::to_string(&state).unwrap();
        let restored_state: ConsistentHashState = serde_json::from_str(&json).unwrap();
        let restored_ring = ConsistentHash::from(restored_state);

        // Should have same nodes
        assert_eq!(ring.node_count(), restored_ring.node_count());

        // Should produce same mappings
        let key = "dataset-1:shard-42";
        assert_eq!(ring.get_node(key), restored_ring.get_node(key));
    }

    #[test]
    fn test_empty_ring() {
        let ring = ConsistentHash::new();
        assert_eq!(ring.get_node("any-key"), None);
        assert_eq!(ring.node_count(), 0);
    }

    #[test]
    fn test_duplicate_node_add() {
        let ring = ConsistentHash::new();
        ring.add_node("worker-1");
        ring.add_node("worker-1"); // Duplicate

        assert_eq!(ring.node_count(), 1);
    }
}
