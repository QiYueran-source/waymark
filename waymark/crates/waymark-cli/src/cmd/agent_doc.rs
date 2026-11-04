//! `waymark agent-doc`: a capability reference generated from the CLI definition.
//!
//! The reference is *derived*, never written by hand: it walks the `clap` command
//! tree and prints what is there — command paths, their summaries, and their
//! arguments with their value names, defaults, and possible values. That is the
//! whole point: documentation cannot claim a capability the binary does not have,
//! and it silently picks up every command a later ticket adds.
//!
//! It adds no semantics of its own. There is no prose about behaviour, no
//! explanation of what a flag *means*, and no example that could disagree with
//! the implementation: descriptions are the `about`/`help` strings declared on the
//! command tree, and everything else is structural.
//!
//! `--json` emits the same tree as one envelope, so an agent can discover
//! capabilities without parsing Markdown.

use clap::{Arg, ArgAction, Command};
use serde_json::{json, Map, Value};
use waymark_core::envelope::{success_envelope, Meta};
use waymark_core::error::WaymarkError;

use super::emit;

/// Arguments of `waymark agent-doc`.
#[derive(Debug, clap::Args)]
pub struct AgentDocArgs {
    /// Print only this command path, for example `git history`.
    #[arg(long, value_name = "PATH")]
    pub command: Option<String>,
}

/// Run `waymark agent-doc`.
///
/// `root` is the CLI's own `clap` command tree, so the adapter is describing
/// itself rather than a copy of itself.
pub fn run(root: Command, args: &AgentDocArgs, json: bool) -> Result<(), WaymarkError> {
    let selected = match &args.command {
        None => vec![(&root, String::new())],
        Some(path) => select(&root, path),
    };

    if json {
        let value = if selected.len() == 1 {
            describe(selected[0].0, &selected[0].1)
        } else {
            json!({
                "commands": selected
                    .iter()
                    .map(|(command, prefix)| describe(command, prefix))
                    .collect::<Vec<_>>()
            })
        };
        emit(&success_envelope(value, Meta::direct_file_scoped()))?;
        return Ok(());
    }

    for (command, prefix) in selected {
        print_command(command, &prefix);
    }
    Ok(())
}

/// Every command whose path (including the root name) ends with the requested
/// path.
///
/// A path is resolved structurally against the tree, so `git history` finds one
/// node and `git` finds one node, without a second name table to keep in sync.
fn select<'a>(root: &'a Command, path: &str) -> Vec<(&'a Command, String)> {
    let wanted: Vec<&str> = path.split_whitespace().collect();
    let mut found: Vec<(&Command, String, Vec<String>)> = Vec::new();
    walk(root, "", &mut Vec::new(), &mut found);
    found
        .into_iter()
        .filter(|(_, _, chain)| {
            // A caller should not have to know the binary's own name, so the
            // path may be written with or without the leading `waymark`.
            chain.len() >= wanted.len()
                && chain[chain.len() - wanted.len()..]
                    .iter()
                    .zip(&wanted)
                    .all(|(left, right)| left == right)
        })
        .map(|(command, prefix, _)| (command, prefix))
        .collect()
}

fn walk<'a>(
    command: &'a Command,
    prefix: &str,
    chain: &mut Vec<String>,
    out: &mut Vec<(&'a Command, String, Vec<String>)>,
) {
    out.push((command, prefix.to_owned(), chain.clone()));
    let own_path = path_of(prefix, command.get_name());
    for child in command.get_subcommands() {
        chain.push(child.get_name().to_owned());
        walk(child, &own_path, chain, out);
        chain.pop();
    }
}

fn path_of(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix} {name}")
    }
}

/// One command as a machine description: name, path, summary, arguments, and
/// subcommands.
fn describe(command: &Command, prefix: &str) -> Value {
    json!({
        "name": command.get_name(),
        "path": path_of(prefix, command.get_name()),
        "summary": command.get_about().map(ToString::to_string),
        "arguments": command
            .get_arguments()
            .filter(|argument| !is_builtin(argument))
            .map(describe_argument)
            .collect::<Vec<_>>(),
        "subcommands": command
            .get_subcommands()
            .map(|child| describe(child, &path_of(prefix, command.get_name())))
            .collect::<Vec<_>>(),
    })
}

fn describe_argument(argument: &Arg) -> Value {
    let short = argument.get_short();
    let long = argument.get_long();
    let mut value = Map::new();
    value.insert("id".to_owned(), json!(argument.get_id().as_str()));
    value.insert(
        "positional".to_owned(),
        json!(short.is_none() && long.is_none()),
    );
    if let Some(short) = short {
        value.insert("short".to_owned(), json!(format!("-{short}")));
    }
    if let Some(long) = long {
        value.insert("long".to_owned(), json!(format!("--{long}")));
    }
    if takes_value(argument) {
        if let Some(name) = argument.get_value_names().and_then(|names| names.first()) {
            value.insert("value_name".to_owned(), json!(name.to_string()));
        }
    }
    value.insert("required".to_owned(), json!(argument.is_required_set()));
    if let Some(default) = argument.get_default_values().first() {
        value.insert("default".to_owned(), json!(default.to_string_lossy()));
    }
    if takes_value(argument) {
        let possible: Vec<String> = argument
            .get_possible_values()
            .into_iter()
            .map(|possible| possible.get_name().to_owned())
            .collect();
        if !possible.is_empty() {
            value.insert("possible_values".to_owned(), json!(possible));
        }
    }
    if let Some(help) = argument.get_help() {
        value.insert("help".to_owned(), json!(help.to_string()));
    }
    Value::Object(value)
}

/// Whether the argument consumes a value. A boolean switch does not, and `clap`
/// would otherwise report its synthesized `true`/`false` set as possible values.
fn takes_value(argument: &Arg) -> bool {
    !matches!(
        argument.get_action(),
        ArgAction::SetTrue | ArgAction::SetFalse | ArgAction::Count
    )
}

/// `clap`'s own generated arguments are not capabilities.
fn is_builtin(argument: &Arg) -> bool {
    matches!(argument.get_id().as_str(), "help" | "version")
}

/// Render one command and its descendants as Markdown, from the command tree
/// alone.
fn print_command(command: &Command, parent_path: &str) {
    let path = path_of(parent_path, command.get_name());

    println!("## `{path}`");
    println!();
    if let Some(about) = command.get_about() {
        println!("{}", about.to_string().trim());
        println!();
    }

    let arguments: Vec<&Arg> = command
        .get_arguments()
        .filter(|argument| !is_builtin(argument))
        .collect();
    if arguments.is_empty() {
        println!("No arguments.");
        println!();
    } else {
        for argument in arguments {
            let mut labels: Vec<String> = Vec::new();
            if let Some(short) = argument.get_short() {
                labels.push(format!("`-{short}`"));
            }
            if let Some(long) = argument.get_long() {
                labels.push(format!("`--{long}`"));
            }
            let mut line = labels.join(", ");
            if takes_value(argument) {
                if let Some(value_name) = argument.get_value_names().and_then(|names| names.first())
                {
                    line.push_str(&format!(" `<{value_name}>`"));
                }
            }
            if argument.is_required_set() {
                line.push_str(" (required)");
            }
            if let Some(default) = argument.get_default_values().first() {
                line.push_str(&format!(" (default `{}`)", default.to_string_lossy()));
            }
            println!("- {line}");
            if let Some(help) = argument.get_help() {
                println!("  {}", help.to_string().trim());
            }
            if takes_value(argument) {
                let possible: Vec<String> = argument
                    .get_possible_values()
                    .into_iter()
                    .map(|possible| possible.get_name().to_owned())
                    .collect();
                if !possible.is_empty() {
                    println!("  one of: {}", possible.join(", "));
                }
            }
        }
        println!();
    }

    for child in command.get_subcommands() {
        print_command(child, &path);
    }
}
