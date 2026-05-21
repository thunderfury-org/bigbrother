mod engine;
pub(super) mod extractors;
pub(super) mod labels;
pub(super) mod release_group;
pub(super) mod span_mask;
pub(super) mod title_resolver;
pub(super) mod tokenizer;

use super::Metadata;

pub(crate) fn parse(value: &str) -> Box<Metadata> {
    engine::parse(value)
}
