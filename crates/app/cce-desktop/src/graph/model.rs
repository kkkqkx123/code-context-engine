//! Canvas-side topology: the payload of one graph answer, indexed for drawing.
//!
//! The desktop canvas consumes answers that already carry nodes and edges
//! (neighbourhood, path, explicit subgraph). Everything here is pure data so
//! it can be unit tested without a window: the paint layer only reads it.

use std::collections::HashMap;

use cce_api::models::{GraphEdge, GraphNode};
use compograph_graph::{AdjacencySnapshot, FixedNodes, NodeIndex, Positions};
use compograph_layout::{ForceLayout, GridLayout, LayoutEngine, RadialLayout};
use compograph_types::Point2;

/// Default node ceiling for one canvas.
///
/// The graph panel's `limit` input usually stays at 200; the ceiling keeps a
/// pasted larger limit from turning a quick look into thousands of layouts.
pub const DEFAULT_MAX_NODES: usize = 200;

/// One drawable node, carrying the fields the inspector shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanvasNode {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub file: String,
    pub location: String,
    pub scoped_name: Option<String>,
}

/// One drawable edge, indexed by canvas node position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanvasEdge {
    pub source: usize,
    pub target: usize,
    pub relation: String,
    pub domain: String,
    pub confidence: String,
    pub is_external: bool,
}

/// Node and edge set of one answer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Topology {
    nodes: Vec<CanvasNode>,
    edges: Vec<CanvasEdge>,
    /// Node key (stable id and label) to canvas index.
    index: HashMap<String, usize>,
}

impl Topology {
    /// Builds the topology from a node/edge answer.
    ///
    /// Edges reference nodes by a string key that is the stable id on some
    /// relation families and the label on others, so both keys are indexed.
    /// Edges whose endpoints are missing from the node list are dropped: the
    /// server truncates node lists independently of edge lists, and a dangling
    /// edge would otherwise be drawn to the origin.
    pub fn from_answer(nodes: &[GraphNode], edges: &[GraphEdge]) -> Self {
        let mut index = HashMap::with_capacity(nodes.len() * 2);
        let mut kept = Vec::with_capacity(nodes.len());
        for node in nodes {
            let position = kept.len();
            index.entry(node.id.clone()).or_insert(position);
            index.entry(node.label.clone()).or_insert(position);
            kept.push(CanvasNode {
                id: node.id.clone(),
                label: node.label.clone(),
                kind: node.kind.clone(),
                file: node.source_file.clone(),
                location: node.source_location.clone(),
                scoped_name: node.scoped_name.clone(),
            });
        }

        let mut seen: HashMap<(usize, usize, &str), ()> = HashMap::new();
        let mut kept_edges = Vec::with_capacity(edges.len());
        for edge in edges {
            let (Some(&source), Some(&target)) = (index.get(&edge.source), index.get(&edge.target))
            else {
                continue;
            };
            // Parallel edges of the same relation carry no extra information
            // for the layout and only stack drawing work, so they collapse.
            if seen
                .insert((source, target, edge.relation.as_str()), ())
                .is_some()
            {
                continue;
            }
            kept_edges.push(CanvasEdge {
                source,
                target,
                relation: edge.relation.clone(),
                domain: edge.domain.clone(),
                confidence: edge.confidence.clone(),
                is_external: edge.is_external,
            });
        }

        Self {
            nodes: kept,
            edges: kept_edges,
            index,
        }
    }

    /// Keep the `max` most connected nodes and the edges between them.
    ///
    /// Degree is computed before the cut, so a hub that only touches dropped
    /// leaves survives with its remaining edges; edges crossing the cut are
    /// removed rather than drawn to a node that no longer exists. Returns the
    /// number of dropped nodes so the panel can say so instead of silently
    /// showing a partial answer.
    pub fn capped(self, max: usize) -> (Self, usize) {
        if self.nodes.len() <= max {
            return (self, 0);
        }
        let degrees = self.degrees();
        let mut ranked: Vec<usize> = (0..self.nodes.len()).collect();
        ranked.sort_unstable_by(|left, right| {
            degrees[*right]
                .cmp(&degrees[*left])
                .then_with(|| left.cmp(right))
        });
        ranked.truncate(max);
        let mut remap = HashMap::with_capacity(ranked.len());
        for (next, old) in ranked.iter().enumerate() {
            remap.insert(*old, next);
        }
        let dropped = self.nodes.len() - ranked.len();
        let nodes: Vec<CanvasNode> = ranked.iter().map(|old| self.nodes[*old].clone()).collect();
        let edges = self
            .edges
            .iter()
            .filter_map(|edge| {
                Some(CanvasEdge {
                    source: *remap.get(&edge.source)?,
                    target: *remap.get(&edge.target)?,
                    relation: edge.relation.clone(),
                    domain: edge.domain.clone(),
                    confidence: edge.confidence.clone(),
                    is_external: edge.is_external,
                })
            })
            .collect();
        let mut index = HashMap::with_capacity(nodes.len() * 2);
        for (position, node) in nodes.iter().enumerate() {
            index.entry(node.id.clone()).or_insert(position);
            index.entry(node.label.clone()).or_insert(position);
        }
        (
            Self {
                nodes,
                edges,
                index,
            },
            dropped,
        )
    }

    pub fn nodes(&self) -> &[CanvasNode] {
        &self.nodes
    }

    pub fn edges(&self) -> &[CanvasEdge] {
        &self.edges
    }

    /// Degree of every node, counting a self loop once.
    pub fn degrees(&self) -> Vec<usize> {
        let mut degrees = vec![0usize; self.nodes.len()];
        for edge in &self.edges {
            degrees[edge.source] += 1;
            if edge.target != edge.source {
                degrees[edge.target] += 1;
            }
        }
        degrees
    }

    /// Compressed identifier list the paint plans work with.
    pub fn ids(&self) -> Vec<NodeIndex> {
        (0..self.nodes.len()).map(NodeIndex::new).collect()
    }

    /// Endpoint pairs matching [`Self::ids`].
    pub fn pairs(&self) -> Vec<(NodeIndex, NodeIndex)> {
        self.edges
            .iter()
            .map(|edge| (NodeIndex::new(edge.source), NodeIndex::new(edge.target)))
            .collect()
    }
}

/// Layout families the canvas offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutKind {
    /// Force-directed: shows clusters and hubs, the useful default for code
    /// graphs where fan-in and fan-out are the question.
    #[default]
    Force,
    /// Concentric rings by degree: reads like a dependency depth chart.
    Radial,
    /// Regular grid: the only deterministic ordering when nothing else reads.
    Grid,
}

impl LayoutKind {
    /// Every layout, in display order.
    pub const ALL: [Self; 3] = [Self::Force, Self::Radial, Self::Grid];

    /// Catalogue key for the layout name.
    pub fn key(self) -> &'static str {
        match self {
            Self::Force => "graph.layout.force",
            Self::Radial => "graph.layout.radial",
            Self::Grid => "graph.layout.grid",
        }
    }

    /// Runs the layout over the whole topology.
    ///
    /// The run is synchronous: answers are capped by the panel's limit and a
    /// background refinement would only make the first frame arrive later.
    pub fn compute(self, topology: &Topology) -> Positions {
        let ids = topology.ids();
        let snapshot = AdjacencySnapshot::from_parts(ids, topology.pairs());
        let previous = Positions::new();
        let fixed = FixedNodes::new();
        match self {
            Self::Force => ForceLayout::new().layout(&snapshot, &previous, &fixed),
            Self::Radial => RadialLayout::new(Point2::ZERO).layout(&snapshot, &previous, &fixed),
            Self::Grid => {
                let side = (topology.nodes.len() as f32).sqrt().ceil().max(1.0);
                let extent = side * 120.0;
                GridLayout::new(Point2::ZERO, extent, extent).layout(&snapshot, &previous, &fixed)
            }
        }
    }
}

/// Half extent of a node body in world units, matching the paint plan.
pub const NODE_HALF_EXTENT: f32 = 12.0;

/// Tolerance in world units applied on top of the body when picking.
pub const PICK_TOLERANCE: f32 = 6.0;

/// Nearest node under a world-space point.
///
/// Picking is nearest-centre within the body plus tolerance instead of a
/// shape test: at the zoom levels a code graph is readable at, the shapes are
/// a few pixels wide and a shape test would make thin nodes unclickable.
pub fn hit_test(topology: &Topology, positions: &Positions, world: Point2) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for index in 0..topology.nodes.len() {
        let Some(center) = positions.get(&NodeIndex::new(index)).copied() else {
            continue;
        };
        let dx = center.x - world.x;
        let dy = center.y - world.y;
        if dx.abs() > NODE_HALF_EXTENT + PICK_TOLERANCE
            || dy.abs() > NODE_HALF_EXTENT + PICK_TOLERANCE
        {
            continue;
        }
        let distance = dx * dx + dy * dy;
        if best.is_none_or(|(_, current)| distance < current) {
            best = Some((index, distance));
        }
    }
    best.map(|(index, _)| index)
}

/// Frames every node inside the viewport, keeping a fixed margin.
///
/// An empty position set leaves the camera untouched so clearing an answer
/// never collapses the view to the origin.
pub fn fit_camera(
    camera: &mut compograph_render::Camera,
    positions: &Positions,
    viewport: compograph_types::Vec2,
    padding: f32,
) -> bool {
    let Some(bounds) = compograph_render::graph_bounds(positions, 0.0) else {
        return false;
    };
    camera.fit_to_bounds(bounds, viewport, padding);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, label: &str, kind: &str) -> GraphNode {
        GraphNode {
            id: id.to_string(),
            label: label.to_string(),
            kind: kind.to_string(),
            source_file: format!("src/{label}.rs"),
            source_location: "1:1".to_string(),
            scoped_name: None,
            signature: None,
        }
    }

    fn edge(source: &str, target: &str, relation: &str) -> GraphEdge {
        GraphEdge {
            source: source.to_string(),
            target: target.to_string(),
            relation: relation.to_string(),
            domain: "call".to_string(),
            confidence: "extracted".to_string(),
            call_context: None,
            is_external: false,
            weight: 1.0,
            cfg_condition: None,
            callsite: None,
            edge_id: format!("{source}->{target}:{relation}"),
        }
    }

    #[test]
    fn edges_resolve_through_ids_and_labels() {
        let nodes = [
            node("sym:1", "alpha", "function"),
            node("sym:2", "beta", "method"),
        ];
        // Both spellings resolve to the same pair, so the relation differs to
        // keep them apart; identical relations collapse on purpose.
        let edges = [
            edge("sym:1", "beta", "calls"),
            edge("alpha", "sym:2", "references"),
        ];
        let topology = Topology::from_answer(&nodes, &edges);
        assert_eq!(topology.nodes().len(), 2);
        assert_eq!(topology.edges().len(), 2);
        for edge in topology.edges() {
            assert_eq!(edge.source, 0);
            assert_eq!(edge.target, 1);
        }
    }

    #[test]
    fn dangling_and_duplicate_edges_are_dropped() {
        let nodes = [
            node("a", "alpha", "function"),
            node("b", "beta", "function"),
        ];
        let edges = [
            edge("a", "b", "calls"),
            edge("a", "b", "calls"),
            edge("a", "missing", "calls"),
        ];
        let topology = Topology::from_answer(&nodes, &edges);
        assert_eq!(topology.edges().len(), 1);
    }

    #[test]
    fn degrees_count_a_self_loop_once() {
        let nodes = [
            node("a", "alpha", "function"),
            node("b", "beta", "function"),
        ];
        let edges = [
            edge("a", "a", "recurses"),
            edge("a", "b", "calls"),
            edge("b", "a", "calls"),
        ];
        let topology = Topology::from_answer(&nodes, &edges);
        // The self loop touches its node once, so `alpha` counts three.
        assert_eq!(topology.degrees(), vec![3, 2]);
    }

    #[test]
    fn opposite_edges_are_kept_apart() {
        let nodes = [
            node("a", "alpha", "function"),
            node("b", "beta", "function"),
        ];
        let edges = [edge("a", "b", "calls"), edge("b", "a", "calls")];
        let topology = Topology::from_answer(&nodes, &edges);
        assert_eq!(topology.edges().len(), 2);
        assert_eq!(topology.degrees(), vec![2, 2]);
    }

    #[test]
    fn every_layout_positions_every_node() {
        let nodes = [
            node("a", "alpha", "function"),
            node("b", "beta", "function"),
            node("c", "gamma", "function"),
        ];
        let edges = [edge("a", "b", "calls"), edge("b", "c", "calls")];
        let topology = Topology::from_answer(&nodes, &edges);
        for kind in LayoutKind::ALL {
            let positions = kind.compute(&topology);
            assert_eq!(positions.len(), 3, "{:?} dropped a node", kind);
            for point in positions.values() {
                assert!(point.x.is_finite() && point.y.is_finite());
            }
        }
    }

    #[test]
    fn grid_layout_is_deterministic() {
        let nodes = [
            node("a", "alpha", "function"),
            node("b", "beta", "function"),
        ];
        let edges = [edge("a", "b", "calls")];
        let topology = Topology::from_answer(&nodes, &edges);
        assert_eq!(
            LayoutKind::Grid.compute(&topology),
            LayoutKind::Grid.compute(&topology)
        );
    }

    #[test]
    fn hit_test_takes_the_nearest_node_inside_the_tolerance() {
        let nodes = [
            node("a", "alpha", "function"),
            node("b", "beta", "function"),
        ];
        let edges = [edge("a", "b", "calls")];
        let topology = Topology::from_answer(&nodes, &edges);
        let mut positions = Positions::new();
        positions.insert(NodeIndex::new(0), Point2::new(0.0, 0.0));
        positions.insert(NodeIndex::new(1), Point2::new(200.0, 0.0));

        assert_eq!(
            hit_test(&topology, &positions, Point2::new(2.0, 2.0)),
            Some(0)
        );
        assert_eq!(
            hit_test(&topology, &positions, Point2::new(196.0, 0.0)),
            Some(1)
        );
        assert_eq!(
            hit_test(&topology, &positions, Point2::new(90.0, 0.0)),
            None
        );
    }

    #[test]
    fn fit_camera_brings_every_node_inside_the_viewport() {
        let mut positions = Positions::new();
        positions.insert(NodeIndex::new(0), Point2::new(-800.0, -400.0));
        positions.insert(NodeIndex::new(1), Point2::new(800.0, 400.0));

        let mut camera = compograph_render::Camera::new(Point2::ZERO, 1.0);
        let viewport = compograph_types::Vec2::new(600.0, 400.0);
        assert!(fit_camera(&mut camera, &positions, viewport, 24.0));
        for point in positions.values() {
            let screen = camera.world_to_viewport(viewport, *point);
            assert!(
                screen.x >= 0.0 && screen.x <= 600.0,
                "x {} escaped",
                screen.x
            );
            assert!(
                screen.y >= 0.0 && screen.y <= 400.0,
                "y {} escaped",
                screen.y
            );
        }
    }

    #[test]
    fn fit_camera_keeps_the_view_when_there_is_nothing_to_fit() {
        let mut camera = compograph_render::Camera::new(Point2::new(5.0, 5.0), 2.0);
        let (center, zoom) = (camera.center, camera.zoom);
        assert!(!fit_camera(
            &mut camera,
            &Positions::new(),
            compograph_types::Vec2::new(100.0, 100.0),
            8.0
        ));
        assert_eq!(camera.center, center);
        assert_eq!(camera.zoom, zoom);
    }
    #[test]
    fn capping_keeps_hubs_and_drops_edges_crossing_the_cut() {
        let nodes = [
            node("a", "alpha", "function"),
            node("b", "beta", "function"),
            node("c", "gamma", "function"),
        ];
        let edges = [
            edge("a", "b", "calls"),
            edge("a", "c", "calls"),
            edge("b", "c", "calls"),
        ];
        let topology = Topology::from_answer(&nodes, &edges);
        let (capped, dropped) = topology.capped(2);
        assert_eq!(dropped, 1);
        assert_eq!(capped.nodes().len(), 2);
        // `alpha` and `beta` have equal degree, so the stable order keeps
        // them and the edge through the dropped `gamma` disappears.
        assert_eq!(capped.edges().len(), 1);
        assert!(capped.nodes().iter().any(|node| node.label == "alpha"));
    }
}
