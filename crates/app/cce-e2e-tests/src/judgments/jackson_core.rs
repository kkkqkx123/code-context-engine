//! Range-based relevance judgments for the jackson-core review fixture.
//!
//! The fixture `fixtures/java/review/jackson-core` is the upstream Jackson
//! streaming parser library (larger than ripgrep), so the judgment set covers
//! its full public API surface and internal engine packages. 45 benchmark
//! queries, mirroring the ripgrep calibration:
//! - G1: Qualified symbol queries (10) - symbol tokens in natural-language
//!   structure (e.g. `nextToken in JsonParser`), Strong only
//! - G2: Semantic queries (20) - Strong + Related, single-entity ranges
//! - FZ: Fuzzy queries (15) - lexical perturbations of G1 sources
//!
//! All line ranges verified against
//! `fixtures/java/review/jackson-core/src/main/java/tools/jackson/core/`.
//! Each range is confined to a single method/class definition to avoid
//! chunking boundary issues. Callers/callees and format-specific overrides
//! are judged Related, not Strong.

use crate::bench_data::{QueryType, RelevanceJudgment, RelevanceLevel, SourceRange};

fn strong(
    id: &str,
    query_text: &str,
    query_type: QueryType,
    file: &str,
    start_line: usize,
    end_line: usize,
) -> RelevanceJudgment {
    RelevanceJudgment {
        id: id.into(),
        query_text: query_text.into(),
        query_type,
        fuzzy_subtype: None,
        relevant_ranges: vec![(
            SourceRange {
                file: file.into(),
                start_line,
                end_line,
            },
            RelevanceLevel::Strong,
        )],
    }
}

fn range(
    file: &str,
    start_line: usize,
    end_line: usize,
    level: RelevanceLevel,
) -> (SourceRange, RelevanceLevel) {
    (
        SourceRange {
            file: file.into(),
            start_line,
            end_line,
        },
        level,
    )
}

fn fuzzy_strong(
    id: &str,
    query_text: &str,
    subtype: &str,
    file: &str,
    start_line: usize,
    end_line: usize,
) -> RelevanceJudgment {
    RelevanceJudgment {
        id: id.into(),
        query_text: query_text.into(),
        query_type: QueryType::Fuzzy,
        fuzzy_subtype: Some(subtype.into()),
        relevant_ranges: vec![range(file, start_line, end_line, RelevanceLevel::Strong)],
    }
}

pub fn jackson_core_relevance_judgments() -> Vec<RelevanceJudgment> {
    use QueryType::{Qualified, Semantic};
    use RelevanceLevel as RL;

    // Core stream API
    let p = "src/main/java/tools/jackson/core/JsonParser.java";
    let g = "src/main/java/tools/jackson/core/JsonGenerator.java";
    let tok = "src/main/java/tools/jackson/core/JsonToken.java";
    let ptr = "src/main/java/tools/jackson/core/JsonPointer.java";
    let tsf = "src/main/java/tools/jackson/core/TokenStreamFactory.java";
    let pmb = "src/main/java/tools/jackson/core/base/ParserMinimalBase.java";
    // json package (default JSON backend)
    let rf = "src/main/java/tools/jackson/core/json/ReaderBasedJsonParser.java";
    let wg = "src/main/java/tools/jackson/core/json/WriterBasedJsonGenerator.java";
    let fact = "src/main/java/tools/jackson/core/json/JsonFactory.java";
    let rctx = "src/main/java/tools/jackson/core/json/JsonReadContext.java";
    let dup = "src/main/java/tools/jackson/core/json/DupDetector.java";
    // sym package (name canonicalization)
    let bqc = "src/main/java/tools/jackson/core/sym/ByteQuadsCanonicalizer.java";
    let ctn = "src/main/java/tools/jackson/core/sym/CharsToNameCanonicalizer.java";
    // io package (number/text conversion)
    let ni = "src/main/java/tools/jackson/core/io/NumberInput.java";
    let no = "src/main/java/tools/jackson/core/io/NumberOutput.java";
    let jse = "src/main/java/tools/jackson/core/io/JsonStringEncoder.java";
    // util package (buffers, pooling, pretty printing)
    let tb = "src/main/java/tools/jackson/core/util/TextBuffer.java";
    let br = "src/main/java/tools/jackson/core/util/BufferRecycler.java";
    let rp = "src/main/java/tools/jackson/core/util/RecyclerPool.java";
    let dpp = "src/main/java/tools/jackson/core/util/DefaultPrettyPrinter.java";
    let seq = "src/main/java/tools/jackson/core/util/JsonParserSequence.java";
    let del = "src/main/java/tools/jackson/core/util/JsonParserDelegate.java";
    // constraints
    let src_ = "src/main/java/tools/jackson/core/StreamReadConstraints.java";

    vec![
        // ==================== G1: Qualified Symbol Queries (10 total) ====================
        // G1Q1: JsonParser::nextToken - core stream-reading method
        strong(
            "G1Q1",
            "nextToken method in JsonParser",
            Qualified,
            p,
            433,
            433,
        ),
        // G1Q2: JsonGenerator::writeString - core stream-writing method
        strong(
            "G1Q2",
            "writeString method in JsonGenerator",
            Qualified,
            g,
            705,
            705,
        ),
        // G1Q3: JsonPointer::compile - parses a JSON Pointer expression
        strong(
            "G1Q3",
            "compile method in JsonPointer",
            Qualified,
            ptr,
            211,
            227,
        ),
        // G1Q4: TokenStreamFactory::createParser - factory entry for parsers
        strong(
            "G1Q4",
            "createParser method in TokenStreamFactory",
            Qualified,
            tsf,
            601,
            601,
        ),
        // G1Q5: JsonFactory::createNonBlockingByteArrayParser - async parser entry
        strong(
            "G1Q5",
            "createNonBlockingByteArrayParser method in JsonFactory",
            Qualified,
            fact,
            351,
            351,
        ),
        // G1Q6: ReaderBasedJsonParser::getString - current text value access
        strong(
            "G1Q6",
            "getString method in ReaderBasedJsonParser",
            Qualified,
            rf,
            302,
            312,
        ),
        // G1Q7: WriterBasedJsonGenerator::writeName - object property name writer
        strong(
            "G1Q7",
            "writeName method in WriterBasedJsonGenerator",
            Qualified,
            wg,
            148,
            159,
        ),
        // G1Q8: ByteQuadsCanonicalizer::findName - quad-based symbol lookup
        strong(
            "G1Q8",
            "findName method in ByteQuadsCanonicalizer",
            Qualified,
            bqc,
            664,
            682,
        ),
        // G1Q9: TextBuffer::contentsAsString - aggregated buffered text accessor
        strong(
            "G1Q9",
            "contentsAsString method in TextBuffer",
            Qualified,
            tb,
            505,
            525,
        ),
        // G1Q10: NumberInput::parseInt(char[]) - fast integer decode from chars
        strong(
            "G1Q10",
            "parseInt method with char array in NumberInput",
            Qualified,
            ni,
            39,
            79,
        ),
        // ==================== G2: Semantic Queries (20 total) ====================
        // G2Q1: advance the stream; nextValue is the caller-facing variant
        RelevanceJudgment {
            id: "G2Q1".into(),
            query_text: "advance the parse stream to the next JSON token".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                range(p, 433, 433, RL::Strong),
                range(p, 456, 456, RL::Related),
            ],
        },
        // G2Q2: object/array value boundaries
        RelevanceJudgment {
            id: "G2Q2".into(),
            query_text: "begin writing a JSON object value and close it afterwards".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                range(g, 452, 452, RL::Strong),
                range(g, 518, 518, RL::Related),
            ],
        },
        // G2Q3: base64 binary read
        RelevanceJudgment {
            id: "G2Q3".into(),
            query_text: "decode a base64 text value read from the stream into binary data".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                range(p, 1411, 1412, RL::Strong),
                range(p, 1399, 1399, RL::Related),
            ],
        },
        // G2Q4: pointer appendProperty is primary, appendIndex a sibling variant
        RelevanceJudgment {
            id: "G2Q4".into(),
            query_text: "append a property name segment to an existing JSON pointer".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                range(ptr, 466, 487, RL::Strong),
                range(ptr, 491, 508, RL::Related),
            ],
        },
        // G2Q5: duplicate detection during parsing
        RelevanceJudgment {
            id: "G2Q5".into(),
            query_text: "detect a duplicate property name while reading a JSON object".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                range(dup, 81, 96, RL::Strong),
                range(rctx, 103, 106, RL::Related),
            ],
        },
        // G2Q6: child context creation tracks nesting depth
        RelevanceJudgment {
            id: "G2Q6".into(),
            query_text: "push a child object context to track nesting while reading".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                range(rctx, 148, 165, RL::Strong),
                range(rctx, 137, 146, RL::Related),
            ],
        },
        // G2Q7: parser sequence flattening and switching
        RelevanceJudgment {
            id: "G2Q7".into(),
            query_text:
                "traverse multiple parsers in sequence and switch to the next when one is exhausted"
                    .into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                range(seq, 128, 148, RL::Strong),
                range(seq, 80, 100, RL::Related),
            ],
        },
        // G2Q8: delegate pattern forwarding
        RelevanceJudgment {
            id: "G2Q8".into(),
            query_text: "forward parser method calls to an underlying delegate parser".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                range(del, 150, 151, RL::Strong),
                range(del, 116, 118, RL::Related),
            ],
        },
        // G2Q9: buffer recycling from a pool
        RelevanceJudgment {
            id: "G2Q9".into(),
            query_text: "acquire a recyclable buffer from the pool and release it back".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                range(br, 158, 168, RL::Strong),
                range(rp, 157, 164, RL::Related),
            ],
        },
        // G2Q10: text buffer concatenation
        RelevanceJudgment {
            id: "G2Q10".into(),
            query_text: "append copied character segments into the parser text buffer".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                range(tb, 262, 285, RL::Strong),
                range(tb, 287, 307, RL::Related),
            ],
        },
        // G2Q11: JSON string escaping on write
        RelevanceJudgment {
            id: "G2Q11".into(),
            query_text: "escape special characters when quoting a JSON string".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                range(jse, 151, 192, RL::Strong),
                range(jse, 195, 298, RL::Related),
            ],
        },
        // G2Q12: UTF-8 encoded escaped output
        RelevanceJudgment {
            id: "G2Q12".into(),
            query_text: "encode a text value into JSON-escaped UTF-8 bytes".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                range(jse, 195, 298, RL::Strong),
                range(jse, 301, 329, RL::Related),
            ],
        },
        // G2Q13: fast int to output buffer
        RelevanceJudgment {
            id: "G2Q13".into(),
            query_text: "write an integer value directly into a byte output buffer".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                range(no, 139, 195, RL::Strong),
                range(no, 73, 137, RL::Related),
            ],
        },
        // G2Q14: number parsing with fallback
        RelevanceJudgment {
            id: "G2Q14".into(),
            query_text: "parse a lenient integer string value with a default fallback".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                range(ni, 245, 282, RL::Strong),
                range(ni, 284, 327, RL::Related),
            ],
        },
        // G2Q15: symbol canonicalization for char input
        RelevanceJudgment {
            id: "G2Q15".into(),
            query_text: "canonicalize a property name from a character buffer".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                range(ctn, 468, 478, RL::Strong),
                range(bqc, 574, 604, RL::Related),
            ],
        },
        // G2Q16: symbol table child table lifecycle
        RelevanceJudgment {
            id: "G2Q16".into(),
            query_text: "create a child symbol table for a new parser and merge it back".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                range(bqc, 357, 372, RL::Strong),
                range(bqc, 403, 431, RL::Related),
            ],
        },
        // G2Q17: adding names to the symbol table
        RelevanceJudgment {
            id: "G2Q17".into(),
            query_text: "intern a new property name into the quad-based canonicalizer".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                range(bqc, 878, 896, RL::Strong),
                range(bqc, 948, 986, RL::Related),
            ],
        },
        // G2Q18: read constraints configuration
        RelevanceJudgment {
            id: "G2Q18".into(),
            query_text: "configure the maximum nesting depth allowed when reading".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                range(src_, 142, 160, RL::Strong),
                range(src_, 115, 120, RL::Related),
            ],
        },
        // G2Q19: pretty printer object indentation
        RelevanceJudgment {
            id: "G2Q19".into(),
            query_text: "print a JSON object with indentation and separators".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                range(dpp, 259, 273, RL::Strong),
                range(dpp, 199, 223, RL::Related),
            ],
        },
        // G2Q20: root value checks in ParserMinimalBase
        RelevanceJudgment {
            id: "G2Q20".into(),
            query_text: "read the property name token of a JSON object during parsing".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                range(pmb, 471, 474, RL::Strong),
                range(pmb, 476, 479, RL::Related),
            ],
        },
        // ==================== FZ: Fuzzy Queries (15 total) ====================
        // FZ-G1Q1: JsonParser::nextToken
        fuzzy_strong(
            "FZ-G1Q1-naming_affix",
            "poll_token method in JsonParser",
            "naming_affix",
            p,
            433,
            433,
        ),
        // FZ-G1Q2: JsonGenerator::writeString
        fuzzy_strong(
            "FZ-G1Q2-synonym",
            "emitText method in JsonGenerator",
            "synonym",
            g,
            705,
            705,
        ),
        // FZ-G1Q3: JsonPointer::compile
        fuzzy_strong(
            "FZ-G1Q3-paraphrase",
            "parse a pointer path expression into a matcher",
            "paraphrase",
            ptr,
            211,
            227,
        ),
        // FZ-G1Q4: ReaderBasedJsonParser::getString
        fuzzy_strong(
            "FZ-G1Q4-abbrev_expand",
            "get_str method on reader based parser",
            "abbrev_expand",
            rf,
            302,
            312,
        ),
        // FZ-G1Q5: WriterBasedJsonGenerator::writeName
        fuzzy_strong(
            "FZ-G1Q5-naming_case",
            "writeName() in WriterBasedJsonGenerator",
            "naming_case",
            wg,
            148,
            159,
        ),
        // FZ-G1Q6: ByteQuadsCanonicalizer::findName
        fuzzy_strong(
            "FZ-G1Q6-synonym",
            "lookupName method in ByteQuadsCanonicalizer",
            "synonym",
            bqc,
            664,
            682,
        ),
        // FZ-G1Q7: TextBuffer::contentsAsString
        fuzzy_strong(
            "FZ-G1Q7-paraphrase",
            "combine buffered character segments into one text value",
            "paraphrase",
            tb,
            505,
            525,
        ),
        // FZ-G1Q8: NumberInput::parseInt(char[])
        fuzzy_strong(
            "FZ-G1Q8-naming_affix",
            "parse_int method in NumberInput",
            "naming_affix",
            ni,
            39,
            79,
        ),
        // FZ-G1Q9: JsonParserSequence::nextToken
        fuzzy_strong(
            "FZ-G1Q9-abbrev_expand",
            "next_tok method on parser sequence",
            "abbrev_expand",
            seq,
            128,
            148,
        ),
        // FZ-G1Q10: JsonToken enum start-object marker
        fuzzy_strong(
            "FZ-G1Q10-naming_case",
            "START_OBJECT() in JsonToken",
            "naming_case",
            tok,
            37,
            42,
        ),
        // Remaining fuzzy variants against additional G1 anchors
        fuzzy_strong(
            "FZ-G2Q5-synonym",
            "checkDuplicate method in DupDetector",
            "synonym",
            dup,
            81,
            96,
        ),
        fuzzy_strong(
            "FZ-G2Q9-paraphrase",
            "take a byte buffer out of the recycle pool and give it back",
            "paraphrase",
            br,
            158,
            168,
        ),
        fuzzy_strong(
            "FZ-G2Q11-naming_affix",
            "quote_str method in JsonStringEncoder",
            "naming_affix",
            jse,
            151,
            192,
        ),
        fuzzy_strong(
            "FZ-G2Q13-abbrev_expand",
            "out_int method on NumberOutput",
            "abbrev_expand",
            no,
            139,
            195,
        ),
        fuzzy_strong(
            "FZ-G2Q17-naming_case",
            "addName() in ByteQuadsCanonicalizer",
            "naming_case",
            bqc,
            878,
            896,
        ),
    ]
}
