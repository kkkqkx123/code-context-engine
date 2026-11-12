//! Fact-to-proposition rendering for the embedding path.
//!
//! Raw fact fragments state how the code works; queries ask what the code
//! is for. This module translates each control-flow and behavior fact into
//! one short sentence. The verb comes from a closed set while every
//! substantive word is copied verbatim from the fact text, so a reviewer
//! can trace each word back to the source and no new claim enters the text.
//!
//! The keyword path never sees these sentences: it keeps the raw fragments.
//! When every mapped fact fits the head budget the embedding path drops the
//! raw code tail entirely, so the two never lengthen the same denominator.
//! When facts overflow the budget the uncovered remainder stays as raw code:
//! eight sentences cannot speak for a whole mega-function, and dropping the
//! body would reintroduce the deep-token amputation the head budget removed.
//!
//! Layout follows responsibility, not history: the vocabulary owns the
//! closed word set, the text module owns string surgery, the guard module
//! owns worthiness checks as data tables, the render module owns only the
//! kind-to-sentence decision table, and the assembly owns budget and order.

pub(super) mod assembly;
mod code_text;
mod guards;
mod render;
pub(super) mod vocab;

pub(super) use assembly::render_propositions;
