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

/** Supported entity kinds in the codebase. */
export type NodeKind =
	| 'function'
	| 'method'
	| 'constructor'
	| 'class'
	| 'struct'
	| 'enum'
	| 'interface'
	| 'trait'
	| 'variable'
	| 'constant'
	| 'module'
	| 'package'
	| 'unknown';

/** Coarse grouping of the backend relation taxonomy.
 *  Mirrors the backend `relation_domain` classification; the authoritative
 *  value arrives on each edge as `domain`, and the string-based inference in
 *  `relationDomain` is only a fallback for mock/legacy data. */
export type RelationDomain =
	'call' | 'dependency' | 'structural' | 'reference' | 'template' | 'other';

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
		color: '#475569',
	},
};

export interface NodeKindMeta {
	kind: NodeKind;
	shape: 'round-rectangle' | 'rectangle' | 'diamond' | 'hexagon';
	description: string;
}

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
	class: { kind: 'class', shape: 'rectangle', description: 'Class definition' },
	struct: {
		kind: 'struct',
		shape: 'rectangle',
		description: 'Structure definition',
	},
	enum: { kind: 'enum', shape: 'rectangle', description: 'Enumeration' },
	interface: {
		kind: 'interface',
		shape: 'hexagon',
		description: 'Interface or protocol',
	},
	trait: { kind: 'trait', shape: 'hexagon', description: 'Trait or mixin' },
	variable: {
		kind: 'variable',
		shape: 'diamond',
		description: 'Variable or field',
	},
	constant: {
		kind: 'constant',
		shape: 'diamond',
		description: 'Constant or macro',
	},
	module: {
		kind: 'module',
		shape: 'diamond',
		description: 'Module or namespace',
	},
	package: {
		kind: 'package',
		shape: 'diamond',
		description: 'Package or workspace',
	},
	unknown: {
		kind: 'unknown',
		shape: 'diamond',
		description: 'Unknown entity type',
	},
};

/** Map a backend kind string to a known NodeKind, fallback to unknown. */
export function normalizeNodeKind(kind?: string | null): NodeKind {
	const value = (kind ?? '').trim().toLowerCase();
	if (!value) return 'unknown';
	return NODE_KINDS[value as NodeKind] ? (value as NodeKind) : 'unknown';
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

/** Map a raw backend relation string onto its presentation domain.
 *  Fallback inference only — live data should use the backend-provided
 *  `domain` field via `edgeDomain`. */
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
 *  inference for mock or legacy payloads that lack it. */
export function edgeDomain(edge: {
	relation: string;
	domain?: string | null;
}): RelationDomain {
	const value = (edge.domain ?? '').trim();
	if (
		value === 'call' ||
		value === 'dependency' ||
		value === 'structural' ||
		value === 'reference' ||
		value === 'template' ||
		value === 'other'
	) {
		return value;
	}
	return relationDomain(edge.relation);
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
 * Confidence values mirror the extraction pipeline. Ambiguous relations are
 * rendered as a warning because they are expected to be reviewed by a human;
 * external relations point outside the project into a dependency manifest.
 */
export type EdgeConfidence =
	'extracted' | 'inferred' | 'ambiguous' | 'external' | 'unknown';

export function edgeConfidence(confidence: string): EdgeConfidence {
	const value = (confidence ?? '').trim().toLowerCase();
	if (
		value === 'extracted' ||
		value === 'inferred' ||
		value === 'ambiguous' ||
		value === 'external'
	) {
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
	ambiguous: {
		confidence: 'ambiguous',
		label: 'Ambiguous',
		description: 'Relationship is uncertain and flagged for review',
		opacity: 0.85,
		color: '#8a6d00',
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

/** Node kinds that are rendered with a distinct silhouette. */
export type NodeShape = 'round-rectangle' | 'rectangle' | 'diamond' | 'hexagon';

export function nodeShape(kind: string): NodeShape {
	switch ((kind ?? '').trim().toLowerCase()) {
		case 'function':
		case 'method':
		case 'constructor':
			return 'round-rectangle';
		case 'class':
		case 'struct':
		case 'enum':
			return 'rectangle';
		case 'interface':
		case 'trait':
			return 'hexagon';
		default:
			return 'diamond';
	}
}

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
		kind: string;
		sourceFile: string;
		sourceLocation: string;
	};
}

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
		 * Traversal weight from the backend: the relation type's base
		 * confidence multiplied by how many call sites the caller uses to
		 * reach the target. Drives edge width, so a heavily repeated edge
		 * reads as structurally more load-bearing than a one-off call.
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

export function toElementNode(node: GraphNode): GraphElementNode {
	return {
		data: {
			id: node.id,
			label: node.label,
			kind: node.kind,
			sourceFile: node.source_file,
			sourceLocation: node.source_location,
		},
	};
}

/** Weight assumed when the backend omits it. The field carries a server-side
 *  default, so a payload without it is a single-call-site edge of neutral
 *  confidence; `mapData` clamps out-of-range values, and a missing one would
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

export function isNodeElement(
	element: GraphElement,
): element is GraphElementNode {
	return 'label' in element.data;
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

/** Weight range the edge-width mapping spans. The backend emits the relation
 *  type's base confidence (0.3 for a type reference up to 1.0 for a direct
 *  call) multiplied by the hotness factor `1 + log2(call_site_count)`, which is
 *  1.0 for a single call site. Anything at or beyond the upper bound is a
 *  hub edge many call sites converge on. */
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
			'text-max-width': '110px',
			'text-wrap': 'ellipsis',
			width: 26,
			height: 26,
			'overlay-opacity': 0,
		},
	},
	{ selector: 'node[kind = "class"]', style: { shape: 'rectangle' } },
	{ selector: 'node[kind = "struct"]', style: { shape: 'rectangle' } },
	{ selector: 'node[kind = "enum"]', style: { shape: 'rectangle' } },
	{ selector: 'node[kind = "function"]', style: { shape: 'round-rectangle' } },
	{ selector: 'node[kind = "method"]', style: { shape: 'round-rectangle' } },
	{ selector: 'node[kind = "interface"]', style: { shape: 'hexagon' } },
	{ selector: 'node[kind = "trait"]', style: { shape: 'hexagon' } },
	{
		selector: 'edge',
		style: {
			// Width encodes how load-bearing the edge is, not which relation
			// family it belongs to: colour and line style already carry the
			// domain, so a per-domain width would be a third encoding of the
			// same fact.
			width: `mapData(weight, ${WEIGHT_RANGE.min}, ${WEIGHT_RANGE.max}, ${WEIGHT_WIDTH.min}, ${WEIGHT_WIDTH.max})`,
			'line-color': RELATION_DOMAINS.other.color,
			'target-arrow-color': RELATION_DOMAINS.other.color,
			'target-arrow-shape': 'triangle',
			'arrow-scale': 0.8,
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
		selector: 'edge[confidence = "ambiguous"]',
		style: {
			opacity: CONFIDENCE_META.ambiguous.opacity,
			'line-color':
				CONFIDENCE_META.ambiguous.color ?? RELATION_DOMAINS.other.color,
			'target-arrow-color':
				CONFIDENCE_META.ambiguous.color ?? RELATION_DOMAINS.other.color,
		},
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
		selector: 'node.impact-transitive',
		style: {
			'border-color': '#2563eb',
			'border-style': 'dashed',
			'border-width': 2,
		},
	},
	{
		selector: 'edge.highlighted',
		style: {
			width: 3,
			opacity: 1,
			'line-color': '#e63600',
			'target-arrow-color': '#e63600',
		},
	},
];
