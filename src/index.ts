/**
 * Abstractions around the Rust bindings (found in lib.rs).
 *
 * Specifically, this file contains the following:
 * - Sane defaults to configure the ripgrep libraries
 * - A wrapper providing AsyncIterator support for ripgrepjs. This lets you do things like this:
 * ```ts
 * const ripgrep = require('ripgrepjs')
 * for await (const match of ripgrep.search({pattern: 'fo+'}, 'path/to/folder')) {
 *     // do something
 * }
 * ```
 *
 * @author Annika L.
 */

import {EventEmitter} from 'events';
// TODO: figure out if an async iterator is possible
// TODO: Support buffering it all in Rust to make it faster (or maybe only buffer n entries in a Vec?)
export interface RipgrepOptions {
	afterContext: number;
	beforeContext: number;
	multilineSearch: boolean;
	invertMatch: boolean;
	includeLineNumbers: boolean;
	passthru: boolean;
	heapLimit?: number;
	caseInsensitive: boolean;
	smartCase: boolean;
	dotMatchesNewline: boolean;
	greedySwap: boolean;
	ignoreWhitespace: boolean;
	unicode: boolean;
	octal: boolean;
	crlf: boolean;
	wordBoundariesOnly: boolean;
	numMatchesToBuffer: number;
	pattern: string;
}

export interface RipgrepResult {
	lines: string[];
	lineNumber?: number;
}

const multithreadedSearchDirectory = require('./ripgrepjs.node').multithreadedSearchDirectory as (
	options: RipgrepOptions,
	path: string,
	onResult: (result: RipgrepResult) => void
) => void;

/**
 * Searches a directory with multithreading, returning results through an EventEmitter.
 *
 * @returns An EventEmitter whose 'result' event will emit RipgrepResult objects.
 */
export function searchWithEventEmitter(options: Partial<RipgrepOptions> & {pattern: string}, path: string) {
	const rustOptions: RipgrepOptions = {
		afterContext: options.afterContext || 0,
		beforeContext: options.beforeContext || 0,
		multilineSearch: options.multilineSearch || false,
		invertMatch: options.invertMatch || false,
		includeLineNumbers: options.includeLineNumbers || true,
		passthru: options.passthru || false,
		caseInsensitive: options.caseInsensitive || false,
		smartCase: options.smartCase || false,
		dotMatchesNewline: options.dotMatchesNewline || false,
		greedySwap: options.greedySwap || false,
		ignoreWhitespace: options.ignoreWhitespace || false,
		unicode: options.unicode || true,
		octal: options.octal ?? false,
		crlf: options.crlf || false,
		wordBoundariesOnly: options.wordBoundariesOnly || false,
		numMatchesToBuffer: options.numMatchesToBuffer || 100000,
		pattern: options.pattern,
	};
	if (typeof options.heapLimit === 'number') rustOptions.heapLimit = options.heapLimit;

	const emitter = new EventEmitter();
	multithreadedSearchDirectory(rustOptions, path, result => {
		emitter.emit('result', result);
	});
	return emitter;
}
