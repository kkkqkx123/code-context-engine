/**
 * Index State Store.
 * Rune-based replacement of the previous `writable<IndexState>` + `indexActions`
 * object. State is `$state` fields; actions are methods on the same instance.
 */
import { errorMessage } from '../utils/errors';
import {
	indexApi,
	projectApi,
	type IncrementalIndexRequest,
} from '../api/index';
import { projectStore } from './project.svelte';

export interface IndexState {
	isIndexing: boolean;
	progress: number;
	currentFile: string;
	phase: 'scan' | 'parse' | 'embed' | 'store' | null;
	errorCount: number;
	lastError: string | null;
}

class IndexStore {
	isIndexing = $state(false);
	progress = $state(0);
	currentFile = $state('');
	phase = $state<IndexState['phase']>(null);
	errorCount = $state(0);
	lastError = $state<string | null>(null);

	/**
	 * Index a project from its registered configuration (root path, extensions,
	 * excludes, ignore rules). The project record is the single source of truth
	 * for what gets indexed.
	 */
	async startProjectIndex(projectId: string) {
		this.isIndexing = true;
		this.progress = 0;
		this.phase = 'scan';

		try {
			await projectApi.indexProject(projectId);
			await projectStore.loadProjects();
			this.isIndexing = false;
			this.progress = 100;
			this.phase = null;
		} catch (error) {
			this.isIndexing = false;
			this.errorCount += 1;
			this.lastError = errorMessage(error);
		}
	}

	async startIncrementalIndex(data: IncrementalIndexRequest) {
		this.isIndexing = true;
		this.progress = 0;
		this.phase = 'scan';

		try {
			await indexApi.incrementalIndex(data);
			this.isIndexing = false;
			this.progress = 100;
			this.phase = null;
		} catch (error) {
			this.isIndexing = false;
			this.errorCount += 1;
			this.lastError = errorMessage(error);
		}
	}

	stopIndex() {
		this.isIndexing = false;
		this.phase = null;
	}

	updateProgress(
		progress: number,
		currentFile: string,
		phase: IndexState['phase'],
	) {
		this.progress = progress;
		this.currentFile = currentFile;
		this.phase = phase;
	}
}

export const indexStore = new IndexStore();
