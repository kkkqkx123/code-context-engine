/**
 * Error-code → human guidance mapping for dead-letter entries.
 * Shared between the ops dead-letter page and project summaries.
 */

export interface DeadLetterGuidance {
	/** Short human-readable cause description. */
	hint: string;
	/** Which row action should be highlighted. */
	primary: 'retry' | 'acknowledge';
}

const GUIDANCE: Record<string, DeadLetterGuidance> = {
	// Deterministic token-budget failure; the truncate-retry pass can repair it.
	LLM_TOKEN_LIMIT_EXCEEDED_ERROR: {
		hint: 'Embedding input exceeded the token budget; a truncated retry can repair it.',
		primary: 'retry',
	},
};

const DEFAULT_GUIDANCE: DeadLetterGuidance = {
	hint: 'Deterministic failure; retrying the same input will most likely fail again.',
	primary: 'acknowledge',
};

export function deadLetterGuidance(errorCode?: string | null): DeadLetterGuidance {
	if (errorCode && errorCode in GUIDANCE) return GUIDANCE[errorCode];
	return DEFAULT_GUIDANCE;
}
