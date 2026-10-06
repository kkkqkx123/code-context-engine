/**
 * Graph presentation model
 *
 * Translates backend relation identifiers and entity kinds into presentation
 * concepts (domains, colors, line styles) and into renderer-specific style
 * rules. Keeping this separate from components means the legend, the filters
 * and the canvas all read from a single source of truth.
 *
 * Backend relation values are dotted strings produced by RelationType, for
 * example "call.direct", "dependency.import.named" or "inheritance".
 */

import type { StylesheetStyle } from 'cytoscape';
import type { GraphEdge, GraphNode } from '$lib/api/graph';

/**
 * Entity kinds that earn a silhouette of their own. Every other backend kind
 * collapses onto one of these through `normalizeNodeKind`.
 */
export type NodeKind =
	| 'class'
	| 'struct'
	| 'enum'
	| 'union'
	| 'type_alias'
	| 'interface'
	| 'trait'
	| 'trait_impl'
	| 'function'
	| 'method'
	| 'constructor'
	| 'destructor'
	| 'operator'
	| 'variable'
	| 'constant'
	| 'module'
	| 'package'
	| 'external'
	| 'unknown';

/** Coarse grouping of the backend relation taxonomy.
 *  Mirrors `RelationType::domain`; the authoritative
 *  value arrives on each edge as `domain`, and the string-based inference in
 *  `relationDomain` is only a fallback for mock/legacy data. */
export type RelationDomain =
	'call' | 'dependency' | 'structural' | 'reference' | 'template' | 'other';

/**
 * Every domain, in display order. The single source of the domain vocabulary:
 * the store's default selection, the filter panel's row order and the canvas's
 * hide-selector all derive from this list, so adding a domain to
 * `RELATION_DOMAINS` makes it filterable everywhere without a second edit.
 */
export const RELATION_DOMAIN_ORDER: RelationDomain[] = [
	'call',
	'dependency',
	'structural',
	'reference',
	'template',
	'other',
];

/** Whether a string names a known domain. */
export function isRelationDomain(value: string): value is RelationDomain {
	return (RELATION_DOMAIN_ORDER as string[]).includes(value);
}

export interface RelationDomainMeta {
	domain: RelationDomain;
	label: string;
	description: string;
	/** Stroke color used for edges of this domain. */
	color: string;
}

/** Presentational metadata for every relation domain. A single source of truth
 *  shared by the stylesheet generator, the legend and the filter panel. */
export const RELATION_DOMAINS: Record<RelationDomain, RelationDomainMeta> = {
	call: {
		domain: 'call',
		label: 'Call',
		description: 'Function / method / constructor invocation',
		color: '#2563eb',
	},
	dependency: {
		domain: 'dependency',
		label: 'Dependency',
		description: 'Import / include / module dependency',
		color: '#737373',
	},
	structural: {
		domain: 'structural',
		label: 'Structural',
		description: 'Inheritance / implementation / containment',
		color: '#8b5cf6',
	},
	reference: {
		domain: 'reference',
		label: 'Reference',
		description: 'Type reference / field access',
		color: '#0d9488',
	},
	template: {
		domain: 'template',
		label: 'Template',
		description: 'Template / markup element relations',
		color: '#d97706',
	},
	other: {
		domain: 'other',
		label: 'Other',
		description: 'Unclassified relations, including plugin-provided ones',
		color: '#be123c',
	},
};

export interface NodeKindMeta {
	kind: NodeKind;
	shape: NodeShape;
	/** Whether the node sits outside the indexed project. */
	external?: boolean;
	description: string;
}

/**
 * Silhouette for every entity kind the graph can carry.
 *
 * Shape encodes how a definition behaves, not which language it came from:
 * a callable body is a rounded rectangle, a type definition a plain
 * rectangle, an interface or trait a hexagon, a value an ellipse, a
 * container an octagon, and anything outside the project a diamond.
 * Unclassified entities use a pentagon so they never share a silhouette
 * with a classified value. The backend reports 50-odd kinds; each is
 * listed here so no kind silently falls back to an arbitrary default.
 */
export const NODE_KINDS: Record<NodeKind, NodeKindMeta> = {
	function: {
		kind: 'function',
		shape: 'round-rectangle',
		description: 'Free function',
	},
	method: {
		kind: 'method',
		shape: 'round-rectangle',
		description: 'Method or member function',
	},
	constructor: {
		kind: 'constructor',
		shape: 'round-rectangle',
		description: 'Constructor or initializer',
	},
	destructor: {
		kind: 'destructor',
		shape: 'round-rectangle',
		description: 'Destructor or finalizer',
	},
	operator: {
		kind: 'operator',
		shape: 'round-rectangle',
		description: 'Operator overload',
	},
	class: { kind: 'class', shape: 'rectangle', description: 'Class definition' },
	struct: {
		kind: 'struct',
		shape: 'rectangle',
		description: 'Structure definition',
	},
	enum: { kind: 'enum', shape: 'rectangle', description: 'Enumeration' },
	union: {
		kind: 'union',
		shape: 'rectangle',
		description: 'Union or variant definition',
	},
	type_alias: {
		kind: 'type_alias',
		shape: 'rectangle',
		description: 'Type alias or typedef',
	},
	interface: {
		kind: 'interface',
		shape: 'hexagon',
		description: 'Interface or protocol',
	},
	trait: { kind: 'trait', shape: 'hexagon', description: 'Trait or mixin' },
	trait_impl: {
		kind: 'trait_impl',
		shape: 'hexagon',
		description: 'Trait or protocol implementation',
	},
	variable: {
		kind: 'variable',
		shape: 'ellipse',
		description: 'Variable, field or property',
	},
	constant: {
		kind: 'constant',
		shape: 'ellipse',
		description: 'Constant or macro',
	},
	module: {
		kind: 'module',
		shape: 'octagon',
		description: 'Module or namespace',
	},
	package: {
		kind: 'package',
		shape: 'octagon',
		description: 'Package or workspace',
	},
	external: {
		kind: 'external',
		shape: 'diamond',
		external: true,
		description: 'Symbol resolved outside the indexed project',
	},
	unknown: {
		kind: 'unknown',
		shape: 'pentagon',
		description: 'Entity of a kind the graph does not classify',
	},
};

/**
 * One legend entry per silhouette, naming what the shape means rather than
 * which backend kinds map onto it. Keeping it here means the legend can never
 * drift from the shapes the stylesheet actually applies.
 */
export const NODE_SHAPE_LEGEND: {
	shape: NodeShape;
	label: string;
	external?: boolean;
}[] = [
	{ shape: 'rectangle', label: 'Type definition' },
	{ shape: 'round-rectangle', label: 'Callable' },
	{ shape: 'hexagon', label: 'Contract' },
	{ shape: 'ellipse', label: 'Value' },
	{ shape: 'octagon', label: 'Container' },
	{ shape: 'diamond', label: 'Outside the project', external: true },
	{ shape: 'pentagon', label: 'Unclassified' },
];

/**
 * Backend kinds that carry no silhouette of their own, collapsed onto the kind
 * they most resemble. Grouping mirrors the backend's `EntityKind::domain`, so
 * the whole backend vocabulary lands somewhere intentional instead of falling
 * through to an arbitrary default. Kinds already present in `NODE_KINDS` are
 * absent here by design.
 */
const ENTITY_KIND_GROUPS: Record<string, NodeKind> = {
	// Code domain
	inherent_impl: 'class',
	enum_variant: 'constant',
	annotation: 'trait',
	macro: 'function',
	field: 'variable',
	property: 'variable',
	// Module domain
	namespace: 'module',
	import: 'module',
	require: 'module',
	include: 'module',
	export: 'module',
	// Template domain
	element: 'variable',
	attribute: 'variable',
	expression: 'variable',
	component: 'class',
	template: 'class',
	directive: 'function',
	control_flow: 'unknown',
	animation: 'unknown',
	binding: 'variable',
	action: 'function',
	at_rule: 'unknown',
	event_handler: 'function',
	// Style domain
	style_rule: 'function',
	style_selector: 'variable',
	style_property: 'constant',
	keyframe: 'class',
	// Test domain
	test_suite: 'class',
	test_case: 'function',
	test_hook: 'function',
	assertion: 'function',
	mock: 'trait',
	// Inline payloads with no meaning of their own
	script_content: 'unknown',
	style_content: 'unknown',
	embedded_block: 'unknown',
};

/**
 * Map a backend entity kind onto a kind with a dedicated silhouette.
 *
 * Kinds that carry no useful shape distinction of their own collapse onto a
 * related one rather than falling through to a default, so the whole backend
 * vocabulary lands somewhere intentional.
 */
export function normalizeNodeKind(kind?: string | null): NodeKind {
	const value = (kind ?? '').trim().toLowerCase();
	if (!value) return 'unknown';
	if (NODE_KINDS[value as NodeKind]) return value as NodeKind;
	return ENTITY_KIND_GROUPS[value] ?? 'unknown';
}

/** Exact structural relation values (these carry no dotted prefix). */
const STRUCTURAL_RELATIONS = new Set([
	'inheritance',
	'implementation',
	'trait_bound',
	'trait_inheritance',
	'protocol_implementation',
	'contains',
	'impl_association',
	'embedding',
	'mixin',
]);

/** Reference-domain relation values shared by reference and template relations. */
const REFERENCE_RELATIONS = new Set(['type_reference', 'field_access']);

/** Template/markup relation values (backend template domain). */
const TEMPLATE_RELATIONS = new Set([
	'contains.element',
	'reference.template',
	'parameter.binding',
	'callback.event',
]);

/**
 * Map a raw backend relation string onto its presentation domain.
 *
 * This is a protocol fallback for payloads that arrive without the backend's
 * authoritative `domain` field, not a second source of classification. The
 * authoritative mapping lives in `RelationType::domain` on the server; keep
 * the two tables in step when a relation type is added there.
 */
export function relationDomain(relation: string): RelationDomain {
	const value = (relation ?? '').trim();
	if (!value) return 'other';
	if (value.startsWith('call.')) return 'call';
	if (value.startsWith('dependency.')) return 'dependency';
	if (STRUCTURAL_RELATIONS.has(value)) return 'structural';
	if (TEMPLATE_RELATIONS.has(value)) return 'template';
	if (REFERENCE_RELATIONS.has(value)) return 'reference';
	return 'other';
}

/** Resolve an edge's domain: trust the backend field, fall back to string
 *  inference only when the payload omits or misspells it. */
export function edgeDomain(edge: {
	relation: string;
	domain?: string | null;
}): RelationDomain {
	const value = (edge.domain ?? '').trim().toLowerCase();
	return isRelationDomain(value) ? value : relationDomain(edge.relation);
}

/**
 * Short human label for a relation value. Drops the domain prefix so the
 * remaining segments stay readable in legends and detail panels.
 */
export function relationLabel(relation: string): string {
	const value = (relation ?? '').trim();
	if (!value) return 'unknown';
	if (value.startsWith('call.'))
		return value.slice('call.'.length).replace(/\./g, ' ');
	if (value.startsWith('dependency.'))
		return value.slice('dependency.'.length).replace(/\./g, ' ');
	return value.replace(/[._]/g, ' ');
}

/** Edge dash pattern per domain. Calls, structural and template edges read as solid. */
export function relationLineStyle(
	domain: RelationDomain,
): 'solid' | 'dashed' | 'dotted' {
	if (domain === 'dependency') return 'dashed';
	if (domain === 'reference') return 'dotted';
	return 'solid';
}

/**
 * Confidence values mirror the extraction pipeline's `Confidence` enum:
 * relationships stated in the source, relationships deduced during
 * resolution, and relationships pointing outside the indexed project.
 */
export type EdgeConfidence = 'extracted' | 'inferred' | 'external' | 'unknown';

export function edgeConfidence(confidence: string): EdgeConfidence {
	const value = (confidence ?? '').trim().toLowerCase();
	if (value === 'extracted' || value === 'inferred' || value === 'external') {
		return value;
	}
	return 'unknown';
}

export interface ConfidenceMeta {
	confidence: EdgeConfidence;
	label: string;
	description: string;
	opacity: number;
	color: string | null;
}

export const CONFIDENCE_META: Record<EdgeConfidence, ConfidenceMeta> = {
	extracted: {
		confidence: 'extracted',
		label: 'Extracted',
		description: 'Relationship is explicitly stated in the source',
		opacity: 1,
		color: null,
	},
	inferred: {
		confidence: 'inferred',
		label: 'Inferred',
		description: 'Relationship is a reasonable deduction',
		opacity: 0.55,
		color: null,
	},
	external: {
		confidence: 'external',
		label: 'External',
		description: 'Relationship points outside the project into a dependency',
		opacity: 0.6,
		color: '#6b7280',
	},
	unknown: {
		confidence: 'unknown',
		label: 'Unknown',
		description: 'Confidence was not reported',
		opacity: 0.75,
		color: '#8a8a8a',
	},
};

/** Silhouettes available to entity nodes. */
export type NodeShape =
	| 'round-rectangle'
	| 'rectangle'
	| 'diamond'
	| 'hexagon'
	| 'ellipse'
	| 'octagon'
	| 'pentagon';

/** Fill and border per silhouette so shape groups stay distinguishable
 *  without relying on outline alone. External keeps its own grey dashed
 *  treatment and is absent here by design. */
export const NODE_SHAPE_STYLE: Record<
	Exclude<NodeShape, 'diamond'>,
	{ background: string; border: string }
> = {
	'round-rectangle': { background: '#eff6ff', border: '#1e40af' },
	rectangle: { background: '#f5f3ff', border: '#5b21b6' },
	hexagon: { background: '#ecfdf5', border: '#065f46' },
	ellipse: { background: '#fffbeb', border: '#92400e' },
	octagon: { background: '#f8fafc', border: '#334155' },
	pentagon: { background: '#fafafa', border: '#0a0a0a' },
};

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

/**
 * Renderer stylesheet rules.
 *
 * Typed as `StylesheetStyle[]` so every entry's `style` block is validated
 * against Cytoscape's Css.Node / Css.Edge / Css.Core union — typos in property
 * names like `'line-colour'` or mis-keyed values surface at compile time.
 *
 * Colors reference the shared RELATION_DOMAINS metadata so the canvas follows
 * the configured palette automatically. Rules are ordered from specific to
 * general so later selectors only act as fallbacks.
 */
/** Opacity applied to edges whose caller is behind a `cfg` predicate.
 *  Faded rather than dashed, because line style already encodes the relation
 *  domain and a second dashed variant would be ambiguous. */
const CONDITIONAL_EDGE_OPACITY = 0.45;

/** Weight range the edge-width mapping spans. The backend emits how strongly
 *  the relation type binds the two endpoints (0.3 for a type reference up to
 *  1.0 for a direct call) multiplied by the reach factor
 *  `1 + log2(call_site_count)`, which is 1.0 for a single call site. Anything
 *  at or beyond the upper bound is a hub edge many call sites converge on. */
const WEIGHT_RANGE = { min: 0.3, max: 4 };
const WEIGHT_WIDTH = { min: 1, max: 4.5 };

export const graphStylesheet: StylesheetStyle[] = [
	{
		selector: 'node',
		style: {
			'background-color': '#fafafa',
			'border-color': '#0a0a0a',
			'border-width': 1.5,
			label: 'data(label)',
			'font-family': 'Space Mono, monospace',
			'font-size': 9,
			color: '#0a0a0a',
			'text-valign': 'bottom',
			'text-halign': 'center',
			'text-margin-y': 4,
			'text-max-width': '150px',
			'text-wrap': 'ellipsis',
			// Sizing comes from the element so long symbol names stay readable.
			width: 'data(width)',
			height: 'data(height)',
			shape: 'round-rectangle',
			'overlay-opacity': 0,
		},
	},
	{
		// Silhouette per kind group. A `data(shape)` mapper is runtime-only and
		// the typings reject it, so each shape stays a literal override rule.
		// Fills come from NODE_SHAPE_STYLE so legend and canvas share values.
		selector: 'node[shape = "rectangle"]',
		style: {
			shape: 'rectangle',
			'background-color': NODE_SHAPE_STYLE.rectangle.background,
			'border-color': NODE_SHAPE_STYLE.rectangle.border,
		},
	},
	{
		selector: 'node[shape = "round-rectangle"]',
		style: {
			shape: 'round-rectangle',
			'background-color': NODE_SHAPE_STYLE['round-rectangle'].background,
			'border-color': NODE_SHAPE_STYLE['round-rectangle'].border,
		},
	},
	{
		selector: 'node[shape = "hexagon"]',
		style: {
			shape: 'hexagon',
			'background-color': NODE_SHAPE_STYLE.hexagon.background,
			'border-color': NODE_SHAPE_STYLE.hexagon.border,
		},
	},
	{
		selector: 'node[shape = "ellipse"]',
		style: {
			shape: 'ellipse',
			'background-color': NODE_SHAPE_STYLE.ellipse.background,
			'border-color': NODE_SHAPE_STYLE.ellipse.border,
		},
	},
	{
		selector: 'node[shape = "octagon"]',
		style: {
			shape: 'octagon',
			'background-color': NODE_SHAPE_STYLE.octagon.background,
			'border-color': NODE_SHAPE_STYLE.octagon.border,
		},
	},
	{
		selector: 'node[shape = "pentagon"]',
		style: {
			shape: 'pentagon',
			'background-color': NODE_SHAPE_STYLE.pentagon.background,
			'border-color': NODE_SHAPE_STYLE.pentagon.border,
		},
	},
	{
		selector: 'node[shape = "diamond"]',
		style: { shape: 'diamond' },
	},
	{
		// Nodes resolved outside the project carry no source location, so a
		// dashed outline distinguishes them from entities defined in-tree.
		// Diamond is reserved for this case, never for in-tree values.
		selector: 'node[external = 1]',
		style: {
			'border-style': 'dashed',
			'background-color': '#f5f5f5',
			'border-color': '#525252',
		},
	},
	{
		selector: 'edge',
		style: {
			// Width encodes how much the surrounding code leans on the link,
			// not which relation family it belongs to: colour and line style
			// already carry the domain, so a per-domain width would be a third
			// encoding of the same fact.
			width: `mapData(weight, ${WEIGHT_RANGE.min}, ${WEIGHT_RANGE.max}, ${WEIGHT_WIDTH.min}, ${WEIGHT_WIDTH.max})`,
			'line-color': RELATION_DOMAINS.other.color,
			'target-arrow-color': RELATION_DOMAINS.other.color,
			'target-arrow-shape': 'triangle',
			'arrow-scale': 1,
			'curve-style': 'bezier',
			opacity: 0.8,
			'overlay-opacity': 0,
			'font-family': 'Space Mono, monospace',
			'font-size': 8,
			color: '#475569',
			'text-background-color': '#ffffff',
			'text-background-opacity': 0.85,
			'text-background-padding': '1px',
			'text-rotation': 'autorotate',
		},
	},
	{
		selector: 'edge.show-label',
		style: {
			label: 'data(relationLabel)',
		},
	},
	{
		selector: 'edge[domain = "call"]',
		style: {
			'line-color': RELATION_DOMAINS.call.color,
			'target-arrow-color': RELATION_DOMAINS.call.color,
		},
	},
	{
		selector: 'edge[domain = "dependency"]',
		style: {
			'line-color': RELATION_DOMAINS.dependency.color,
			'target-arrow-color': RELATION_DOMAINS.dependency.color,
			'line-style': 'dashed',
		},
	},
	{
		selector: 'edge[domain = "structural"]',
		style: {
			'line-color': RELATION_DOMAINS.structural.color,
			'target-arrow-color': RELATION_DOMAINS.structural.color,
		},
	},
	{
		selector: 'edge[domain = "reference"]',
		style: {
			'line-color': RELATION_DOMAINS.reference.color,
			'target-arrow-color': RELATION_DOMAINS.reference.color,
			'line-style': 'dotted',
		},
	},
	{
		selector: 'edge[domain = "template"]',
		style: {
			'line-color': RELATION_DOMAINS.template.color,
			'target-arrow-color': RELATION_DOMAINS.template.color,
			'target-arrow-shape': 'circle',
		},
	},
	{
		selector: 'edge[domain = "other"]',
		style: {
			'line-color': RELATION_DOMAINS.other.color,
			'target-arrow-color': RELATION_DOMAINS.other.color,
			'target-arrow-shape': 'vee',
		},
	},
	{
		selector: 'edge[conditional = 1]',
		style: { opacity: CONDITIONAL_EDGE_OPACITY },
	},
	{
		selector: 'edge[confidence = "inferred"]',
		style: { opacity: CONFIDENCE_META.inferred.opacity },
	},
	{
		// Deductions link entities the source never names directly, so they are
		// drawn faintly rather than at full strength.
		selector: 'edge[confidence = "external"]',
		style: { opacity: CONFIDENCE_META.external.opacity },
	},
	{
		// A guarded deduction carries both attenuations, so the combined
		// selectors below must win over either single-axis rule.
		selector: 'edge[conditional = 1][confidence = "inferred"]',
		style: { opacity: 0.25 },
	},
	{
		selector: 'edge[conditional = 1][confidence = "external"]',
		style: { opacity: 0.27 },
	},
	{
		selector: 'edge[conditional = 1][confidence = "unknown"]',
		style: { opacity: 0.34 },
	},
	{
		selector: 'node.dimmed',
		style: { opacity: 0.25 },
	},
	{
		selector: 'edge.dimmed',
		style: { opacity: 0.12 },
	},
	{
		selector: 'node.impact',
		style: {
			'border-color': '#2563eb',
			'border-width': 3,
			'background-color': '#eff6ff',
		},
	},
	{
		// Solid outline on purpose: dashed is reserved for external nodes.
		selector: 'node.impact-transitive',
		style: {
			'border-color': '#2563eb',
			'border-style': 'solid',
			'border-width': 2,
			'background-color': '#f8fafc',
		},
	},
	{
		selector: 'node.selected',
		style: {
			'border-width': 3,
			'border-color': '#0a0a0a',
		},
	},
	{
		selector: 'edge.incident',
		style: { opacity: 1 },
	},
	{
		selector: 'node.focus',
		style: {
			'border-width': 3,
			'border-color': '#e63600',
			'background-color': '#fef2f0',
		},
	},
	{
		// Focus wins over impact: the seed stays readable while the impact
		// fill still shows through on directly affected nodes.
		selector: 'node.focus.impact',
		style: {
			'border-width': 3,
			'border-color': '#e63600',
			'background-color': '#eff6ff',
		},
	},
	{
		selector: 'node.focus.impact-transitive',
		style: {
			'border-width': 3,
			'border-color': '#e63600',
			'border-style': 'solid',
		},
	},
];
