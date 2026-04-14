use grove_daemon::rpc::envelope::{RpcError, RpcRequest, RpcResponse};
use serde_json::json;

#[test]
fn request_parses_valid_jsonrpc_2_0() {
    let raw = r#"{"jsonrpc":"2.0","method":"grove.health","params":{},"id":1}"#;
    let req: RpcRequest = serde_json::from_str(raw).unwrap();
    assert_eq!(req.method, "grove.health");
    assert_eq!(req.id, Some(json!(1)));
}

#[test]
fn response_ok_serializes_with_result_and_id() {
    let resp = RpcResponse::ok(Some(json!(42)), json!({"status":"ok"}));
    let s = serde_json::to_string(&resp).unwrap();
    assert!(s.contains(r#""jsonrpc":"2.0""#));
    assert!(s.contains(r#""result":{"status":"ok"}"#));
    assert!(s.contains(r#""id":42"#));
}

#[test]
fn response_err_uses_standard_code() {
    let resp = RpcResponse::err(Some(json!(7)), RpcError::method_not_found("foo"));
    let s = serde_json::to_string(&resp).unwrap();
    assert!(s.contains(r#""code":-32601"#));
    assert!(s.contains(r#""id":7"#));
}

#[test]
fn parse_error_uses_minus_32700() {
    let err = RpcError::parse_error("bad json");
    assert_eq!(err.code, -32700);
}
