/**
 * Dependency-free code highlighting helpers for the console.
 *
 * The console deliberately avoids adding a highlighter dependency (design
 * review Q4). Helpers return plain token data so CodeBlock.svelte can render
 * text nodes only — no `{@html}` and no injection risk from a source file.
 */

const KEYWORDS = new Set([
	'abstract',
	'and',
	'as',
	'assert',
	'async',
	'await',
	'break',
	'case',
	'catch',
	'class',
	'const',
	'continue',
	'crate',
	'declare',
	'default',
	'def',
	'defer',
	'del',
	'delete',
	'do',
	'dyn',
	'elif',
	'else',
	'enum',
	'except',
	'export',
	'extends',
	'extern',
	'false',
	'final',
	'finally',
	'fn',
	'for',
	'from',
	'func',
	'function',
	'global',
	'go',
	'if',
	'impl',
	'import',
	'in',
	'infer',
	'instanceof',
	'interface',
	'is',
	'keyof',
	'lambda',
	'let',
	'loop',
	'match',
	'mod',
	'module',
	'move',
	'mut',
	'namespace',
	'new',
	'none',
	'nonlocal',
	'not',
	'null',
	'or',
	'override',
	'package',
	'pass',
	'private',
	'protected',
	'pub',
	'public',
	'raise',
	'readonly',
	'ref',
	'return',
	'satisfies',
	'self',
	'static',
	'struct',
	'super',
	'switch',
	'this',
	'throw',
	'trait',
	'true',
	'try',
	'type',
	'typeof',
	'undefined',
	'unsafe',
	'use',
	'var',
	'void',
	'where',
	'while',
	'with',
	'yield',
]);

// Strings / comments / numbers / identifiers / whitespace / single char.
// Order matters: strings and comments must win over the catch-all branches.
const TOKEN_RE =
	/("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|`(?:[^`\\]|\\.)*`|\/\/[^\n]*|#[^\n]*|\b\d+(?:\.\d+)?\b|[A-Za-z_$][\w$]*|\s+|[^\s])/g;

export interface CodeToken {
	text: string;
	/** One of the `tok-*` classes, or '' for plain text. */
	cls: string;
}

/**
 * Split one source line into tokens the component renders as text nodes.
 * Returning data instead of HTML keeps the viewer free of `{@html}` (which the
 * project lint rules forbid) and therefore immune to injection from a file.
 */
export function tokenizeLine(line: string, terms: string[] = []): CodeToken[] {
	const needles = new Set(
		terms.map((term) => term.toLowerCase()).filter(Boolean),
	);
	const tokens: CodeToken[] = [];
	for (const match of line.matchAll(TOKEN_RE)) {
		const text = match[0];
		let cls = '';
		if (text.startsWith('//') || text.startsWith('#')) cls = 'tok-comment';
		else if (/^["'`]/.test(text)) cls = 'tok-string';
		else if (/^\d/.test(text)) cls = 'tok-number';
		else if (/^[A-Za-z_$][\w$]*$/.test(text) && needles.has(text.toLowerCase()))
			cls = 'tok-match';
		else if (KEYWORDS.has(text)) cls = 'tok-keyword';
		tokens.push({ text, cls });
	}
	return tokens.length > 0 ? tokens : [{ text: ' ', cls: '' }];
}

/** Split a natural-language query into terms worth highlighting. */
export function splitTerms(query: string): string[] {
	const stop = new Set([
		'the',
		'and',
		'for',
		'with',
		'from',
		'that',
		'this',
		'how',
		'what',
		'where',
		'why',
		'are',
		'was',
		'were',
		'into',
		'代码',
		'的',
		'在',
		'和',
		'如何',
		'什么',
		'哪里',
	]);
	return [
		...new Set(
			query
				.toLowerCase()
				.split(/[^\p{L}\p{N}_$]+/u)
				.filter((term) => term.length >= 2 && !stop.has(term)),
		),
	].slice(0, 8);
}

/** Keep in sync with `language_of` in cce-server handlers/file.rs. */
export function languageFromPath(path: string): string {
	const ext = path.split('.').pop()?.toLowerCase() ?? '';
	const languages: Record<string, string> = {
		rs: 'rust',
		py: 'python',
		ts: 'typescript',
		tsx: 'tsx',
		js: 'javascript',
		mjs: 'javascript',
		cjs: 'javascript',
		jsx: 'jsx',
		go: 'go',
		java: 'java',
		c: 'c',
		h: 'c',
		cc: 'cpp',
		cpp: 'cpp',
		cxx: 'cpp',
		hpp: 'cpp',
		hh: 'cpp',
		cs: 'csharp',
		rb: 'ruby',
		php: 'php',
		kt: 'kotlin',
		kts: 'kotlin',
		scala: 'scala',
		dart: 'dart',
		lua: 'lua',
		sh: 'bash',
		bash: 'bash',
		zsh: 'bash',
		html: 'html',
		htm: 'html',
		css: 'css',
		vue: 'vue',
		svelte: 'svelte',
		md: 'markdown',
		markdown: 'markdown',
		json: 'json',
		toml: 'toml',
		yaml: 'yaml',
		yml: 'yaml',
		xml: 'xml',
		sql: 'sql',
		proto: 'protobuf',
	};
	return languages[ext] ?? 'text';
}
