/*
 * Copyright (c) Facebook, Inc. and its affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */
use std::cmp::{Eq, PartialEq};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::hash::{Hash, Hasher};

use fxhash::FxHashSet;
use roaring::RoaringBitmap;

use crate::dachshund::error::{CLQError, CLQResult};
use crate::dachshund::id_types::{EdgeTypeId, NodeId, NodeTypeId};

/// Used to indicate a typed edge leading to the neighbor of a node.
pub trait NodeEdgeBase
where
    Self: Sized,
{
    type NodeIdType;
    fn get_neighbor_id(&self) -> Self::NodeIdType;
}

pub struct NodeEdge {
    pub edge_type: EdgeTypeId,
    pub target_id: u32,
}
impl NodeEdgeBase for NodeEdge {
    type NodeIdType = u32;
    fn get_neighbor_id(&self) -> u32 {
        self.target_id
    }
}
impl NodeEdge {
    pub fn new(edge_type: EdgeTypeId, target_id: u32) -> Self {
        Self {
            edge_type,
            target_id,
        }
    }
}

impl NodeEdgeBase for NodeId {
    type NodeIdType = NodeId;
    fn get_neighbor_id(&self) -> NodeId {
        *self
    }
}

/// Used to indicate a weighted edge leading to the neighbor of a node.
pub struct WeightedNodeEdge {
    pub target_id: NodeId,
    pub weight: f64,
}
impl NodeEdgeBase for WeightedNodeEdge {
    type NodeIdType = NodeId;
    fn get_neighbor_id(&self) -> NodeId {
        self.target_id
    }
}
pub trait WeightedNodeEdgeBase
where
    Self: Sized,
{
    fn get_weight(&self) -> f64;
}

impl WeightedNodeEdgeBase for WeightedNodeEdge {
    fn get_weight(&self) -> f64 {
        self.weight
    }
}

impl WeightedNodeEdge {
    pub fn new(target_id: NodeId, weight: f64) -> Self {
        Self { target_id, weight }
    }
}

pub trait NodeBase
where
    Self: Sized,
{
    type NodeIdType: Clone + Ord;
    type NodeEdgeType: NodeEdgeBase + Sized;
    type NodeSetType;

    fn get_id(&self) -> Self::NodeIdType;
    // used to return *all* edges
    fn get_edges(&self) -> Box<dyn Iterator<Item = &Self::NodeEdgeType> + '_>;
    // used to return *outgoing* edges only (to perform a traversal)
    fn get_outgoing_edges(&self) -> Box<dyn Iterator<Item = &Self::NodeEdgeType> + '_>;
    fn degree(&self) -> usize;
    fn count_ties_with_ids(&self, ids: &Self::NodeSetType) -> usize;
}

/// Core data structure used to represent a node in our graph. A node can be
/// either a "core" node, or a non-core node. Non-core nodes also have a type (e.g.
/// IP, URL, etc.) Each node also keeps track of its neighbors, via a vector of
/// edges that specify edge type and target node.
pub struct Node {
    pub node_id: u32,
    pub is_core: bool,
    pub non_core_type: Option<NodeTypeId>,
    pub edges: Vec<NodeEdge>,
    pub neighbors_sets: HashMap<EdgeTypeId, RoaringBitmap>,
}
impl Hash for Node {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.node_id.hash(state);
    }
}
impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.node_id == other.node_id
    }
}
impl Eq for Node {}
impl NodeBase for Node {
    type NodeEdgeType = NodeEdge;
    type NodeIdType = u32;
    type NodeSetType = RoaringBitmap;

    fn get_id(&self) -> u32 {
        self.node_id
    }
    fn get_edges(&self) -> Box<dyn Iterator<Item = &NodeEdge> + '_> {
        Box::new(self.edges.iter())
    }
    fn get_outgoing_edges(&self) -> Box<dyn Iterator<Item = &NodeEdge> + '_> {
        self.get_edges()
    }
    /// degree is the edge count (in an unweighted graph)
    fn degree(&self) -> usize {
        self.edges.len()
    }

    fn count_ties_with_ids(&self, ids: &RoaringBitmap) -> usize {
        self.neighbors_sets
            .values()
            .map(|neighbors| neighbors.intersection_len(ids))
            .sum::<u64>() as usize
    }
}

impl Node {
    pub fn new(
        node_id: u32,
        is_core: bool,
        non_core_type: Option<NodeTypeId>,
        edges: Vec<NodeEdge>,
        neighbors_sets: HashMap<EdgeTypeId, RoaringBitmap>,
    ) -> Node {

        Node {
            node_id,
            is_core,
            non_core_type,
            edges,
            // neighbors,
            neighbors_sets,
        }
    }
    pub fn is_core(&self) -> bool {
        self.is_core
    }
    pub fn max_edge_count_with_core_node(&self) -> CLQResult<Option<usize>> {
        let non_core_type = self.non_core_type.ok_or_else(|| {
            CLQError::from(format!(
                "Node {} is unexpextedly a core node.",
                self.node_id
            ))
        })?;
        Ok(non_core_type.max_edge_count_with_core_node())
    }
}

pub struct SimpleNode {
    pub node_id: NodeId,
    pub neighbors: BTreeSet<NodeId>,
}
impl Hash for SimpleNode {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.node_id.hash(state);
    }
}
impl PartialEq for SimpleNode {
    fn eq(&self, other: &Self) -> bool {
        self.node_id == other.node_id
    }
}
impl Eq for SimpleNode {}
impl NodeBase for SimpleNode {
    type NodeEdgeType = NodeId;
    type NodeIdType = NodeId;
    type NodeSetType = FxHashSet<NodeId>;

    fn get_id(&self) -> NodeId {
        self.node_id
    }
    fn get_edges(&self) -> Box<dyn Iterator<Item = &NodeId> + '_> {
        Box::new(self.neighbors.iter())
    }
    fn get_outgoing_edges(&self) -> Box<dyn Iterator<Item = &NodeId> + '_> {
        self.get_edges()
    }
    /// degree is the edge count (in an unweighted graph)
    fn degree(&self) -> usize {
        self.neighbors.len()
    }
    /// used to determine degree in a subgraph (i.e., the clique we're considering).
    /// HashSet is supplied by Candidate struct.
    fn count_ties_with_ids(&self, ids: &FxHashSet<NodeId>) -> usize {
        ids.iter().filter(|x| self.neighbors.contains(x)).count()
    }
}

pub trait DirectedNodeBase:
    NodeBase<NodeIdType = NodeId, NodeEdgeType: NodeEdgeBase<NodeIdType = NodeId>>
{
    fn get_in_neighbors(&self) -> Box<dyn Iterator<Item = &Self::NodeEdgeType> + '_>;
    fn get_out_neighbors(&self) -> Box<dyn Iterator<Item = &Self::NodeEdgeType> + '_>;
    fn has_in_neighbor(&self, nid: NodeId) -> bool;
    fn has_out_neighbor(&self, nid: NodeId) -> bool;
    fn get_in_degree(&self) -> usize;
    fn get_out_degree(&self) -> usize;
    // used to determine if the node is a leaf
    fn has_no_out_neighbors_except_set(&self, exclude_set: &HashSet<NodeId>) -> bool {
        for e in self.get_out_neighbors() {
            let nid = e.get_neighbor_id();
            if !exclude_set.contains(&nid) {
                return false;
            }
        }
        true
    }
}
pub struct SimpleDirectedNode {
    pub node_id: NodeId,
    pub in_neighbors: BTreeSet<NodeId>,
    pub out_neighbors: BTreeSet<NodeId>,
}
impl DirectedNodeBase for SimpleDirectedNode {
    fn get_in_neighbors(&self) -> Box<dyn Iterator<Item = &Self::NodeEdgeType> + '_> {
        Box::new(self.in_neighbors.iter())
    }
    fn get_out_neighbors(&self) -> Box<dyn Iterator<Item = &Self::NodeEdgeType> + '_> {
        Box::new(self.out_neighbors.iter())
    }
    fn has_in_neighbor(&self, nid: NodeId) -> bool {
        self.in_neighbors.contains(&nid)
    }
    fn has_out_neighbor(&self, nid: NodeId) -> bool {
        self.out_neighbors.contains(&nid)
    }
    fn get_in_degree(&self) -> usize {
        self.in_neighbors.len()
    }
    fn get_out_degree(&self) -> usize {
        self.out_neighbors.len()
    }
}
impl Hash for SimpleDirectedNode {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.node_id.hash(state);
    }
}
impl PartialEq for SimpleDirectedNode {
    fn eq(&self, other: &Self) -> bool {
        self.node_id == other.node_id
    }
}
impl Eq for SimpleDirectedNode {}
impl NodeBase for SimpleDirectedNode {
    type NodeEdgeType = NodeId;
    type NodeSetType = FxHashSet<NodeId>;
    type NodeIdType = NodeId;

    fn get_id(&self) -> NodeId {
        self.node_id
    }
    fn get_edges(&self) -> Box<dyn Iterator<Item = &NodeId> + '_> {
        Box::new(self.in_neighbors.iter().chain(self.out_neighbors.iter()))
    }
    fn get_outgoing_edges(&self) -> Box<dyn Iterator<Item = &NodeId> + '_> {
        self.get_edges()
    }
    /// degree is the edge count (in an unweighted graph)
    fn degree(&self) -> usize {
        self.in_neighbors.len() + self.out_neighbors.len()
    }
    /// used to determine degree in a subgraph (i.e., the clique we're considering).
    /// HashSet is supplied by Candidate struct.
    fn count_ties_with_ids(&self, ids: &FxHashSet<NodeId>) -> usize {
        ids.iter()
            .filter(|x| self.in_neighbors.contains(x) || self.out_neighbors.contains(x))
            .count()
    }
}

pub trait WeightedNodeBase: NodeBase {
    fn weight(&self) -> f64;
}
pub struct WeightedNode {
    pub node_id: NodeId,
    pub edges: Vec<WeightedNodeEdge>,
    pub neighbors: BTreeSet<NodeId>,
}
impl WeightedNodeBase for WeightedNode {
    fn weight(&self) -> f64 {
        self.edges.iter().map(|x| x.get_weight()).sum()
    }
}
impl Hash for WeightedNode {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.node_id.hash(state);
    }
}
impl PartialEq for WeightedNode {
    fn eq(&self, other: &Self) -> bool {
        self.node_id == other.node_id
    }
}
impl Eq for WeightedNode {}
impl NodeBase for WeightedNode {
    type NodeIdType = NodeId;
    type NodeEdgeType = WeightedNodeEdge;
    type NodeSetType = FxHashSet<NodeId>;

    fn get_id(&self) -> NodeId {
        self.node_id
    }
    fn get_edges(&self) -> Box<dyn Iterator<Item = &WeightedNodeEdge> + '_> {
        Box::new(self.edges.iter())
    }
    fn get_outgoing_edges(&self) -> Box<dyn Iterator<Item = &WeightedNodeEdge> + '_> {
        self.get_edges()
    }
    /// degree is the edge count (in an unweighted graph)
    fn degree(&self) -> usize {
        self.edges.len()
    }

    fn count_ties_with_ids(&self, ids: &FxHashSet<NodeId>) -> usize {
        ids.iter().filter(|x| self.neighbors.contains(x)).count()
    }
}
