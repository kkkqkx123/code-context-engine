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

import type { GraphEdge, GraphNode } from '$lib/api/graph';

/** Coarse grouping of the backend relation taxonomy. */
export type RelationDomain = 'call' | 'dependency' | 'structural' | 'reference' | 'other';

export interface RelationDomainMeta {
	domain: RelationDomain;
	label: string;
	description: string;
	/** Stroke color used for edges of this domain. */
	color: string;
}

export const RELATION_DOMAINS: Record<RelationDomain, RelationDomainMeta> = {
	call: {
		domain: 'call',
		label: 'Call',
		description: 'Function and method invocation',
		color: '#2563eb'
	},
	dependency: {
		domain: 'dependency',
		label: 'Dependency',
		description: 'Imports, includes and module references',
		color: '#737373'
	},
	structural: {
		domain: 'structural',
		label: 'Structural',
		description: 'Inheritance, implementation and containment',
		color: '#8b5cf6'
	},
	reference: {
		domain: 'reference',
		label: 'Reference',
		description: 'Type, field and template references',
		color: '#0d9488'
	},
	other: {
		domain: 'other',
		label: 'Other',
		description: 'Unclassified relation',
		color: '#d4d4d4'
	}
};

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
	'mixin'
]);

/** Reference-domain relation values shared by reference and template relations. */
const REFERENCE_RELATIONS = new Set([
	'type_reference',
	'field_access',
	'contains.element',
	'reference.template',
	'parameter.binding',
	'callback.event'
]);

/** Map a raw backend relation string onto its presentation domain. */
export function relationDomain(relation: string): RelationDomain {
	const value = (relation ?? '').trim();
	if (!value) return 'other';
	if (value.startsWith('call.')) return 'call';
	if (value.startsWith('dependency.')) return 'dependency';
	if (STRUCTURAL_RELATIONS.has(value)) return 'structural';
	if (REFERENCE_RELATIONS.has(value)) return 'reference';
	return 'other';
}

/**
 * Short human label for a relation value. Drops the domain prefix so the
 * remaining segments stay readable in legends and detail panels.
 */
export function relationLabel(relation: string): string {
	const value = (relation ?? '').trim();
	if (!value) return 'unknown';
	if (value.startsWith('call.')) return value.slice('call.'.length).replace(/\./g, ' ');
	if (value.startsWith('dependency.')) return value.slice('dependency.'.length).replace(/\./g, ' ');
	return value.replace(/[._]/g, ' ');
}

/** Edge dash pattern per domain. Calls and structural edges read as solid. */
export function relationLineStyle(domain: RelationDomain): 'solid' | 'dashed' | 'dotted' {
	if (domain === 'dependency') return 'dashed';
	if (domain === 'reference') return 'dotted';
	return 'solid';
}

/**
 * Confidence values mirror the extraction pipeline. Ambiguous relations are
 * rendered as a warning because they are expected to be reviewed by a human.
 */
export type EdgeConfidence = 'extracted' | 'inferred' | 'ambiguous' | 'unknown';

export function edgeConfidence(confidence: string): EdgeConfidence {
	const value = (confidence ?? '').trim().toLowerCase();
	if (value === 'extracted' || value === 'inferred' || value === 'ambiguous') return value;
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
		color: null
	},
	inferred: {
		confidence: 'inferred',
		label: 'Inferred',
		description: 'Relationship is a reasonable deduction',
		opacity: 0.55,
		color: null
	},
	ambiguous: {
		confidence: 'ambiguous',
		label: 'Ambiguous',
		description: 'Relationship is uncertain and flagged for review',
		opacity: 0.85,
		color: '#8a6d00'
	},
	unknown: {
		confidence: 'unknown',
		label: 'Unknown',
		description: 'Confidence was not reported',
		opacity: 0.75,
		color: '#8a8a8a'
	}
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
export function edgeElementId(edge: Pick<GraphEdge, 'source' | 'target' | 'relation'>): string {
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
			sourceLocation: node.source_location
		}
	};
}

export function toElementEdge(edge: GraphEdge): GraphElementEdge {
	const domain = relationDomain(edge.relation);
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
			lineStyle: relationLineStyle(domain)
		}
	};
}

export function isNodeElement(element: GraphElement): element is GraphElementNode {
	return 'label' in element.data;
}

/**
 * Renderer stylesheet rules.
 *
 * Colors reference the shared design tokens so the canvas follows the console
 * theme automatically. Rules are ordered from specific to general so later
 * selectors only act as fallbacks.
 */
export const graphStylesheet: Array<Record<string, unknown>> = [
	{
		selector: 'node',
		style: {
			'background-color': '#fafafa',
			'border-color': '#0a0a0a',
			'border-width': 1.5,
			'label': 'data(label)',
			'font-family': 'Space Mono, monospace',
			'font-size': 9,
			'color': '#0a0a0a',
			'text-valign': 'bottom',
			'text-halign': 'center',
			'text-margin-y': 4,
			'text-max-width': '110px',
			'text-wrap': 'ellipsis',
			'width': 26,
			'height': 26,
			'overlay-opacity': 0
		}
	},
	{ selector: 'node[kind = "class"]', style: { 'shape': 'rectangle' } },
	{ selector: 'node[kind = "struct"]', style: { 'shape': 'rectangle' } },
	{ selector: 'node[kind = "enum"]', style: { 'shape': 'rectangle' } },
	{ selector: 'node[kind = "function"]', style: { 'shape': 'round-rectangle' } },
	{ selector: 'node[kind = "method"]', style: { 'shape': 'round-rectangle' } },
	{ selector: 'node[kind = "interface"]', style: { 'shape': 'hexagon' } },
	{ selector: 'node[kind = "trait"]', style: { 'shape': 'hexagon' } },
	{
		selector: 'edge',
		style: {
			'width': 1.4,
			'line-color': RELATION_DOMAINS.other.color,
			'target-arrow-color': RELATION_DOMAINS.other.color,
			'target-arrow-shape': 'triangle',
			'arrow-scale': 0.8,
			'curve-style': 'bezier',
			'opacity': 0.8,
			'overlay-opacity': 0
		}
	},
	{ selector: 'edge[domain = "call"]', style: { 'line-color': RELATION_DOMAINS.call.color, 'target-arrow-color': RELATION_DOMAINS.call.color, 'width': 1.8 } },
	{ selector: 'edge[domain = "dependency"]', style: { 'line-color': RELATION_DOMAINS.dependency.color, 'target-arrow-color': RELATION_DOMAINS.dependency.color, 'line-style': 'dashed' } },
	{ selector: 'edge[domain = "structural"]', style: { 'line-color': RELATION_DOMAINS.structural.color, 'target-arrow-color': RELATION_DOMAINS.structural.color, 'width': 2.2 } },
	{ selector: 'edge[domain = "reference"]', style: { 'line-color': RELATION_DOMAINS.reference.color, 'target-arrow-color': RELATION_DOMAINS.reference.color, 'line-style': 'dotted' } },
	{ selector: 'edge[confidence = "inferred"]', style: { 'opacity': CONFIDENCE_META.inferred.opacity } },
	{ selector: 'edge[confidence = "ambiguous"]', style: { 'opacity': CONFIDENCE_META.ambiguous.opacity, 'line-color': CONFIDENCE_META.ambiguous.color, 'target-arrow-color': CONFIDENCE_META.ambiguous.color } },
	{
		selector: 'node.focus',
		style: { 'border-width': 3, 'border-color': '#e63600', 'background-color': '#fef2f0' }
	},
	{
		selector: 'node.dimmed',
		style: { 'opacity': 0.25 }
	},
	{
		selector: 'edge.dimmed',
		style: { 'opacity': 0.12 }
	},
	{
		selector: 'node.impact',
		style: { 'border-color': '#2563eb', 'border-width': 3, 'background-color': '#eff6ff' }
	},
	{
		selector: 'node.impact-transitive',
		style: { 'border-color': '#2563eb', 'border-style': 'dashed', 'border-width': 2 }
	},
	{
		selector: 'edge.highlighted',
		style: { 'width': 3, 'opacity': 1, 'line-color': '#e63600', 'target-arrow-color': '#e63600' }
	}
];
