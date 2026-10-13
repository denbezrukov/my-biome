use crate::prelude::*;
use crate::verbatim::{CssVerbatimTokenFormat, format_css_verbatim_range};
use biome_css_syntax::CssSupportsAtRuleFallback;
#[derive(Debug, Clone, Default)]
pub(crate) struct FormatCssSupportsAtRuleFallback;
impl FormatNodeRule<CssSupportsAtRuleFallback> for FormatCssSupportsAtRuleFallback {
    fn fmt_fields(
        &self,
        node: &CssSupportsAtRuleFallback,
        f: &mut CssFormatter,
    ) -> FormatResult<()> {
        format_css_verbatim_range(
            node.syntax(),
            node.syntax().text_trimmed_range(),
            |_, _, _| Ok(CssVerbatimTokenFormat::Source),
        )
        .fmt(f)
    }
}
