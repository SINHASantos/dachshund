/*
 * Copyright (c) Facebook, Inc. and its affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */
extern crate fxhash;

use crate::dachshund::algorithms::connected_components::ConnectedComponents;
use crate::dachshund::graph_base::GraphBase;
use crate::dachshund::id_types::NodeId;
use crate::dachshund::node::{NodeBase, NodeEdgeBase};
use core::cmp::Reverse;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::iter::FromIterator;

use fxhash::{FxHashSet};

type OrderedNodeSet = BTreeSet<NodeId>;
type OrderedEdgeSet = BTreeSet<(NodeId, NodeId)>;

pub trait Coreness: GraphBase + ConnectedComponents {
    fn _get_k_cores(&self, k: usize, removed: &mut FxHashSet<NodeId>) -> Vec<Vec<NodeId>> {
        let mut queue: OrderedNodeSet = self.get_ids_iter().cloned().collect();
        let mut num_neighbors: HashMap<NodeId, usize> = self
            .get_nodes_iter()
            .map(|x| (x.get_id(), x.degree()))
            .collect();
        // iteratively delete all nodes w/ degree less than k
        while !queue.is_empty() {
            let id = queue.pop_first().unwrap();
            // this assumes no multiple connections to neighbors
            if num_neighbors[&id] < k {
                removed.insert(id);
                for e in self.get_node(id).get_edges() {
                    let nid = e.get_neighbor_id();
                    if !removed.contains(&nid) {
                        queue.insert(nid);
                        *num_neighbors.get_mut(&id).unwrap() -= 1;
                        *num_neighbors.get_mut(&nid).unwrap() -= 1;
                    }
                }
            }
        }
        self._get_connected_components(Some(removed), None)
    }

    fn get_k_cores(&self, k: usize) -> Vec<Vec<NodeId>> {
        let mut removed: FxHashSet<NodeId> = FxHashSet::default();
        self._get_k_cores(k, &mut removed)
    }

    fn get_coreness(&self) -> (Vec<Vec<Vec<NodeId>>>, HashMap<NodeId, usize>) {
        let mut core_assignments: Vec<Vec<Vec<NodeId>>> = Vec::new();
        let mut removed: FxHashSet<NodeId> = FxHashSet::default();
        let mut k: usize = 0;
        while removed.len() < self.count_nodes() {
            k += 1;
            core_assignments.push(self._get_k_cores(k, &mut removed))
        }
        let mut coreness: HashMap<NodeId, usize> = HashMap::new();
        for i in (0..k).rev() {
            for ids in &core_assignments[i] {
                for id in ids {
                    if !coreness.contains_key(id) {
                        coreness.insert(*id, i + 1);
                    }
                }
            }
        }
        (core_assignments, coreness)
    }

    fn get_coreness_anomaly(&self, coreness: &HashMap<NodeId, usize>) -> HashMap<NodeId, f64> {
        // Calculate the coreness anomaly score of all nodes as the absolute
        // value of the difference between the logs of the ranks by
        // degree and coreness.

        // See algorithm Core-A in https://www.cs.cmu.edu/~kijungs/papers/kcoreICDM2016.pdf
        let mut anomaly_scores = HashMap::new();
        let core_ranks = self._averaged_ties_ranking(&coreness);
        let deg_ranks = self._averaged_ties_ranking(
            &self
                .get_nodes_iter()
                .map(|x| (x.get_id(), x.degree()))
                .collect(),
        );
        for node in self.get_ordered_node_ids() {
            anomaly_scores.insert(node, (core_ranks[&node].ln() - deg_ranks[&node].ln()).abs());
        }
        anomaly_scores
    }

    fn _averaged_ties_ranking(&self, scores: &HashMap<NodeId, usize>) -> HashMap<NodeId, f64> {
        // [TODO] Needs to handle ties correctly.
        let mut ranking = HashMap::new();
        let mut sorted_nodes: Vec<(&NodeId, &usize)> = scores.into_iter().collect();
        sorted_nodes.sort_unstable_by_key(|(_node, value)| Reverse(*value));
        let mut i = 1;
        for (&node, _value) in sorted_nodes.into_iter() {
            ranking.insert(node, i as f64);
            i += 1;
        }
        ranking
    }

    fn _get_k_trusses(
        &self,
        k: usize,
        ignore_nodes: &FxHashSet<NodeId>,
    ) -> (Vec<OrderedEdgeSet>, HashSet<OrderedNodeSet>) {
        let mut neighbors: HashMap<NodeId, HashSet<NodeId>> = HashMap::new();
        let mut edges: OrderedEdgeSet = BTreeSet::new();
        for node in self.get_nodes_iter() {
            // [TODO] This step is unncessary now.
            neighbors.insert(
                node.get_id(),
                HashSet::from_iter(
                    node.get_edges()
                        .map(|x| x.get_neighbor_id())
                        .filter(|x| !ignore_nodes.contains(x)),
                ),
            );
            for e in node.get_edges() {
                let id_pair: (NodeId, NodeId);
                let node_id = node.get_id();
                let neighbor_id = e.get_neighbor_id();
                if node_id < neighbor_id {
                    id_pair = (node_id, neighbor_id);
                } else {
                    id_pair = (neighbor_id, node_id);
                }
                edges.insert(id_pair);
            }
        }
        let mut changes = true;
        let mut ignore_edges: HashSet<(NodeId, NodeId)> = HashSet::new();
        while changes {
            changes = false;
            let mut to_remove: Vec<(NodeId, NodeId)> = Vec::new();
            for (id1, id2) in &edges {
                let n1 = &neighbors[&id1];
                let n2 = &neighbors[&id2];
                let intersection = n1.intersection(n2);
                if intersection.count() < k - 2 {
                    to_remove.push((*id1, *id2));
                    neighbors.get_mut(id1).unwrap().remove(id2);
                    neighbors.get_mut(id2).unwrap().remove(id1);
                }
            }
            for e in &to_remove {
                changes = true;
                edges.remove(&e);
                ignore_edges.insert(*e);
            }
        }
        let (components, num_components) =
            self._get_connected_components_membership(None, Some(&ignore_edges));
        let mut trusses: Vec<OrderedEdgeSet> = vec![BTreeSet::new(); num_components];
        for (id, idx) in &components {
            // reusing the neighbors sets from above
            for nid in &neighbors[&id] {
                // will only return (lesser_id, greater_id) for an UndirectedGraph
                if components[nid] == *idx && id < nid {
                    let eid = (*id, *nid);
                    if !ignore_edges.contains(&eid) && edges.contains(&eid) {
                        trusses[*idx].insert(eid);
                    }
                }
            }
        }
        let filtered_trusses: Vec<OrderedEdgeSet> =
            trusses.into_iter().filter(|x| !x.is_empty()).collect();
        let truss_nodes = filtered_trusses
            .iter()
            .map(|y| BTreeSet::from_iter(y.iter().map(|x| x.0).chain(y.iter().map(|x| x.1))))
            .collect::<HashSet<OrderedNodeSet>>();
        (filtered_trusses, truss_nodes)
    }
    fn get_k_trusses(&self, k: usize) -> (Vec<OrderedEdgeSet>, HashSet<OrderedNodeSet>) {
        // Basic algorithm: https://louridas.github.io/rwa/assignments/finding-trusses/

        // ignore_nodes will contain all the irrelevant nodes after
        // calling self._get_k_cores();
        let mut ignore_nodes: FxHashSet<NodeId> = FxHashSet::default();
        // this really only works for an undirected graph
        self._get_k_cores(k - 1, &mut ignore_nodes);
        self._get_k_trusses(k, &ignore_nodes)
    }
}
