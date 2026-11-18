//! Interactive relation canvas for the graph panel.
//!
//! The widget owns one answer's topology plus its viewport. Painting is
//! delegated to the `compograph` paint plans, which take fully resolved screen
//! geometry: this module only decides positions, colours and what the pointer
//! did. Layout runs once per answer, not per frame.

use std::collections::HashMap;

use compograph_graph::{NodeIndex, Positions};
use compograph_interact::{PanState, wheel_zoom_factor};
use compograph_render::WaypointStore;
use compograph_render::{
    Camera, DEFAULT_EDGE_TINT, DetailLevel, EDGE_AGGREGATION_THRESHOLD, EdgePaintOptions,
    EdgeStyle, ExportRequest, ExportScope, ExportSnapshot, HIGHLIGHT_EDGE_TINT, HOVER_NODE_FILL,
    LabelStyle, NodeFill, NodeShape, NodeStyle, SELECTED_NODE_FILL, encode_png, export_pixels,
    graph_view, paint_arrows_for_level, paint_edge_labels_for, paint_edges_for, paint_labels_for,
    paint_nodes_for_level, world_viewport_rect,
};
use compograph_types::{Point2, Vec2};
use gpui_kit::{
    Bounds, Context, CursorStyle, EventEmitter, InteractiveElement as _, IntoElement, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement as _, Pixels, Render, ScrollDelta,
    ScrollEvent, Styled as _, Window, div,
};

use crate::design_tokens::{self, Section};
use crate::graph::model::{self, CanvasNode, LayoutKind, NODE_HALF_EXTENT, Topology};

/// Detail level the canvas always asks for.
///
/// The desktop canvas never carries enough nodes for the lower levels to pay
/// off, and dropping labels at low zoom would hide the only thing that makes a
/// code graph readable.
const LEVEL: DetailLevel = DetailLevel::Full;

/// Margin kept clear around a fitted graph, in logical pixels.
const FIT_PADDING: f32 = 28.0;

/// Pixels per scroll line, used when the platform reports pixel deltas.
const SCROLL_PIXELS_PER_LINE: f32 = 40.0;

/// Requests the canvas sends to its host panel.
///
/// The canvas never talks to the client or the navigator: it reports what the
/// user asked for and the panel decides, so the widget stays reusable and
/// testable without a backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanvasEvent {
    /// A node was double clicked: re-root the neighbourhood on it.
    Expand { label: String },
}

/// Painted topology plus its camera and pointer state.
pub struct GraphCanvas {
    topology: Topology,
    layout_kind: LayoutKind,
    positions: Positions,
    camera: Camera,
    bounds: Option<Bounds<Pixels>>,
    viewport: Vec2,
    pan: PanState,
    hovered: Option<usize>,
    selected: Option<usize>,
    fit_pending: bool,
    /// Relation text per endpoint pair, indexed once per answer instead of on
    /// every frame.
    relations: HashMap<(usize, usize), String>,
    /// Node degree per canvas index, driving the node size.
    degrees: Vec<usize>,
    /// Nodes removed by the panel's ceiling on the last answer.
    dropped: usize,
}

impl EventEmitter<CanvasEvent> for GraphCanvas {}

impl GraphCanvas {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let _ = cx;
        Self {
            topology: Topology::default(),
            layout_kind: LayoutKind::default(),
            positions: Positions::new(),
            camera: Camera::new(Point2::ZERO, 1.0),
            bounds: None,
            viewport: Vec2::ZERO,
            pan: PanState::default(),
            hovered: None,
            selected: None,
            fit_pending: true,
            relations: HashMap::new(),
            degrees: Vec::new(),
            dropped: 0,
        }
    }

    /// Replaces the drawn answer and re-runs the layout.
    ///
    /// The camera refits because a new answer is a different graph: keeping
    /// the previous viewport would land the user on empty space.
    pub fn set_topology(&mut self, topology: Topology, dropped: usize, cx: &mut Context<Self>) {
        self.topology = topology;
        self.dropped = dropped;
        self.relations = self
            .topology
            .edges()
            .iter()
            .map(|edge| ((edge.source, edge.target), edge.relation.clone()))
            .collect();
        self.selected = None;
        self.hovered = None;
        self.degrees = self.topology.degrees();
        self.positions = self.layout_kind.compute(&self.topology);
        self.fit_pending = true;
        self.fit();
        cx.notify();
    }

    pub fn layout_kind(&self) -> LayoutKind {
        self.layout_kind
    }

    /// Switches the layout family and refits the camera.
    pub fn set_layout(&mut self, kind: LayoutKind, cx: &mut Context<Self>) {
        if self.layout_kind == kind {
            return;
        }
        self.layout_kind = kind;
        self.positions = kind.compute(&self.topology);
        self.fit_pending = true;
        self.fit();
        cx.notify();
    }

    /// Nodes removed from the current answer by the ceiling.
    pub fn dropped_nodes(&self) -> usize {
        self.dropped
    }

    /// Rasterise the whole graph to a PNG, independent of the current camera.
    ///
    /// The export snapshot is plain data so the software rasteriser can run
    /// on the interface thread without touching the entity or the window.
    pub fn export_png(&self) -> Option<Vec<u8>> {
        if self.topology.nodes().is_empty() {
            return None;
        }
        let node_ids: Vec<NodeIndex> = (0..self.topology.nodes().len())
            .map(NodeIndex::new)
            .collect();
        let pairs = self.topology.pairs();
        let node_styles = node_ids
            .iter()
            .map(|node| (*node, self.node_style(*node)))
            .collect();
        let edge_styles = pairs
            .iter()
            .map(|(source, target)| ((*source, *target), self.edge_style(*source, *target)))
            .collect();
        let node_texts = node_ids
            .iter()
            .filter_map(|node| self.node_label(*node).map(|text| (*node, text)))
            .collect();
        let edge_texts = pairs
            .iter()
            .filter_map(|(source, target)| {
                self.edge_label(*source, *target)
                    .map(|text| ((*source, *target), text))
            })
            .collect();
        let viewport = if self.viewport.x > 0.0 && self.viewport.y > 0.0 {
            self.viewport
        } else {
            Vec2::new(800.0, 600.0)
        };
        let snapshot = ExportSnapshot {
            node_ids,
            pairs,
            positions: self.positions.clone(),
            node_styles,
            edge_styles,
            node_texts,
            edge_texts,
            camera: self.camera,
            viewport,
            aggregate: false,
            waypoints: WaypointStore::new(),
        };
        let request = ExportRequest {
            scope: ExportScope::FullGraph,
            scale: 2.0,
            viewport,
        };
        let (pixels, width, height) = export_pixels(ExportScope::FullGraph, request, &snapshot)?;
        Some(encode_png(width, height, &pixels))
    }

    pub fn selected_node(&self) -> Option<&CanvasNode> {
        self.selected
            .and_then(|index| self.topology.nodes().get(index))
    }

    /// Neighbours of the selected node, split by direction.
    pub fn selected_neighbours(&self) -> (Vec<&CanvasNode>, Vec<&CanvasNode>) {
        let Some(selected) = self.selected else {
            return (Vec::new(), Vec::new());
        };
        let mut outgoing = Vec::new();
        let mut incoming = Vec::new();
        for edge in self.topology.edges() {
            if edge.source == selected
                && let Some(node) = self.topology.nodes().get(edge.target)
            {
                outgoing.push(node);
            }
            if edge.target == selected
                && let Some(node) = self.topology.nodes().get(edge.source)
            {
                incoming.push(node);
            }
        }
        (outgoing, incoming)
    }

    /// Frames the whole topology in the viewport.
    pub fn fit(&mut self) -> bool {
        if self.viewport.x <= 1.0 || self.viewport.y <= 1.0 {
            self.fit_pending = true;
            return false;
        }
        let fitted = model::fit_camera(
            &mut self.camera,
            &self.positions,
            self.viewport,
            FIT_PADDING,
        );
        self.fit_pending = !fitted;
        fitted
    }

    pub fn zoom_in(&mut self, cx: &mut Context<Self>) {
        self.zoom(1.25, cx);
    }

    pub fn zoom_out(&mut self, cx: &mut Context<Self>) {
        self.zoom(0.8, cx);
    }

    fn zoom(&mut self, factor: f32, cx: &mut Context<Self>) {
        let anchor = Point2::new(self.viewport.x / 2.0, self.viewport.y / 2.0);
        self.camera.zoom_at(self.viewport, anchor, factor);
        cx.notify();
    }

    /// Pointer position in canvas-local logical pixels.
    fn local(&self, position: gpui_kit::Point<Pixels>) -> Option<Point2> {
        let bounds = self.bounds?;
        Some(Point2::new(
            f32::from(position.x - bounds.origin.x),
            f32::from(position.y - bounds.origin.y),
        ))
    }

    fn measure(&mut self, bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
        let size = Vec2::new(f32::from(bounds.size.width), f32::from(bounds.size.height));
        let moved = self
            .bounds
            .is_none_or(|known| known.origin != bounds.origin || known.size != bounds.size);
        if !moved {
            return;
        }
        let resized = self.viewport != size;
        self.bounds = Some(bounds);
        self.viewport = size;
        // A resized viewport keeps the fitted framing instead of leaving the
        // graph outside the new bounds. A pure move (the panel scrolled) only
        // updates the origin used for pointer mapping; notifying there would
        // repaint on every scroll frame.
        if self.fit_pending || resized {
            self.fit();
            cx.notify();
        }
    }

    /// Fill and border of one node, per the shared node palette.
    fn node_colors(&self, index: usize) -> (u32, u32) {
        let Some(node) = self.topology.nodes().get(index) else {
            return (DEFAULT_FILL, DEFAULT_STROKE);
        };
        palette(&node.kind)
    }

    fn node_style(&self, node: NodeIndex) -> NodeStyle {
        let index = node.index();
        let (fill, stroke) = self.node_colors(index);
        let selected = self.selected == Some(index);
        let hovered = self.hovered == Some(index);
        NodeStyle {
            fill: NodeFill::solid(if selected {
                SELECTED_NODE_FILL
            } else if hovered {
                HOVER_NODE_FILL
            } else {
                fill
            }),
            stroke,
            stroke_width: if selected { 2.5 } else { 1.5 },
            opacity: 1.0,
            label_size: if selected { 13.0 } else { 12.0 },
            // Hubs grow slightly with degree so fan-in is visible before any
            // node is selected.
            scale: if selected {
                1.45
            } else if hovered {
                1.25
            } else {
                1.0 + (self.degrees.get(index).copied().unwrap_or(0).min(8) as f32) * 0.04
            },
            shape: NodeShape::Circle,
            image: None,
            label: LabelStyle::default(),
        }
    }

    /// True when the pair touches the selection.
    fn highlighted(&self, source: NodeIndex, target: NodeIndex) -> bool {
        self.selected
            .is_some_and(|selected| selected == source.index() || selected == target.index())
    }

    fn edge_style(&self, source: NodeIndex, target: NodeIndex) -> EdgeStyle {
        let highlighted = self.highlighted(source, target);
        EdgeStyle {
            tint: if highlighted {
                HIGHLIGHT_EDGE_TINT
            } else {
                DEFAULT_EDGE_TINT
            },
            width: if highlighted { 2.0 } else { 1.2 },
            // Dim everything outside the selected neighbourhood: the point of
            // selecting a node is to read its fan-in and fan-out.
            opacity: if self.selected.is_some() && !highlighted {
                0.25
            } else {
                0.9
            },
            arrow_scale: if highlighted { 1.2 } else { 1.0 },
            ..EdgeStyle::default()
        }
    }

    fn node_label(&self, node: NodeIndex) -> Option<String> {
        self.topology
            .nodes()
            .get(node.index())
            .map(|node| node.label.clone())
    }

    fn edge_label(&self, source: NodeIndex, target: NodeIndex) -> Option<String> {
        if !self.highlighted(source, target) {
            return None;
        }
        self.relations
            .get(&(source.index(), target.index()))
            .cloned()
    }

    /// Node indices whose body overlaps the viewport.
    fn visible(&self) -> Vec<NodeIndex> {
        let rect = world_viewport_rect(&self.camera, self.viewport);
        let mut visible = Vec::new();
        for index in 0..self.topology.nodes().len() {
            let node = NodeIndex::new(index);
            let Some(point) = self.positions.get(&node) else {
                continue;
            };
            // The body is half an extent wide, so a node whose centre sits just
            // outside the rect still paints inside it.
            let near = point.x + NODE_HALF_EXTENT >= rect.origin.x
                && point.x - NODE_HALF_EXTENT <= rect.origin.x + rect.size.x
                && point.y + NODE_HALF_EXTENT >= rect.origin.y
                && point.y - NODE_HALF_EXTENT <= rect.origin.y + rect.size.y;
            if near {
                visible.push(node);
            }
        }
        visible
    }
}

impl Render for GraphCanvas {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = self.viewport;
        let visible = self.visible();
        let pairs: Vec<(NodeIndex, NodeIndex)> = self
            .topology
            .pairs()
            .into_iter()
            .filter(|(source, target)| {
                self.positions.contains_key(source) && self.positions.contains_key(target)
            })
            .collect();
        let camera = self.camera;
        let positions = &self.positions;

        let nodes = paint_nodes_for_level(&visible, positions, &camera, viewport, LEVEL, |node| {
            self.node_style(node)
        });
        let options = EdgePaintOptions {
            level: LEVEL,
            aggregate_threshold: EDGE_AGGREGATION_THRESHOLD,
            force_haystack: false,
            ortho: None,
            taxi: None,
        };
        let edges = paint_edges_for(
            &pairs,
            positions,
            &camera,
            viewport,
            options,
            |source, target| self.edge_style(source, target),
        );
        let arrows = paint_arrows_for_level(&edges, LEVEL);
        let labels = paint_labels_for(&nodes, LEVEL, |node| self.node_label(node), |_| 12.0);
        let edge_labels = paint_edge_labels_for(
            &edges,
            LEVEL,
            |source, target| self.edge_label(source, target),
            |_, _| 11.0,
        );
        let cursor = if self.hovered.is_some() || self.pan.is_active() {
            CursorStyle::PointingHand
        } else {
            CursorStyle::Arrow
        };
        // The canvas area is only known once the element has been laid out, so
        // the measurement lands here and the next frame draws with it.
        let weak = cx.entity().downgrade();

        div()
            .relative()
            .size_full()
            .cursor(cursor)
            .child(graph_view(
                Vec::new(),
                nodes,
                edges,
                arrows,
                labels,
                edge_labels,
                None,
            ))
            .on_children_prepainted(move |bounds: Vec<Bounds<Pixels>>, _window, cx| {
                if let Some(bounds) = bounds.first().copied() {
                    let _ = weak.update(cx, |this, cx| this.measure(bounds, cx));
                }
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                    let Some(local) = this.local(event.position) else {
                        return;
                    };
                    let world = this.camera.viewport_to_world(this.viewport, local);
                    match model::hit_test(&this.topology, &this.positions, world) {
                        Some(index) => {
                            // A second click on the same node re-roots the
                            // neighbourhood, matching the web canvas.
                            if event.click_count >= 2
                                && let Some(node) = this.topology.nodes().get(index)
                            {
                                cx.emit(CanvasEvent::Expand {
                                    label: node.label.clone(),
                                });
                            }
                            this.selected = Some(index);
                            this.pan.end();
                        }
                        None => {
                            this.selected = None;
                            this.pan.begin(local);
                        }
                    }
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                let Some(local) = this.local(event.position) else {
                    return;
                };
                if let Some(delta) = this.pan.advance(local) {
                    this.camera.pan_by(delta);
                    cx.notify();
                    return;
                }
                let world = this.camera.viewport_to_world(this.viewport, local);
                let hovered = model::hit_test(&this.topology, &this.positions, world);
                if hovered != this.hovered {
                    this.hovered = hovered;
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _window, cx| {
                    if this.pan.is_active() {
                        this.pan.end();
                        cx.notify();
                    }
                }),
            )
            .on_scroll(cx.listener(|this, event: &ScrollEvent, _window, cx| {
                let Some(local) = this.local(event.position) else {
                    return;
                };
                let lines = match event.delta {
                    ScrollDelta::Lines(delta) => delta.y,
                    ScrollDelta::Pixels(delta) => f32::from(delta.y) / SCROLL_PIXELS_PER_LINE,
                };
                let factor = wheel_zoom_factor(lines);
                if (factor - 1.0).abs() > f32::EPSILON {
                    this.camera.zoom_at(this.viewport, local, factor);
                    cx.notify();
                }
            }))
    }
}

/// Neutral fills used when a node kind is not in the palette.
const DEFAULT_FILL: u32 = 0xf1f5f9;
const DEFAULT_STROKE: u32 = 0x94a3b8;

/// Fill and border of a node kind, read from the shared node palette.
///
/// Matching is by substring because the server serializes entity kinds from a
/// wider enum than the palette names, and a kind the palette does not know
/// must still draw.
fn palette(kind: &str) -> (u32, u32) {
    let kind = kind.to_ascii_lowercase();
    let slot = if kind.contains("func") || kind.contains("method") || kind.contains("call") {
        ("--node-callable-bg", "--node-callable-border")
    } else if kind.contains("trait") || kind.contains("interface") || kind.contains("contract") {
        ("--node-contract-bg", "--node-contract-border")
    } else if kind.contains("type")
        || kind.contains("class")
        || kind.contains("struct")
        || kind.contains("enum")
        || kind.contains("union")
    {
        ("--node-type-bg", "--node-type-border")
    } else if kind.contains("const")
        || kind.contains("field")
        || kind.contains("var")
        || kind.contains("value")
        || kind.contains("property")
    {
        ("--node-value-bg", "--node-value-border")
    } else if kind.contains("module")
        || kind.contains("file")
        || kind.contains("package")
        || kind.contains("namespace")
    {
        ("--node-container-bg", "--node-container-border")
    } else {
        return (DEFAULT_FILL, DEFAULT_STROKE);
    };
    (packed(slot.0, DEFAULT_FILL), packed(slot.1, DEFAULT_STROKE))
}

/// Packed `0xRRGGBB` value of a node palette token.
fn packed(name: &str, fallback: u32) -> u32 {
    design_tokens::packed(Section::Nodes, name).unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_separates_callables_from_types() {
        let (callable_fill, _) = palette("function");
        let (type_fill, _) = palette("struct");
        let (fallback, _) = palette("something_new");
        assert_ne!(callable_fill, type_fill);
        assert_eq!(fallback, DEFAULT_FILL);
    }

    #[test]
    fn palette_matches_on_substrings() {
        assert_eq!(palette("instance_method").0, palette("function").0);
        assert_eq!(palette("TypeAlias").0, palette("struct").0);
        assert_eq!(palette("module").0, palette("file").0);
    }
}
