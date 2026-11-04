//! `waymark doctor` and `waymark validate`: one project-wide diagnostic report.
//!
//! Both commands call the same core run. `doctor` is the health check; `validate`
//! is the same review phrased as the compatibility check the specification
//! requires. The adapter owns only argument parsing, stream routing, and the
//! strict-mode exit code, which it reads from the report and therefore from the
//! registry.

use camino::Utf8Path;
use clap::Args;
use waymark_core::envelope::success_envelope;
#[cfg(feature = "web")]
use waymark_core::envelope::Finding;
use waymark_core::error::WaymarkError;
use waymark_core::ops::doctor::{self, DoctorOptions, Report};

use super::{emit, err_line, invocation_dir, EXIT_OK};

/// Arguments of `waymark doctor`.
#[derive(Debug, Args)]
pub struct DoctorArgs {
    /// Return the invalid-data exit class when error-severity findings remain.
    #[arg(long)]
    pub strict: bool,
}

/// Arguments of `waymark validate`.
#[derive(Debug, Args)]
pub struct ValidateArgs {
    /// Return the invalid-data exit class when error-severity findings remain.
    #[arg(long)]
    pub strict: bool,
}

/// Run `waymark doctor`.
pub fn run_doctor(
    args: DoctorArgs,
    project: Option<&Utf8Path>,
    json: bool,
) -> Result<u8, WaymarkError> {
    let report = diagnose(project, DoctorOptions::new().with_strict(args.strict))?;
    finish(&report, json, "doctor")
}

/// Run `waymark validate` across the whole project.
pub fn run_validate(
    args: ValidateArgs,
    project: Option<&Utf8Path>,
    json: bool,
) -> Result<u8, WaymarkError> {
    let report = diagnose(project, DoctorOptions::new().with_strict(args.strict))?;
    finish(&report, json, "validate")
}

/// Resolve the project and run the shared diagnostics.
///
/// A project whose configuration cannot be opened still deserves a diagnosis,
/// so the shared core entry point maps `CONFIG_INVALID` and
/// `UNSUPPORTED_VERSION` to a configuration-scoped report instead of a bare
/// failure with no findings.
fn diagnose(project: Option<&Utf8Path>, options: DoctorOptions) -> Result<Report, WaymarkError> {
    let start = match project {
        Some(path) => path.to_path_buf(),
        None => invocation_dir()?,
    };
    let mut report = doctor::diagnose_from(&start, options)?;
    #[cfg(feature = "web")]
    attach_asset_finding(&mut report, waymark_web::embedded_manifest_mismatch());
    Ok(report)
}

/// Attach the embedded frontend provenance finding, when this build has one.
///
/// The comparison itself lives with the manifest in `waymark-web`; the adapter
/// only turns a disagreement into the registry's finding and appends it to the
/// report it already has, so the existing envelope and exit rules apply.
#[cfg(feature = "web")]
fn attach_asset_finding(report: &mut Report, mismatch: Option<waymark_web::AssetMismatch>) {
    let Some(mismatch) = mismatch else {
        return;
    };
    report.push_finding(Finding::from_error(&WaymarkError::assets_version_mismatch(
        mismatch.kind(),
        mismatch.expected(),
        mismatch.actual(),
    )));
}

/// Emit the report and choose the process exit from the registry.
fn finish(report: &Report, json: bool, label: &str) -> Result<u8, WaymarkError> {
    if json {
        emit(&success_envelope(report.data_json(), report.meta.clone()))?;
    } else {
        render(report, label);
    }
    Ok(report
        .exit_class()
        .map(|class| u8::try_from(class.as_i32()).unwrap_or(EXIT_INTERNAL))
        .unwrap_or(EXIT_OK))
}

/// Human narrative goes to stderr, always; the machine envelope is the only
/// thing stdout ever carries.
fn render(report: &Report, label: &str) {
    if report.findings.is_empty() {
        err_line(&format!("{label}: no findings"));
    } else {
        err_line(&format!(
            "{label}: {} finding(s) — {} error, {} warning, {} info",
            report.counts.total(),
            report.counts.errors,
            report.counts.warnings,
            report.counts.info
        ));
        for finding in &report.findings {
            let location = match &finding.location {
                Some(location) => format!(" at {location}"),
                None => String::new(),
            };
            err_line(&format!(
                "  {} {}{location}: {}",
                finding.severity.as_str(),
                finding.code,
                finding.message
            ));
        }
    }
    err_line(&format!("  reviewed: {}", report.scope.join(", ")));
    for repair in &report.repairs {
        for line in &repair.changed {
            err_line(&format!("  repair: {line}"));
        }
    }
    if report.strict {
        err_line("  strict: error findings return the invalid-data exit class");
    } else {
        err_line("  not strict: findings do not change the exit class");
    }
}

/// Exit class 7, used if a report ever asks for a class the registry does not
/// expose as a u8. It cannot happen; the fallback keeps the mapping total.
const EXIT_INTERNAL: u8 = 7;

#[cfg(all(test, feature = "web"))]
mod tests {
    use super::*;
    use waymark_core::envelope::Meta;
    use waymark_core::error::{Code, ExitClass, Severity};

    fn report() -> Report {
        Report::new(
            Vec::new(),
            true,
            &[],
            Vec::new(),
            Meta::direct_file_scoped(),
        )
    }

    /// A mismatched manifest handed in directly is the test seam: neither the
    /// embedded manifest nor any on-disk asset file has to change.
    #[test]
    fn a_mismatching_manifest_becomes_one_error_finding_naming_expected_and_actual() {
        let mut report = report();
        attach_asset_finding(
            &mut report,
            Some(waymark_web::AssetMismatch::Version {
                expected: "0.2.0".to_owned(),
                actual: "0.1.0".to_owned(),
            }),
        );

        assert_eq!(report.findings.len(), 1, "one disagreement, one finding");
        let finding = &report.findings[0];
        assert_eq!(finding.code, Code::AssetsVersionMismatch);
        assert_eq!(finding.severity, Severity::Error);
        assert!(!finding.fixable, "no command repairs an embedded manifest");
        assert!(finding.location.is_none());
        assert!(
            finding.message.contains("0.2.0") && finding.message.contains("0.1.0"),
            "the message names expected and actual: {}",
            finding.message
        );
        assert_eq!(report.counts.errors, 1);
        assert_eq!(report.exit_class(), Some(ExitClass::InvalidInput));
    }

    #[test]
    fn a_matching_manifest_attaches_nothing() {
        let mut report = report();
        attach_asset_finding(&mut report, None);
        assert!(report.findings.is_empty());
        assert_eq!(report.counts.total(), 0);
        assert_eq!(report.exit_class(), None);
    }
}
