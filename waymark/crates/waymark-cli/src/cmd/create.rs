//! `waymark create`: create one work item.

use std::io::Read;

use clap::Args;
use waymark_core::envelope::{success_envelope, Meta};
use waymark_core::error::WaymarkError;
use waymark_core::ops::create::CreateRequest;
use waymark_core::ops::{create, OpOptions};
use waymark_core::Project;

use super::{emit, err_line, item_json};

/// Arguments of `waymark create`.
#[derive(Debug, Args)]
pub struct CreateArgs {
    /// Short single-line label.
    #[arg(long, value_name = "TEXT")]
    pub title: String,

    /// Category key the item belongs to; must already be declared.
    #[arg(long, value_name = "KEY")]
    pub category: String,

    /// Accountable identity.
    #[arg(long, value_name = "NAME")]
    pub owner: String,

    /// Markdown body, or `-` to read it from stdin.
    #[arg(long, value_name = "TEXT")]
    pub body: Option<String>,

    /// Validate and report what would be written, without writing.
    #[arg(long)]
    pub dry_run: bool,
}

/// Run `waymark create`.
pub fn run(
    project: &Project,
    args: CreateArgs,
    options: &OpOptions,
    json: bool,
) -> Result<(), WaymarkError> {
    let body = match args.body.as_deref() {
        Some("-") => {
            let mut buffer = String::new();
            std::io::stdin()
                .read_to_string(&mut buffer)
                .map_err(|source| WaymarkError::io("stdin", "read", &source))?;
            Some(buffer)
        }
        Some(text) => Some(text.to_owned()),
        None => None,
    };

    let mut request = CreateRequest::new(args.title, args.category, args.owner);
    if let Some(body) = body {
        request = request.with_body(body);
    }
    let options = if args.dry_run {
        options.clone().dry_run()
    } else {
        options.clone()
    };

    let created = create::create(project, request, &options)?;

    if json {
        emit(&success_envelope(
            serde_json::json!({
                "id": created.item.id.as_str(),
                "path": created.path.to_string(),
                "written": created.written,
                "item": item_json(&created.item, &created.path),
            }),
            Meta::direct_file_scoped(),
        ))?;
    } else if created.written {
        err_line(&format!("created {}", created.item.id));
        err_line(&format!("  {}", created.path));
    } else {
        err_line(&format!("would create {}", created.item.id));
        err_line(&format!("  {}", created.path));
    }
    Ok(())
}
