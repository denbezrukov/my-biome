use super::{literal_member_name, literal_string};
use biome_js_syntax::{AnyJsExpression, JsCallExpression, JsPropertyObjectMember};
use biome_rowan::{AstNode, AstSeparatedList, TextRange};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectCommand {
    pub range: TextRange,
    pub method: String,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub cwd: Option<String>,
    pub unsupported: bool,
}

pub(super) fn collect(call: &JsCallExpression, method: &str) -> ProjectCommand {
    let mut command = ProjectCommand {
        range: call.range(),
        method: method.into(),
        command: None,
        args: Some(Vec::new()),
        cwd: None,
        unsupported: false,
    };
    let Ok(arguments) = call.arguments() else {
        command.unsupported = true;
        return command;
    };
    let mut arguments = arguments.args().into_iter();
    command.command = arguments
        .next()
        .and_then(Result::ok)
        .and_then(|a| a.as_any_js_expression().and_then(literal_string));
    if let Some(argument) = arguments.next().and_then(Result::ok) {
        if let Some(AnyJsExpression::JsArrayExpression(array)) = argument.as_any_js_expression() {
            command.args = array
                .elements()
                .iter()
                .map(|element| {
                    let element = element.ok()?;
                    AnyJsExpression::cast(element.into_syntax())
                        .as_ref()
                        .and_then(literal_string)
                })
                .collect();
            if let Some(options) = arguments
                .next()
                .and_then(Result::ok)
                .and_then(|a| a.as_any_js_expression().cloned())
            {
                read_options(&mut command, &options);
            }
        } else if let Some(options) = argument.as_any_js_expression() {
            read_options(&mut command, options);
        }
    }
    command
}
fn read_options(command: &mut ProjectCommand, expression: &AnyJsExpression) {
    let Some(object) = expression.as_js_object_expression() else {
        if !matches!(
            expression,
            AnyJsExpression::JsArrowFunctionExpression(_)
                | AnyJsExpression::JsFunctionExpression(_)
        ) {
            command.unsupported = true;
        }
        return;
    };
    for member in object.members() {
        let Ok(member) = member else {
            command.unsupported = true;
            continue;
        };
        let Some(property) = JsPropertyObjectMember::cast(member.into_syntax()) else {
            command.unsupported = true;
            continue;
        };
        let Some(name) = property
            .name()
            .ok()
            .and_then(|name| literal_member_name(&name))
        else {
            command.unsupported = true;
            continue;
        };
        let Ok(value) = property.value() else {
            command.unsupported = true;
            continue;
        };
        match name.as_str() {
            "cwd" => {
                command.cwd = literal_string(&value);
                command.unsupported |= command.cwd.is_none() || command.method == "worker";
            }
            "shell" => {
                command.unsupported |= value.syntax().text_trimmed() != "false";
            }
            "env" | "eval" | "execArgv" => command.unsupported = true,
            _ => {}
        }
    }
}

pub(super) fn collect_worker(new: &biome_js_syntax::JsNewExpression) -> ProjectCommand {
    let mut command = ProjectCommand {
        range: new.range(),
        method: "worker".into(),
        command: None,
        args: Some(Vec::new()),
        cwd: None,
        unsupported: false,
    };
    if let Some(arguments) = new.arguments() {
        let [first, second] = arguments.get_arguments_by_index([0, 1]);
        command.command = first
            .as_ref()
            .and_then(|a| a.as_any_js_expression())
            .and_then(literal_string);
        if let Some(options) = second.as_ref().and_then(|a| a.as_any_js_expression()) {
            read_options(&mut command, options);
        }
    }
    command
}
