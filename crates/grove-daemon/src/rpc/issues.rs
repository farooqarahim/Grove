use super::envelope::RpcError;
use super::{internal, invalid_params, join_err, to_value, DispatchCtx};
use serde::Deserialize;
use serde_json::Value;

pub async fn list_issues(ctx: &DispatchCtx, _params: Value) -> Result<Value, RpcError> {
    let root = ctx.cfg.project_root.clone();
    let rows = tokio::task::spawn_blocking(move || grove_core::facade::list_issues(&root))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    to_value(&rows)
}

#[derive(Deserialize)]
struct IdParams {
    id: String,
}

pub async fn get_issue(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let IdParams { id } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    let issue = tokio::task::spawn_blocking(move || grove_core::facade::get_issue(&root, &id))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    Ok(issue)
}

#[derive(Deserialize)]
struct CreateParams {
    title: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default)]
    priority: Option<i64>,
}

pub async fn create_issue(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let p: CreateParams = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    let issue = tokio::task::spawn_blocking(move || {
        grove_core::facade::create_issue(&root, &p.title, p.body.as_deref(), p.labels, p.priority)
    })
    .await
    .map_err(join_err)?
    .map_err(internal)?;
    Ok(issue)
}

pub async fn close_issue(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let IdParams { id } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    tokio::task::spawn_blocking(move || grove_core::facade::close_issue(&root, &id))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
struct SearchParams {
    query: String,
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    provider: Option<String>,
}
fn default_limit() -> i64 {
    100
}

pub async fn search_issues(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let p: SearchParams = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    let rows = tokio::task::spawn_blocking(move || {
        grove_core::facade::search_issues(&root, &p.query, p.limit, p.provider.as_deref())
    })
    .await
    .map_err(join_err)?
    .map_err(internal)?;
    to_value(&rows)
}

#[derive(Deserialize)]
struct SyncParams {
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    full: bool,
}

pub async fn sync_issues(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let p: SyncParams = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    let out = tokio::task::spawn_blocking(move || {
        grove_core::facade::sync_issues(&root, &root, p.provider.as_deref(), p.full)
    })
    .await
    .map_err(join_err)?
    .map_err(internal)?;
    Ok(out)
}

#[derive(Deserialize)]
struct UpdateParams {
    id: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    assignee: Option<String>,
    #[serde(default)]
    priority: Option<String>,
}

pub async fn update_issue(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let p: UpdateParams = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    let issue = tokio::task::spawn_blocking(move || {
        grove_core::facade::update_issue(
            &root,
            &p.id,
            p.title.as_deref(),
            p.status.as_deref(),
            p.label.as_deref(),
            p.assignee.as_deref(),
            p.priority.as_deref(),
        )
    })
    .await
    .map_err(join_err)?
    .map_err(internal)?;
    Ok(issue)
}

#[derive(Deserialize)]
struct CommentParams {
    id: String,
    body: String,
}

pub async fn comment_issue(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let CommentParams { id, body } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    let out =
        tokio::task::spawn_blocking(move || grove_core::facade::comment_issue(&root, &id, &body))
            .await
            .map_err(join_err)?
            .map_err(internal)?;
    Ok(out)
}

#[derive(Deserialize)]
struct AssignParams {
    id: String,
    assignee: String,
}

pub async fn assign_issue(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let AssignParams { id, assignee } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    tokio::task::spawn_blocking(move || grove_core::facade::assign_issue(&root, &id, &assignee))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
struct MoveParams {
    id: String,
    status: String,
}

pub async fn move_issue(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let MoveParams { id, status } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    tokio::task::spawn_blocking(move || grove_core::facade::move_issue(&root, &id, &status))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    Ok(Value::Null)
}

pub async fn reopen_issue(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let IdParams { id } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    tokio::task::spawn_blocking(move || grove_core::facade::reopen_issue(&root, &id))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    Ok(Value::Null)
}

pub async fn activity_issue(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let IdParams { id } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    let rows =
        tokio::task::spawn_blocking(move || grove_core::facade::activity_issue(&root, &id))
            .await
            .map_err(join_err)?
            .map_err(internal)?;
    to_value(&rows)
}

#[derive(Deserialize)]
struct PushParams {
    id: String,
    provider: String,
}

pub async fn push_issue(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let PushParams { id, provider } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    let out =
        tokio::task::spawn_blocking(move || grove_core::facade::push_issue(&root, &id, &provider))
            .await
            .map_err(join_err)?
            .map_err(internal)?;
    Ok(out)
}

pub async fn issue_ready(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let IdParams { id } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    let out = tokio::task::spawn_blocking(move || grove_core::facade::issue_ready(&root, &id))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    Ok(out)
}
