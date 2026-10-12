/**
 * Small text helpers shared across pages.
 */

/** Split a comma-separated input into trimmed, non-empty items. */
export function parseList(value: string): string[] {
	return value
		.split(',')
		.map((item) => item.trim())
		.filter(Boolean);
}
