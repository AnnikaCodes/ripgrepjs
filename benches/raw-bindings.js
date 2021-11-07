/**
 * Benchmarks the raw bindings without abstractions.
 */

const {multithreadedSearchDirectory} = require('../dist/ripgrepjs.node');
const options = {
    afterContext: 0,
    beforeContext: 0,
    multilineSearch: false,
    invertMatch: false,
    includeLineNumbers: true,
    passthru: false,
    caseInsensitive: false,
    smartCase: true,
    dotMatchesNewline: false,
    greedySwap: false,
    ignoreWhitespace: false,
    unicode: true,
    octal: false,
    crlf: false,
    wordBoundariesOnly: false,
    numMatchesToBuffer: 50000,
    pattern: "fo+"
};

multithreadedSearchDirectory(options, process.argv.pop(), (matchedLines) => {
    for (const line of matchedLines) {
        console.log(line.matchedLines);
    }
});