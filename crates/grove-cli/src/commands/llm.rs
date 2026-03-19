use crate::cli::{LlmAction, LlmArgs};
use crate::error::CliResult;
use crate::output::{OutputMode, json as json_out, text};
use crate::transport::{GroveTransport, Transport};

/// Arguments for `llm models`.
pub struct LlmModelsArgs {
    pub provider: String,
}

/// Arguments for `llm select`.
pub struct LlmSelectArgs {
    pub provider: String,
    pub model: Option<String>,
}

// ── dispatch ──────────────────────────────────────────────────────────────────

pub fn dispatch(a: LlmArgs, t: GroveTransport, m: OutputMode) -> CliResult<()> {
    match a.action {
        LlmAction::List => list_cmd(t, m),
        LlmAction::Models { provider } => models_cmd(LlmModelsArgs { provider }, t, m),
        LlmAction::Select {
            provider,
            model,
            own_key: _,
            workspace_credits: _,
        } => select_cmd(LlmSelectArgs { provider, model }, t, m),
    }
}

// ── list ──────────────────────────────────────────────────────────────────────

pub fn list_cmd(transport: GroveTransport, mode: OutputMode) -> CliResult<()> {
    let providers = transport.list_providers()?;

    match mode {
        OutputMode::Json => {
            let val = serde_json::Value::Array(providers);
            println!("{}", json_out::emit_json(&val));
        }
        OutputMode::Text { .. } => {
            if providers.is_empty() {
                println!("{}", text::dim("no providers"));
                return Ok(());
            }
            let rows: Vec<Vec<String>> = providers
                .iter()
                .map(|v| {
                    vec![
                        v.get("provider")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string(),
                        v.get("authenticated")
                            .and_then(|x| x.as_bool())
                            .map(|b| if b { "yes" } else { "no" })
                            .unwrap_or("no")
                            .to_string(),
                        v.get("default_model")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string(),
                    ]
                })
                .collect();
            println!(
                "{}",
                text::render_table(&["PROVIDER", "AUTH", "DEFAULT_MODEL"], &rows)
            );
        }
    }
    Ok(())
}

// ── models ────────────────────────────────────────────────────────────────────

pub fn models_cmd(
    args: LlmModelsArgs,
    transport: GroveTransport,
    mode: OutputMode,
) -> CliResult<()> {
    let models = transport.list_models(&args.provider)?;

    match mode {
        OutputMode::Json => {
            let val = serde_json::Value::Array(models);
            println!("{}", json_out::emit_json(&val));
        }
        OutputMode::Text { .. } => {
            if models.is_empty() {
                println!("{}", text::dim("no models"));
                return Ok(());
            }
            let rows: Vec<Vec<String>> = models
                .iter()
                .map(|v| {
                    vec![
                        v.get("id")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string(),
                        v.get("name")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string(),
                        v.get("context_window")
                            .and_then(|x| x.as_u64())
                            .map(|n| n.to_string())
                            .unwrap_or_default(),
                        v.get("cost_input_per_m")
                            .and_then(|x| x.as_f64())
                            .map(|f| format!("${f:.2}"))
                            .unwrap_or_default(),
                        v.get("cost_output_per_m")
                            .and_then(|x| x.as_f64())
                            .map(|f| format!("${f:.2}"))
                            .unwrap_or_default(),
                        v.get("vision")
                            .and_then(|x| x.as_bool())
                            .map(|b| if b { "yes" } else { "no" })
                            .unwrap_or("no")
                            .to_string(),
                        v.get("tools")
                            .and_then(|x| x.as_bool())
                            .map(|b| if b { "yes" } else { "no" })
                            .unwrap_or("no")
                            .to_string(),
                    ]
                })
                .collect();
            println!(
                "{}",
                text::render_table(
                    &[
                        "ID", "NAME", "CONTEXT", "INPUT/M", "OUTPUT/M", "VISION", "TOOLS"
                    ],
                    &rows
                )
            );
        }
    }
    Ok(())
}

// ── select ────────────────────────────────────────────────────────────────────

pub fn select_cmd(
    args: LlmSelectArgs,
    transport: GroveTransport,
    mode: OutputMode,
) -> CliResult<()> {
    transport.select_llm(&args.provider, args.model.as_deref())?;

    let model_display = args.model.as_deref().unwrap_or("default");
    match mode {
        OutputMode::Json => {
            println!(
                "{}",
                json_out::emit_json(&serde_json::json!({
                    "ok": true,
                    "provider": args.provider,
                    "model": model_display,
                }))
            );
        }
        OutputMode::Text { .. } => {
            println!("Selected {}/{}", args.provider, model_display);
        }
    }
    Ok(())
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{GroveTransport, TestTransport};

    #[test]
    fn llm_list_empty_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(list_cmd(t, crate::output::OutputMode::Text { no_color: true }).is_ok());
    }

    #[test]
    fn llm_list_json_mode_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(list_cmd(t, crate::output::OutputMode::Json).is_ok());
    }

    #[test]
    fn llm_models_empty_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = models_cmd(
            LlmModelsArgs {
                provider: "anthropic".into(),
            },
            t,
            crate::output::OutputMode::Text { no_color: true },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn llm_models_json_mode_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = models_cmd(
            LlmModelsArgs {
                provider: "anthropic".into(),
            },
            t,
            crate::output::OutputMode::Json,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn llm_select_returns_err_on_test_transport() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = select_cmd(
            LlmSelectArgs {
                provider: "anthropic".into(),
                model: Some("claude-sonnet-4-6".into()),
            },
            t,
            crate::output::OutputMode::Text { no_color: true },
        );
        assert!(result.is_err());
    }
}
