# Security Policy

Waymark reads and writes work-item documents that often arrive from a
repository you cloned, and it runs with your user's privileges. A bug in
document parsing, path handling, or the local Web view is a security bug, not
just a defect. Please report it privately.

## Supported versions

Waymark is pre-1.0. Only the most recent release receives security fixes; older
tags are not backported.

| Version | Supported |
| --- | --- |
| latest `0.1.x` release | Yes |
| older tags | No — upgrade to the latest release |

A fix ships as a new release. If you cannot upgrade, the advisory will name the
release that contains the fix so you can pin it deliberately.

## Reporting a vulnerability

**Do not open a public issue, pull request, or discussion for a suspected
vulnerability.**

Use GitHub's private vulnerability reporting on this repository:

https://github.com/QiYueran-source/waymark/security/advisories/new

Please include:

- the output of `waymark version` (and `waymark version --json` for a Web build);
- your platform and how you installed Waymark;
- the smallest reproduction you can manage — a project directory, a work-item
  document, a command, or an MCP client exchange;
- what an attacker gains, and who can trigger it;
- anything you already know about a fix or a workaround.

If private reporting is unavailable to you for any reason, open a public issue
that says only that you have a security report and asks for a private channel.
Do not include the details in that issue.

## What to expect

This is a single-maintainer project, so these are targets rather than a
contract:

- **Acknowledgement** within 7 days.
- **Triage and severity** — an assessment against CVSS, and a decision on
  whether the report is in scope.
- **Fix and advisory** — we aim to ship a fix and publish a GitHub Security
  Advisory within 90 days of the report. If that is not achievable, we will say
  so and agree a disclosure date with you rather than let the report go silent.
- **Credit** — you are credited in the advisory unless you ask otherwise.

## Coordinated disclosure

Please keep the report private until the advisory is published. If you publish
or discuss it earlier, tell us first so we can order the release and the
advisory correctly rather than discovering it alongside everyone else.

If you believe the issue is being exploited in the wild, say so in the report
and we will shorten the timeline.

## Scope

In scope, for the `waymark` CLI, `waymark-mcp`, and the `waymark serve` Web view,
built from a released archive or from a commit on `master`:

- parsing and handling of work-item documents, configuration, and any other
  file read from a project directory, including a repository you did not write;
- path traversal, symlink handling, and writes escaping the project's storage
  root or `.waymark/` directory;
- the MCP server: stdio protocol handling, tool argument validation, and
  privilege boundaries between what an agent asks for and what is performed;
- the local Web service: request handling, the served asset set, and anything
  that would let a page or a process reach data the operator did not expose;
- release integrity: checksums, archive contents, and the published build
  provenance manifest.

Out of scope:

- vulnerabilities in a dependency that a newer release already fixes — report
  those upstream, or tell us and we will bump it;
- attacks that require the user to run a modified binary, or to trust a
  configuration file they wrote themselves;
- the Web view being reachable from another machine. It binds `127.0.0.1` by
  design and has no option to bind elsewhere; exposing it requires the operator
  to put a proxy in front of it, which is outside what Waymark promises;
- self-inflicted resource exhaustion, such as pointing Waymark at a project of
  your own construction that is designed to be slow;
- missing hardening headers, or the absence of features (authentication, TLS,
  rate limiting) that the Web view does not claim to have.

## Hardening notes

- The Web view is read-only and loopback-only. Do not put it behind a public
  reverse proxy.
- `waymark-mcp` executes the operations the CLI does, with your privileges.
  Point it only at projects you would let that agent edit.
- Work-item documents are untrusted input. If you clone a repository, treat its
  `work/` tree the way you treat its build scripts.
- Verify release archives against their `.sha256` sibling before running them.
  The macOS and Windows binaries are not code-signed; a checksum match is what
  establishes that you have the published file.
