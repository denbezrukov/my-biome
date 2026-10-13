use crate::prelude::*;
use crate::utils::custom_property::CustomPropertyContainer;
use crate::verbatim::{CssVerbatimTokenFormat, format_css_verbatim_range};
use biome_css_syntax::{
    AnyCssCustomPropertyComponent, CssCustomPropertyFunction, CssCustomPropertyFunctionFields,
    decode_css_identifier,
};
use biome_formatter::write;
#[derive(Debug, Clone, Default)]
pub(crate) struct FormatCssCustomPropertyFunction;
impl FormatNodeRule<CssCustomPropertyFunction> for FormatCssCustomPropertyFunction {
    fn fmt_fields(
        &self,
        node: &CssCustomPropertyFunction,
        f: &mut CssFormatter,
    ) -> FormatResult<()> {
        let CssCustomPropertyFunctionFields {
            name,
            l_paren_token: _,
            components,
            r_paren_token: _,
        } = node.as_fields();
        let name = name?;

        // Unquoted URL commas and comment-shaped text are payload, including
        // trivia that ordinary component formatting would separate with spaces.
        if decode_css_identifier(name.value_token()?.text_trimmed()).eq_ignore_ascii_case("url")
            && !matches!(
                components.first(),
                Some(
                    AnyCssCustomPropertyComponent::CssString(_)
                        | AnyCssCustomPropertyComponent::ScssInterpolatedString(_)
                )
            )
        {
            return format_css_verbatim_range(
                node.syntax(),
                node.syntax().text_trimmed_range(),
                |_, _, _| Ok(CssVerbatimTokenFormat::Source),
            )
            .fmt(f);
        }

        write!(
            f,
            [name.format(), CustomPropertyContainer::from(node.clone())]
        )
    }
}
