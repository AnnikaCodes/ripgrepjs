//! Binds Rust ripgrep libraries to Node.js
//!
//! This library has two principal goals:
//! - to support the use of BurntSushi's `grep` crate from within Node.js
//! - to simplify the `grep` crate's API to make it more user-friendly

use std::{convert::Infallible, path::Path, str::Utf8Error, sync::Arc};

use grep::{
    matcher::LineTerminator,
    regex::{RegexMatcher, RegexMatcherBuilder},
    searcher::{BinaryDetection, Searcher, SearcherBuilder, SinkError, SinkFinish, SinkMatch},
};
use neon::{macro_internal::runtime::string, prelude::*, result::Throw};
use rayon::prelude::*;

#[derive(Debug)]
enum RipgrepjsError {
    JavaScript(neon::result::Throw),
    StringConversion(Utf8Error),
    Regex(grep::regex::Error),
    IO(std::io::Error),
    Sink(String),
}

impl From<neon::result::Throw> for RipgrepjsError {
    fn from(error: neon::result::Throw) -> Self {
        RipgrepjsError::JavaScript(error)
    }
}
impl From<Utf8Error> for RipgrepjsError {
    fn from(error: Utf8Error) -> Self {
        RipgrepjsError::StringConversion(error)
    }
}
impl From<Infallible> for RipgrepjsError {
    fn from(_: Infallible) -> Self {
        unreachable!()
    }
}
impl From<std::io::Error> for RipgrepjsError {
    fn from(error: std::io::Error) -> Self {
        RipgrepjsError::IO(error)
    }
}
impl From<grep::regex::Error> for RipgrepjsError {
    fn from(error: grep::regex::Error) -> Self {
        RipgrepjsError::Regex(error)
    }
}

impl SinkError for RipgrepjsError {
    fn error_message<T: std::fmt::Display>(message: T) -> Self {
        RipgrepjsError::Sink(format!("{}", message))
    }

    fn error_io(err: std::io::Error) -> Self {
        RipgrepjsError::IO(err)
    }
}
/// Options for building a searcher
pub struct SearcherOptions {
    pub line_terminator: Option<u8>,
    pub invert_match: bool,
    pub include_line_numbers: bool,
    pub multiline_search: bool,
    pub after_context: usize,
    pub before_context: usize,
    pub passthru: bool,
    pub heap_limit: Option<usize>,
}

impl SearcherOptions {
    /// Generates a ripgrep Seacher from an options struct.
    ///
    /// This abstracts away the builder pattern, which doesn't work well across the FFI boundary.
    fn to_searcher(&self) -> Searcher {
        let mut builder = SearcherBuilder::new();

        if let Some(term) = self.line_terminator {
            builder.line_terminator(LineTerminator::byte(term));
        }

        builder.invert_match(self.invert_match);
        builder.line_number(self.include_line_numbers);
        builder.multi_line(self.multiline_search);
        builder.after_context(self.after_context);
        builder.before_context(self.before_context);
        builder.passthru(self.passthru);
        builder.heap_limit(self.heap_limit);

        builder.build()
    }
}

pub struct MatcherOptions<'a> {
    pub case_insensitive: bool,
    pub smart_case: bool,
    pub multi_line: bool,
    pub dot_matches_new_line: bool,
    pub greedy_swap: bool,
    pub ignore_whitespace: bool,
    pub unicode: bool,
    pub octal: bool,
    pub line_terminator: Option<u8>,
    pub crlf: bool,
    pub word_boundaries_only: bool,

    pub pattern: &'a str,
}

impl<'a> MatcherOptions<'a> {
    /// Generates a ripgrep Matcher from an options struct.
    ///
    /// This abstracts away the builder pattern, which doesn't work well across the FFI boundary.
    fn to_matcher(&self) -> Result<RegexMatcher, RipgrepjsError> {
        let mut builder = RegexMatcherBuilder::new();

        builder.case_insensitive(self.case_insensitive);
        builder.case_smart(self.smart_case);
        builder.multi_line(self.multi_line);
        builder.dot_matches_new_line(self.dot_matches_new_line);
        builder.swap_greed(self.greedy_swap);
        builder.ignore_whitespace(self.ignore_whitespace);
        builder.unicode(self.unicode);
        builder.octal(self.octal);
        builder.line_terminator(self.line_terminator);
        builder.crlf(self.crlf);
        builder.word(self.word_boundaries_only);

        Ok(builder.build(self.pattern)?)
    }
}

/// Sink that executes a JavaScript callback on each match
///
/// TODO: buffer matches for better perf?
struct JSCallbackSink {
    on_match: Arc<Root<JsFunction>>,
    // Sends a match to the calling thread so that it can be passed to the JavaScript callback
    channel: Channel,
    inner_buf: Vec<(Vec<Result<String, Utf8Error>>, Option<u64>)>,
}

impl JSCallbackSink {
    /// on_match JS function signature: `(results: {matchedLines: string[], lineNumber?: number}) => void;`
    ///
    /// `matchedLines` is an array of lines that matchsed the search pattern.
    /// It should have length 1 unless multiline searching is enabled.
    fn new(on_match: Arc<Root<JsFunction>>, channel: Channel) -> Self {
        Self {
            channel,
            on_match,
            inner_buf: vec![],
        }
    }
}

impl<'a> grep::searcher::Sink for JSCallbackSink {
    type Error = RipgrepjsError;

    fn matched(&mut self, _: &Searcher, matched: &SinkMatch) -> Result<bool, Self::Error> {
        let line_number = matched.line_number();
        // TODO: perf improvements possible here?
        let lines_iter = matched
            .lines()
            .map(|line| match std::str::from_utf8(line) {
                Ok(s) => Ok(s.to_string()),
                Err(e) => Err(e),
            })
            .collect::<Vec<_>>();
        self.inner_buf.push((lines_iter, line_number));

        Ok(true)
    }

    fn finish(&mut self, _: &Searcher, _: &SinkFinish) -> Result<(), Self::Error> {
        let callback = self.on_match.clone();

        let mut buf = vec![];
        std::mem::swap(&mut buf, &mut self.inner_buf);
        self.channel.send(move |mut context| {
            let js_matches = context.empty_array();
            let mut i = 0;
            while let Some((lines, line_number)) = buf.pop() {
                let js_lines = context.empty_array();
                // TODO: Support providing the filename
                let js_match_object = context.empty_object();
                for (idx, line) in lines.iter().enumerate() {
                    match line {
                        Ok(line_string) => {
                            let js_line = context.string(line_string);
                            js_lines.set(&mut context, idx as u32, js_line)?;
                        }
                        Err(_) => {
                            // We can't throw an error: https://github.com/neon-bindings/neon/issues/819
                            // We'll just put a `null` in the array instead for now
                            let null = context.null();
                            js_lines.set(&mut context, idx as u32, null)?;
                        }
                    };
                }
                js_match_object.set(&mut context, "matchedLines", js_lines)?;
                if let Some(line_number) = line_number {
                    let n = context.number(line_number as f64);
                    js_match_object.set(&mut context, "lineNumber", n)?;
                }

                js_matches.set(&mut context, i, js_match_object)?;
                i += 1;
            }
            let null = context.null();
            callback
                .to_inner(&mut context)
                .call(&mut context, null, vec![js_matches])?;
            Ok(())
        });
        Ok(())
    }
}

/// Searches a file with a `JsFunction` callback

/// Searches a directory with a `JsFunction` callback
///
/// Parallelized with Rayon.
fn search_directory_with_rayon<P>(
    searcher_opts: SearcherOptions,
    matcher_opts: MatcherOptions,
    directory: P,
    callback: Root<JsFunction>,
    js_context: &mut FunctionContext,
    to_buffer: usize,
) -> Result<(), RipgrepjsError>
where
    P: AsRef<Path>,
{
    let matcher = matcher_opts.to_matcher()?;
    let callback = Arc::new(callback);
    let channel = js_context.channel();
    walkdir::WalkDir::new(directory)
        .into_iter()
        .collect::<Vec<_>>()
        .par_iter()
        .try_for_each_init(
            || {
                (
                    searcher_opts.to_searcher(),
                    JSCallbackSink::new(callback.clone(), channel.clone()),
                )
            },
            |(searcher, sink), entry| -> Result<(), RipgrepjsError> {
                if let Ok(entry) = entry {
                    if entry.file_type().is_file() {
                        searcher.search_path(&matcher, entry.path(), sink).unwrap()
                    }
                }
                Ok(())
            },
        )?;
    channel.send(move |mut cx| {
        let null = cx.null();
        callback.to_inner(&mut cx).call(&mut cx, null, vec![null])?;
        Ok(())
    });
    Ok(())
}

/// helper to get ints from a JS obj
fn get_int_from_js_object<'a>(
    obj: Handle<JsObject>,
    cx: &mut impl Context<'a>,
    key: &str,
) -> Result<usize, Throw> {
    match obj.get(cx, key) {
        Ok(item) => Ok(item.downcast_or_throw::<JsNumber, _>(cx)?.value(cx) as usize),
        Err(e) => Err(e),
    }
}

fn get_possible_int_from_js_object<'a>(
    obj: Handle<JsObject>,
    cx: &mut impl Context<'a>,
    key: &str,
) -> Option<usize> {
    match obj.get(cx, key) {
        Ok(item) => Some(item.downcast::<JsNumber, _>(cx).ok()?.value(cx) as usize),
        Err(_) => None,
    }
}

fn get_bool_from_js_object<'a>(
    obj: Handle<JsObject>,
    cx: &mut impl Context<'a>,
    key: &str,
) -> Result<bool, Throw> {
    match obj.get(cx, key) {
        Ok(item) => Ok(item.downcast_or_throw::<JsBoolean, _>(cx)?.value(cx)),
        Err(e) => Err(e),
    }
}

fn get_string_from_js_object<'a>(
    obj: Handle<JsObject>,
    cx: &mut impl Context<'a>,
    key: &str,
) -> Result<String, Throw> {
    match obj.get(cx, key) {
        Ok(item) => Ok(item.downcast_or_throw::<JsString, _>(cx)?.value(cx)),
        Err(e) => Err(e),
    }
}

/// JS function signature: (
///     searcherOptions: {
///         afterContext: number,
///         beforeContext: number,
///         multilineSearch: boolean,
///         invertMatch: boolean,
///         includeLineNumbers: boolean,
///         passthru: boolean,
///         heapLimit?: number,
///         caseInsensitive: boolean,
///         smartCase: boolean,
///         dotMatchesNewline: boolean,
///         greedySwap: boolean,
///         ignoreWhitespace: boolean,
///         unicode: boolean,
///         octal: boolean,
///         crlf: boolean,
///         wordBoudariesOnly: boolean,
///         numMatchesToBuffer: number,
///         pattern: string,
///     },
///     path: string,
///     callback: (results: {matchedLines: string[], lineNumber?: number}) => void,
/// ) => void;
fn multithreaded_search_directory(mut cx: FunctionContext) -> JsResult<JsUndefined> {
    let options = cx.argument::<JsObject>(0)?;
    let path = cx.argument::<JsString>(1)?.value(&mut cx);
    let callback = cx.argument::<JsFunction>(2)?;

    // TODO: make this a macro?
    let searcher_opts = SearcherOptions {
        line_terminator: None, // TODO: implement
        after_context: get_int_from_js_object(options, &mut cx, "afterContext")?,
        before_context: get_int_from_js_object(options, &mut cx, "beforeContext")?,
        multiline_search: get_bool_from_js_object(options, &mut cx, "multilineSearch")?,
        invert_match: get_bool_from_js_object(options, &mut cx, "invertMatch")?,
        include_line_numbers: get_bool_from_js_object(options, &mut cx, "includeLineNumbers")?,
        passthru: get_bool_from_js_object(options, &mut cx, "passthru")?,
        heap_limit: get_possible_int_from_js_object(options, &mut cx, "heapLimit"),
    };
    let pattern = get_string_from_js_object(options, &mut cx, "pattern")?;
    let matcher_opts = MatcherOptions {
        case_insensitive: get_bool_from_js_object(options, &mut cx, "caseInsensitive")?,
        smart_case: get_bool_from_js_object(options, &mut cx, "smartCase")?,
        multi_line: searcher_opts.multiline_search,
        dot_matches_new_line: get_bool_from_js_object(options, &mut cx, "dotMatchesNewline")?,
        greedy_swap: get_bool_from_js_object(options, &mut cx, "greedySwap")?,
        ignore_whitespace: get_bool_from_js_object(options, &mut cx, "ignoreWhitespace")?,
        unicode: get_bool_from_js_object(options, &mut cx, "unicode")?,
        octal: get_bool_from_js_object(options, &mut cx, "octal")?,
        line_terminator: searcher_opts.line_terminator,
        crlf: get_bool_from_js_object(options, &mut cx, "crlf")?,
        word_boundaries_only: get_bool_from_js_object(options, &mut cx, "wordBoundariesOnly")?,
        pattern: pattern.as_str(),
    };

    let to_buffer = get_int_from_js_object(options, &mut cx, "numMatchesToBuffer")?;
    if let Err(e) = search_directory_with_rayon(
        searcher_opts,
        matcher_opts,
        path,
        callback.root(&mut cx),
        &mut cx,
        to_buffer,
    ) {
        cx.throw_error(format!("Rust Error: {:?}", e))?;
    }

    Ok(cx.undefined())
}

#[neon::main]
fn main(mut cx: ModuleContext) -> NeonResult<()> {
    cx.export_function(
        "multithreadedSearchDirectory",
        multithreaded_search_directory,
    )
}
