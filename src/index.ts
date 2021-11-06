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

// TODO: implement
