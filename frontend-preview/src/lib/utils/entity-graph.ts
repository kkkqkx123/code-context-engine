/**
 * Entity to graph adapters.
 *
 * Converts entity scoped relationship payloads (call chains, class
 * inheritance) into the shared GraphElement model consumed by GraphCanvas.
 * Centralizing the mapping keeps the canvas, legend and filters on a single
 * source of truth and stops CallGraph / InheritanceTree from maintaining
 * bespoke renderers for data that is already graph shaped.
 */

import type { CallChainNode } from '$lib/api/search';
import type {
	ClassImplementationsResponse,
	ClassInheritanceResponse,
} from '$lib/api/entities';
import {
	edgeElementId,
	toElementEdge,
	toElementNode,
	type GraphElement,
} from './graph-style';
import type { GraphEdge, GraphNode } from '$lib/api/graph';

/** Relation value used for call chain edges. Maps to the call domain. */
const CALL_RELATION = 'call.direct';

/** Relation value for base / derived class edges. Maps to structural. */
const INHERITANCE_RELATION = 'inheritance';

/** Relation value for interface edges. Maps to structural. */
const IMPLEMENTATION_RELATION = 'implementation';

function callLocation(node: CallChainNode): string {
	return node.call_line === null || node.call_line === undefined
		? node.file_path
		: `${node.file_path}:${node.call_line}`;
}

/**
 * Convert a linear call chain into renderer elements.
 *
 * Nodes are deduplicated by function id while edges preserve the returned
 * order (consecutive pairs). Self loops and duplicate edges are dropped so
 * reordered or cyclic payloads cannot produce overlapping edges.
 */
export function callChainToElements(nodes: CallChainNode[]): GraphElement[] {
	const graphNodes = new Map<string, GraphNode>();
	for (const node of nodes) {
		if (!node.function_id || graphNodes.has(node.function_id)) continue;
		graphNodes.set(node.function_id, {
			id: node.function_id,
			label: node.function_name,
			kind: 'function',
			source_file: node.file_path,
			source_location: callLocation(node),
		});
	}

	const seenEdges = new Set<string>();
	const graphEdges: GraphEdge[] = [];
	for (let i = 0; i + 1 < nodes.length; i += 1) {
		const source = nodes[i].function_id;
		const target = nodes[i + 1].function_id;
		if (!source || !target || source === target) continue;
		const edge: GraphEdge = {
			source,
			target,
			relation: CALL_RELATION,
			confidence: 'extracted',
			domain: 'call',
		};
		const id = edgeElementId(edge);
		if (seenEdges.has(id)) continue;
		seenEdges.add(id);
		graphEdges.push(edge);
	}

	return [
		...[...graphNodes.values()].map(toElementNode),
		...graphEdges.map(toElementEdge),
	];
}

export interface InheritanceElementsInput {
	inheritance?: ClassInheritanceResponse | null;
	implementations?: ClassImplementationsResponse | null;
	/** Explicit center when neither payload carries an id. */
	fallbackId?: string | null;
	fallbackName?: string | null;
}

/**
 * Convert class inheritance / implementation payloads into a star shaped
 * element set centered on the inspected class.
 *
 * Edge direction follows extends / implements semantics: derived classes
 * point at their base, a class points at the interfaces it implements, and
 * implementing classes point at the inspected interface.
 */
export function inheritanceToElements(
	input: InheritanceElementsInput,
): GraphElement[] {
	const { inheritance, implementations, fallbackId, fallbackName } = input;
	const centerId =
		inheritance?.class_id ?? implementations?.class_id ?? fallbackId ?? null;
	if (!centerId) return [];
	const centerName =
		inheritance?.class_name ??
		implementations?.class_name ??
		fallbackName ??
		centerId;

	const graphNodes = new Map<string, GraphNode>();
	const addNode = (id: string, label: string, kind: string, file: string) => {
		if (!id || graphNodes.has(id)) return;
		graphNodes.set(id, {
			id,
			label,
			kind,
			source_file: file,
			source_location: file,
		});
	};

	addNode(centerId, centerName, 'class', '');

	const graphEdges: GraphEdge[] = [];
	const seenEdges = new Set<string>();
	const addEdge = (source: string, target: string, relation: string) => {
		if (!source || !target || source === target) return;
		const edge: GraphEdge = {
			source,
			target,
			relation,
			confidence: 'extracted',
			domain: 'structural',
		};
		const id = edgeElementId(edge);
		if (seenEdges.has(id)) return;
		seenEdges.add(id);
		graphEdges.push(edge);
	};

	for (const base of inheritance?.base_classes ?? []) {
		addNode(base.class_id, base.class_name, 'class', base.file_path);
		addEdge(centerId, base.class_id, INHERITANCE_RELATION);
	}
	for (const derived of inheritance?.derived_classes ?? []) {
		addNode(derived.class_id, derived.class_name, 'class', derived.file_path);
		addEdge(derived.class_id, centerId, INHERITANCE_RELATION);
	}
	for (const iface of implementations?.implemented_interfaces ?? []) {
		addNode(
			iface.interface_id,
			iface.interface_name,
			'interface',
			iface.file_path,
		);
		addEdge(centerId, iface.interface_id, IMPLEMENTATION_RELATION);
	}
	for (const impl of implementations?.implementing_classes ?? []) {
		addNode(impl.class_id, impl.class_name, 'class', impl.file_path);
		addEdge(impl.class_id, centerId, IMPLEMENTATION_RELATION);
	}

	return [
		...[...graphNodes.values()].map(toElementNode),
		...graphEdges.map(toElementEdge),
	];
}

/** Resolve the center id used for focus highlight in inheritance graphs. */
export function inheritanceFocusId(
	input: InheritanceElementsInput,
): string | null {
	return (
		input.inheritance?.class_id ??
		input.implementations?.class_id ??
		input.fallbackId ??
		null
	);
}
