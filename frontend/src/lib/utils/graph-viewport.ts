import type { Core } from 'cytoscape';

/**
 * Shared graph viewport zoom constants and helpers.
 *
 * The canvas uses multiplicative (geometric) button zoom so the perceived
 * speed is uniform across the whole zoom range, and one min/max range shared
 * by wheel and button zooming.
 */

/** Absolute zoom bounds shared by wheel and button zooming. The floor is low
 *  enough that `fit` can honour the extent of a fully loaded graph (labels
 *  included) instead of silently cropping it at the canvas border. */
export const GRAPH_MIN_ZOOM = 0.02;
export const GRAPH_MAX_ZOOM = 5;

/** Padding every fit uses, so labels and arrowheads never touch the border.
 *  Single source for the canvas, the page and the keyboard shortcuts. */
export const GRAPH_FIT_PADDING = 40;

/** Multiplicative step per button click: one click = x1.25 zoom in. */
export const GRAPH_ZOOM_STEP = 1.25;

export function clampZoom(level: number): number {
	return Math.min(GRAPH_MAX_ZOOM, Math.max(GRAPH_MIN_ZOOM, level));
}

/**
 * Next zoom level after one button click.
 * Positive `direction` zooms in, negative zooms out.
 */
export function steppedZoom(current: number, direction: number): number {
	const factor = direction >= 0 ? GRAPH_ZOOM_STEP : 1 / GRAPH_ZOOM_STEP;
	return clampZoom(current * factor);
}

/**
 * Imperative viewport operations on the Cytoscape core instance.
 * Kept as free functions taking `Core | null` so pages and panels can call
 * them without owning the instance and so they stay unit-testable.
 */

/** Export the full canvas as a PNG download. */
export function exportGraphPng(cy: Core | null): void {
	if (!cy) return;
	const png = cy.png({ full: true, scale: 2, bg: '#ffffff' });
	const link = document.createElement('a');
	link.href = png;
	link.download = 'graph.png';
	link.click();
}

/** Fit the whole graph into the viewport with padding. */
export function fitGraphViewport(cy: Core | null): void {
	cy?.fit(undefined, GRAPH_FIT_PADDING);
}

/** Zoom one stepped click around the viewport center. */
export function zoomGraphViewport(cy: Core | null, direction: number): void {
	if (!cy) return;
	cy.zoom({
		level: steppedZoom(cy.zoom(), direction),
		renderedPosition: { x: cy.width() / 2, y: cy.height() / 2 },
	});
}

/** Reset zoom to 1 and re-center the graph. */
export function resetGraphViewport(cy: Core | null): void {
	if (!cy) return;
	cy.zoom(1);
	cy.center();
}

/** Animate the viewport so the given node is centered. */
export function centerGraphOn(cy: Core | null, nodeId: string): void {
	if (!cy) return;
	const node = cy.getElementById(nodeId);
	if (node.length === 0) return;
	cy.animate(
		{ center: { eles: node }, zoom: Math.max(cy.zoom(), 1) },
		{ duration: 250 },
	);
}
