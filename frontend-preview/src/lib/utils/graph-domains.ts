/**
 * Graph presentation model: relation domains, node kinds and labels.
 *
 * Translates backend relation identifiers and entity kinds into presentation
 * concepts (domains, colors, line styles, silhouettes) and their human
 * labels. Keeping this separate from components means the legend, the
 * filters and the canvas all read from a single source of truth.
 *
 * Backend relation values are dotted strings produced by RelationType, for
 * example "call.direct", "dependency.import.named" or "inheritance".
 */

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

/** Human-readable domain label for an edge, e.g. "calls" / "types". */
export function domainLabel(edge: { relation: string }): string {
	return RELATION_DOMAINS[edgeDomain(edge)].label;
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
