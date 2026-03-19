use crate::cli::{AuthAction, AuthArgs};
use crate::error::CliResult;
use crate::output::{OutputMode, json as json_out, text};
use crate::transport::{GroveTransport, Transport};

/// Arguments for `auth set`.
pub struct AuthSetArgs {
    pub provider: String,
    pub key: String,
}

/// Arguments for `auth remove`.
pub struct AuthRemoveArgs {
    pub provider: String,
}

// ── dispatch ──────────────────────────────────────────────────────────────────

pub fn dispatch(a: AuthArgs, t: GroveTransport, m: OutputMode) -> CliResult<()> {
    match a.action {
        AuthAction::List => list_cmd(t, m),
        AuthAction::Set { provider, api_key } => set_cmd(
            AuthSetArgs {
                provider,
                key: api_key,
            },
            t,
            m,
        ),
        AuthAction::Remove { provider } => remove_cmd(AuthRemoveArgs { provider }, t, m),
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
                println!("{}", text::dim("no providers configured"));
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
                        v.get("key_hint")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string(),
                    ]
                })
                .collect();
            println!(
                "{}",
                text::render_table(&["PROVIDER", "AUTHENTICATED", "KEY_HINT"], &rows)
            );
        }
    }
    Ok(())
}

// ── set ───────────────────────────────────────────────────────────────────────

pub fn set_cmd(args: AuthSetArgs, transport: GroveTransport, mode: OutputMode) -> CliResult<()> {
    transport.set_api_key(&args.provider, &args.key)?;

    match mode {
        OutputMode::Json => {
            println!(
                "{}",
                json_out::emit_json(&serde_json::json!({ "ok": true, "provider": args.provider }))
            );
        }
        OutputMode::Text { .. } => {
            println!("API key set for {}", args.provider);
        }
    }
    Ok(())
}

// ── remove ────────────────────────────────────────────────────────────────────

pub fn remove_cmd(
    args: AuthRemoveArgs,
    transport: GroveTransport,
    mode: OutputMode,
) -> CliResult<()> {
    transport.remove_api_key(&args.provider)?;

    match mode {
        OutputMode::Json => {
            println!(
                "{}",
                json_out::emit_json(&serde_json::json!({ "ok": true, "provider": args.provider }))
            );
        }
        OutputMode::Text { .. } => {
            println!("API key removed for {}", args.provider);
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
    fn auth_list_with_test_transport_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = list_cmd(t, crate::output::OutputMode::Text { no_color: true });
        assert!(result.is_ok());
    }

    #[test]
    fn auth_list_json_mode_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = list_cmd(t, crate::output::OutputMode::Json);
        assert!(result.is_ok());
    }

    #[test]
    fn auth_set_returns_err_on_test_transport() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = set_cmd(
            AuthSetArgs {
                provider: "anthropic".into(),
                key: "sk-test".into(),
            },
            t,
            crate::output::OutputMode::Text { no_color: true },
        );
        assert!(result.is_err());
    }

    #[test]
    fn auth_remove_returns_err_on_test_transport() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = remove_cmd(
            AuthRemoveArgs {
                provider: "anthropic".into(),
            },
            t,
            crate::output::OutputMode::Text { no_color: true },
        );
        assert!(result.is_err());
    }
}
