/**
 * Benchmarks of the EventEmitter abstraction for the Rust bindings.
 */

const {searchWithEventEmitter} = require('..');

const filePath = process.argv.pop();
const emitter = searchWithEventEmitter({pattern: 'fo+'}, filePath);
emitter.on('result', result => console.log(result.matchedLines[0]));
