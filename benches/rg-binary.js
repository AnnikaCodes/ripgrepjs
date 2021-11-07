/**
 * Benchmarks running the binary `rg` program in a child process and processing its results.
 */

const {spawn} = require('child_process');

const filePath = process.argv.pop();

const rg = spawn('rg', ['-uuu', 'fo+', filePath]);
rg.stdout.on('data', data => console.log(data));
