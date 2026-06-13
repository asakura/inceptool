//! Formats the engine's [`HookOutputEvent`] into the driver's wire format
//! and writes it to stdout, applying any terminal side effects (exit codes,
//! stderr messages, special-cased plain-text responses).

use inceptool_protocol::{Driver, HookOutputEvent};

use miette::{IntoDiagnostic, Result};
use std::io::{self, Write};

/// Translates the computed [`HookOutputEvent`] into JSON and issues side effects.
#[expect(
    clippy::print_stderr,
    clippy::exit,
    reason = "terminal hook actions are contractually required to write a message to stderr \
              and exit with a specific code"
)]
pub fn handle_output<D: Driver>(output: Option<HookOutputEvent>, driver: &D) -> Result<()> {
    if let Some(output) = output {
        // `WorktreeCreate` is special-cased: when invoked as a command hook, Claude Code
        // expects the (possibly modified) worktree path on stdout as plain text rather
        // than as a JSON envelope (the JSON `hookSpecificOutput.worktreePath` form is
        // only used for the HTTP hook contract).
        if let HookOutputEvent::WorktreeCreate(o) = &output
            && let Some(path) = &o.worktree_path
        {
            io::stdout().write_all(path.as_bytes()).into_diagnostic()?;
            io::stdout().flush().into_diagnostic()?;
            return Ok(());
        }

        let (exit_code, stderr_msg) = output.exit_metadata();
        let stderr_string = stderr_msg.map(str::to_string);
        let json_resp = inceptool_protocol::to_wire(driver, "", &output).into_diagnostic()?;

        // Print the result to stdout
        io::stdout()
            .write_all(json_resp.as_bytes())
            .into_diagnostic()?;
        io::stdout().flush().into_diagnostic()?;

        // Handle terminal actions requiring system exit
        if let Some(code) = exit_code {
            if let Some(err) = stderr_string {
                eprintln!("{err}");
            }

            std::process::exit(code);
        }
    } else {
        // If no hooks modified the output, provide the default allow for Claude/Gemini
        io::stdout().write_all(b"{}").into_diagnostic()?;
        io::stdout().flush().into_diagnostic()?;
    }

    Ok(())
}
