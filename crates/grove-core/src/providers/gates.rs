use std::io::{self, BufRead, Write};
use std::process::Command;

use serde::Deserialize;

use crate::errors::{GroveError, GroveResult};

/// A detected permission denial from Claude's output.
pub struct PermissionRequest {
    pub tool: String,
    pub reason: String,
}

/// The outcome of a permission gate decision.
pub enum GateDecision {
    /// Allow this specific tool use and retry.
    AllowOnce,
    /// Allow this tool for the remainder of the run and retry.
    AllowAlways,
    /// Deny; the agent will receive an error.
    Deny,
    /// Abort the entire run.
    Abort,
}

/// Prompt the human operator via TTY for a permission decision.
///
/// Returns `GateDecision::Deny` with a clear error message when stdin is not
/// a TTY (i.e. on EOF — non-interactive / CI environment).
pub fn human_gate_prompt(req: &PermissionRequest) -> GateDecision {
    eprintln!();
    eprintln!("[PERMISSION] Agent requests tool: {}", req.tool);
    eprintln!("  Reason: {}", req.reason);
    eprintln!("  [y] Allow once  [a] Allow for rest of run  [n] Deny  [q] Abort");
    eprint!("  > ");
    io::stderr().flush().ok();

    let stdin = io::stdin();
    let mut line = String::new();
    match stdin.lock().read_line(&mut line) {
        Ok(0) => {
            // EOF — non-TTY (CI or piped stdin). Fail safe.
            eprintln!(
                "[PERMISSION] stdin EOF — HumanGate requires a TTY; \
                 re-run interactively or use --permission-mode autonomous_gate"
            );
            GateDecision::Deny
        }
        _ => match line.trim().to_lowercase().as_str() {
            "y" | "yes" => GateDecision::AllowOnce,
            "a" | "allow" => GateDecision::AllowAlways,
            "q" | "quit" | "abort" => GateDecision::Abort,
            _ => GateDecision::Deny,
        },
    }
}

#[derive(Debug, Deserialize)]
struct GatekeeperResponse {
    allow: bool,
    reason: String,
}

/// Spawn a lightweight gatekeeper Claude instance to decide on a permission
/// request autonomously.
///
/// The gatekeeper receives the objective, role, tool name, Claude's stated
/// reason, and the last ~200 chars of output as context. It replies with
/// `{"allow": true/false, "reason": "..."}`.
///
/// Falls back to `GateDecision::Deny` if the response cannot be parsed.
pub fn gatekeeper_agent(
    req: &PermissionRequest,
    objective: &str,
    role: &str,
    output_context: &str,
    gatekeeper_model: Option<&str>,
    claude_command: &str,
) -> GroveResult<GateDecision> {
    let context_snippet: String = {
        let chars: Vec<char> = output_context.chars().collect();
        let start = chars.len().saturating_sub(200);
        chars[start..].iter().collect()
    };
    let model = gatekeeper_model.unwrap_or("claude-haiku-4-5-20251001");

    let prompt = format!(
        "You are a security gatekeeper for an AI orchestration system.\n\
         An agent working on objective: \"{objective}\"\n\
         Role: {role}\n\
         Wants to use tool: {tool}\n\
         Reason given: {reason}\n\
         Recent output context: {context_snippet}\n\n\
         Evaluate whether this tool use is appropriate for the stated objective.\n\
         Reply with JSON only: {{\"allow\": true/false, \"reason\": \"brief explanation\"}}",
        objective = objective,
        role = role,
        tool = req.tool,
        reason = req.reason,
        context_snippet = context_snippet,
    );

    let output = Command::new(claude_command)
        .args([
            "--print",
            "--output-format",
            "text",
            "--dangerously-skip-permissions",
            "--model",
            model,
            &prompt,
        ])
        .env("PATH", crate::capability::shell_path())
        .output()
        .map_err(|e| GroveError::Runtime(format!("gatekeeper agent failed to launch: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GroveError::Runtime(format!(
            "gatekeeper agent exited {}: {}",
            output.status,
            stderr.lines().next().unwrap_or("(no stderr)")
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if let Some(json_str) = extract_json_object(&stdout) {
        if let Ok(resp) = serde_json::from_str::<GatekeeperResponse>(json_str) {
            eprintln!(
                "[GATEKEEPER] tool={} allow={} reason={}",
                req.tool, resp.allow, resp.reason
            );
            return Ok(if resp.allow {
                GateDecision::AllowAlways
            } else {
                GateDecision::Deny
            });
        }
    }

    eprintln!("[GATEKEEPER] could not parse response — defaulting to Deny");
    Ok(GateDecision::Deny)
}

/// Extract the first `{...}` object from a string that may contain surrounding text.
fn extract_json_object(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let slice = &s[start..];
    let mut depth = 0usize;
    let mut end_offset = 0usize;
    for (i, ch) in slice.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end_offset = i + 1;
                    break;
                }
            }
            _ => {}
        }
    }
    if depth == 0 && end_offset > 0 {
        Some(&s[start..start + end_offset])
    } else {
        None
    }
}
