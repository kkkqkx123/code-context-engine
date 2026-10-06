/**
 * Shared graph viewport zoom constants and helpers.
 *
 * Both graph pages (frontend and frontend-preview) use multiplicative
 * (geometric) button zoom so the perceived speed is uniform across the
 * whole zoom range, and share a single min/max range consistent with
 * wheel zooming.
 */

/** Absolute zoom bounds shared by wheel and button zooming. */
export const GRAPH_MIN_ZOOM = 0.05;
export const GRAPH_MAX_ZOOM = 5;

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
