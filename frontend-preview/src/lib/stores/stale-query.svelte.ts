/**
 * Shared "epoch-aware query" state for graph explorer panels.
 *
 * Every page-local panel query (cycles, structural relations, module
 * relations) repeats the same pattern: fire a request, record the
 * relation_epoch from the response, derive a `stale` flag when the watch
 * service reports a newer epoch, and discard nothing on race — the latest
 * resolved response wins because each panel owns exactly one query slot.
 * This class centralizes that state machine so panels only supply the
 * loader function.
 */
import type { GraphActionResult } from '$lib/stores/graph';

export class StaleQuery<T extends { relation_epoch: number }> {
	data = $state<T | null>(null);
	loading = $state(false);
	error = $state<string | null>(null);
	/** Relation epoch the current data was built from (0 = never loaded). */
	epoch = $state(0);

	#watchEpoch: () => number | null;

	constructor(watchEpoch: () => number | null) {
		this.#watchEpoch = watchEpoch;
	}

	/** True when the relation index advanced past the epoch this data was built from. */
	get stale(): boolean {
		const watch = this.#watchEpoch();
		return watch !== null && this.epoch !== 0 && watch > this.epoch;
	}

	/**
	 * Run one graph action and record its result. Concurrent calls are
	 * ignored while a request is in flight, matching the previous
	 * per-panel guards. Returns whether the query succeeded.
	 */
	async run(load: () => Promise<GraphActionResult<T>>): Promise<boolean> {
		if (this.loading) return false;
		this.loading = true;
		this.error = null;
		try {
			const result = await load();
			if (result.ok) {
				this.data = result.value;
				this.epoch = result.value.relation_epoch;
			} else {
				this.error = result.error.message;
			}
			return result.ok;
		} finally {
			this.loading = false;
		}
	}

	/** Clear the result (used when the answer's subject changes). */
	reset(): void {
		this.data = null;
		this.error = null;
		this.epoch = 0;
	}
}
