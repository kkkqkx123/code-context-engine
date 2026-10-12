/**
 * Renderer element model: converts backend graph payloads into Cytoscape
 * element definitions (id/label/shape/size for nodes, domain/style data for
 * edges). Split from the presentation metadata module so the conversion
 * layer evolves independently of label and legend vocabulary.
 */
import type { GraphEdge, GraphNode } from '$lib/api/graph';
import {
	NODE_KINDS,
	normalizeNodeKind,
	edgeConfidence,
	edgeDomain,
	relationLabel,
	relationLineStyle,
	type EdgeConfidence,
	type NodeShape,
	type RelationDomain,
} from '$lib/utils/graph-domains';

/** Deterministic, collision-free renderer id for an edge. */
export function edgeElementId(
	edge: Pick<GraphEdge, 'source' | 'target' | 'relation'>,
): string {
	return `${edge.source}->${edge.target}@${edge.relation}`;
}

/** Renderer element for a backend node. */
export interface GraphElementNode {
	data: {
		id: string;
		label: string;
		/** Backend kind string, kept verbatim for the detail panel. */
		kind: string;
		/** Silhouette, carried as data so the stylesheet needs no per-kind rules. */
		shape: NodeShape;
		external: 0 | 1;
		sourceFile: string;
		sourceLocation: string;
	};
	/** Body size, sized to the label so long symbol names stay readable. */
	width: number;
	height: number;
}

/** Base node area in square pixels, and the area a node gains per label char.
 *  Holding area roughly constant keeps the layout's repulsion balanced, so a
 *  long symbol name widens its node instead of overlapping its neighbours. */
const NODE_BASE_AREA = 26 * 26;
const NODE_AREA_PER_CHAR = 34;
/** Label band height reserved below the node body. */
const NODE_LABEL_HEIGHT = 13;
const NODE_MIN_SIZE = 22;
const NODE_MAX_WIDTH = 150;

/** Renderer element for a backend edge. */
export interface GraphElementEdge {
	data: {
		id: string;
		source: string;
		target: string;
		relation: string;
		relationLabel: string;
		domain: RelationDomain;
		confidence: EdgeConfidence;
		lineStyle: string;
		/**
		 * Structural load from the backend: how strongly the relation type
		 * binds caller to callee, scaled by how many call sites reach the
		 * target. Drives edge width, so a heavily referenced edge reads as
		 * carrying more of the surrounding code than a one-off link.
		 */
		weight: number;
		/** 1 when the caller carries a `cfg` predicate, so the edge only
		 *  exists under that condition; 0 otherwise. Drawn faded so a
		 *  platform-specific edge is never mistaken for an unconditional one. */
		conditional: 0 | 1;
		/** The raw predicate, kept for detail panels. */
		cfgCondition: string | null;
	};
}

export type GraphElement = GraphElementNode | GraphElementEdge;

/**
 * Node body sized to its label.
 *
 * A fixed 26px body truncates every long symbol to an ellipsis, which defeats
 * the label being the primary way to read a graph. Growing the body with the
 * label keeps the name legible, and scaling both axes against a constant area
 * stops long names from shoving their neighbours around during layout.
 */
function nodeSize(label: string): { width: number; height: number } {
	const chars = Math.max((label ?? '').trim().length, 3);
	const width = Math.min(
		NODE_MAX_WIDTH,
		Math.max(NODE_MIN_SIZE, Math.round(Math.sqrt(chars * NODE_AREA_PER_CHAR))),
	);
	const height = Math.max(
		NODE_MIN_SIZE,
		Math.round((NODE_BASE_AREA + chars * NODE_AREA_PER_CHAR) / width),
	);
	return { width, height: height + NODE_LABEL_HEIGHT };
}

export function toElementNode(node: GraphNode): GraphElementNode {
	const meta = NODE_KINDS[normalizeNodeKind(node.kind)];
	const { width, height } = nodeSize(node.label);
	return {
		data: {
			id: node.id,
			label: node.label,
			kind: node.kind,
			shape: meta.shape,
			external: meta.external ? 1 : 0,
			sourceFile: node.source_file,
			sourceLocation: node.source_location,
		},
		width,
		height,
	};
}

/** Weight assumed when the backend omits it. The field carries a server-side
 *  default, so a payload without it is a single-call-site edge of ordinary
 *  binding; `mapData` clamps out-of-range values, and a missing one would
 *  render as no width at all. */
const DEFAULT_EDGE_WEIGHT = 1;

export function toElementEdge(edge: GraphEdge): GraphElementEdge {
	const domain = edgeDomain(edge);
	const confidence = edgeConfidence(edge.confidence);
	return {
		data: {
			id: edgeElementId(edge),
			source: edge.source,
			target: edge.target,
			relation: edge.relation,
			relationLabel: relationLabel(edge.relation),
			domain,
			confidence,
			lineStyle: relationLineStyle(domain),
			weight: edge.weight ?? DEFAULT_EDGE_WEIGHT,
			conditional: edge.cfg_condition ? 1 : 0,
			cfgCondition: edge.cfg_condition ?? null,
		},
	};
}
