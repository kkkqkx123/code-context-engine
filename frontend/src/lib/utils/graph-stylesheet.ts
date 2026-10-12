/**
 * Renderer stylesheet rules for the graph canvas.
 *
 * Typed as `StylesheetStyle[]` so every entry's `style` block is validated
 * against Cytoscape's Css.Node / Css.Edge / Css.Core union — typos in property
 * names like `'line-colour'` or mis-keyed values surface at compile time.
 *
 * Colors reference the shared RELATION_DOMAINS metadata so the canvas follows
 * the configured palette automatically. Rules are ordered from specific to
 * general so later selectors only act as fallbacks.
 */
import type { StylesheetStyle } from 'cytoscape';
import {
	CONFIDENCE_META,
	RELATION_DOMAINS,
	type NodeShape,
} from '$lib/utils/graph-domains';

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
