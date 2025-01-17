use crate::database::SafeConnection;
use crate::database::{KVStore, Matter, NotificationRecord, RepeatTask, Tag, Todo};
use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use chrono::{DateTime, Utc};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{oneshot, Mutex};

#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    code: i32,
    msg: String,
    data: Option<T>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            code: 200,
            msg: "success".to_string(),
            data: Some(data),
        }
    }

    pub fn error(code: i32, msg: &str) -> Self {
        Self {
            code,
            msg: msg.to_string(),
            data: None,
        }
    }
}

#[derive(Error, Debug)]
pub enum ServerError {
    #[error("服务器启动失败：{0}")]
    StartupError(String),
    #[error("数据库错误：{0}")]
    DatabaseError(String),
    #[error("效请求：{0}")]
    BadRequest(String),
    #[error("未找到资源：{0}")]
    NotFound(String),
}

impl IntoResponse for ServerError {
    fn into_response(self) -> axum::response::Response {
        let (code, message) = match self {
            ServerError::NotFound(msg) => (404, msg),
            ServerError::DatabaseError(msg) => (500, msg),
            ServerError::StartupError(msg) => (500, msg),
            ServerError::BadRequest(msg) => (400, msg),
        };

        Json(ApiResponse::<()>::error(code, &message)).into_response()
    }
}

pub struct AppState {
    shutdown_tx: Option<oneshot::Sender<()>>,
    db: Arc<SafeConnection>,
}

#[derive(Debug, Deserialize)]
pub struct TimeRangeQuery {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
}

trait RouteConfig {
    fn configure(self, state: Arc<Mutex<AppState>>) -> Router;
}

struct ApiRoutes;

impl RouteConfig for ApiRoutes {
    fn configure(self, state: Arc<Mutex<AppState>>) -> Router {
        Router::new()
            .route("/matter", post(create_matter))
            .route("/matter/:id", get(get_matter))
            .route("/matter/:id", put(update_matter))
            .route("/matter/:id", delete(delete_matter))
            .route("/matter/range", get(get_matters_by_range))
            .route("/matter", get(get_all_matters))
            .route("/matter/query", get(query_matter_by_field))
            .route("/kv/:key", get(get_kv))
            .route("/kv/:key", put(set_kv))
            .route("/kv/:key", delete(delete_kv))
            .route("/tags", post(create_tag))
            .route("/tags", get(get_all_tags))
            .route("/tags/:name", delete(delete_tag))
            .route("/tags/update/:name", put(update_tag_last_used_at))
            .route("/repeat-task", post(create_repeat_task))
            .route("/repeat-task/:id", get(get_repeat_task))
            .route("/repeat-task/:id", put(update_repeat_task))
            .route("/repeat-task/:id", delete(delete_repeat_task))
            .route("/repeat-task", get(get_all_repeat_tasks))
            .route("/repeat-task/active", get(get_active_repeat_tasks))
            .route(
                "/repeat-task/:id/status/:status",
                put(update_repeat_task_status),
            )
            .route("/todo", post(create_todo))
            .route("/todo/:id", get(get_todo))
            .route("/todo/:id", put(update_todo))
            .route("/todo/:id", delete(delete_todo))
            .route("/todo", get(get_all_todos))
            .route("/notification", post(create_notification))
            .route("/notification/:id", get(get_notification))
            .route("/notification/:id", put(update_notification))
            .route("/notification/:id", delete(delete_notification))
            // .route("/notification", get(get_all_notifications))
            .route("/notification/unread", get(get_unread_notifications))
            .route("/notification/:id/read", put(mark_notification_as_read))
            // make special type notification as read
            .route(
                "/notification/read/:type",
                put(mark_notification_as_read_by_type),
            )
            .route(
                "/notification/read-all",
                put(mark_all_notifications_as_read),
            )
            .with_state(state)
    }
}

#[derive(Clone)]
pub struct HttpServer {
    state: Arc<Mutex<AppState>>,
}

impl HttpServer {
    pub fn new(db: Arc<SafeConnection>) -> Self {
        let state = Arc::new(Mutex::new(AppState {
            shutdown_tx: None,
            db,
        }));
        Self { state }
    }

    pub async fn start(&self, port: u16) -> Result<(), ServerError> {
        let app = ApiRoutes.configure(self.state.clone());
        log::info!("HTTP server starting on port {}", port);
        // localhost
        let addr = format!("localhost:{}", port);
        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .map_err(|e| ServerError::StartupError(e.to_string()))?;

        log::info!("HTTP server listening on {}", addr);

        axum::serve(listener, app)
            .await
            .map_err(|e| ServerError::StartupError(e.to_string()))?;

        Ok(())
    }

    pub async fn stop(&self) {
        if let Ok(mut state) = self.state.try_lock() {
            if let Some(tx) = state.shutdown_tx.take() {
                let _ = tx.send(());
            }
        }
    }
}

async fn root() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

#[derive(Deserialize)]
struct CreateData {
    name: String,
    value: String,
}

async fn create_data(
    State(_state): State<Arc<Mutex<AppState>>>,
    Json(payload): Json<CreateData>,
) -> Result<impl IntoResponse, ServerError> {
    // 这里可以访问用状态进行数据处理
    Ok(Json(json!({
        "name": payload.name,
        "value": payload.value,
        "status": "created"
    })))
}

// Matter 相关处理函数
async fn create_matter(
    State(state): State<Arc<Mutex<AppState>>>,
    Json(mut matter): Json<Matter>,
) -> Result<impl IntoResponse, ServerError> {
    matter.created_at = Utc::now();
    matter.updated_at = Utc::now();

    let state = state.lock().await;
    Matter::create(&state.db, &matter).map_err(|e| ServerError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(matter)))
}

async fn get_matter(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ServerError> {
    let state = state.lock().await;
    let matter = Matter::get_by_id(&state.db, &id)
        .map_err(|e| ServerError::DatabaseError(e.to_string()))?
        .ok_or_else(|| ServerError::NotFound("Matter not found".into()))?;

    Ok(Json(ApiResponse::success(matter)))
}

// get all matters
async fn get_all_matters(
    State(state): State<Arc<Mutex<AppState>>>,
) -> Result<impl IntoResponse, ServerError> {
    let state = state.lock().await;
    let matters =
        Matter::get_all(&state.db).map_err(|e| ServerError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(matters)))
}

async fn update_matter(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(id): Path<String>,
    Json(mut matter): Json<Matter>,
) -> Result<impl IntoResponse, ServerError> {
    matter.id = id;
    matter.updated_at = Utc::now();

    let state = state.lock().await;
    matter
        .update(&state.db)
        .map_err(|e| ServerError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(matter)))
}

async fn delete_matter(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ServerError> {
    let state = state.lock().await;
    Matter::delete(&state.db, &id).map_err(|e| ServerError::DatabaseError(e.to_string()))?;
    Ok(Json(ApiResponse::<()>::success(())))
}

async fn get_matters_by_range(
    State(state): State<Arc<Mutex<AppState>>>,
    Query(range): Query<TimeRangeQuery>,
) -> Result<impl IntoResponse, ServerError> {
    let state = state.lock().await;
    let matters = Matter::get_by_time_range(&state.db, range.start, range.end)
        .map_err(|e| ServerError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(matters)))
}

// KVStore 相关处理函数
async fn set_kv(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(key): Path<String>,
    value: String,
) -> Result<impl IntoResponse, ServerError> {
    let state = state.lock().await;
    KVStore::set(&state.db, &key, &value).map_err(|e| ServerError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::<()>::success(())))
}

async fn get_kv(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(key): Path<String>,
) -> Result<impl IntoResponse, ServerError> {
    let state = state.lock().await;
    let value =
        KVStore::get(&state.db, &key, "").map_err(|e| ServerError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(value)))
}

async fn delete_kv(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(key): Path<String>,
) -> Result<impl IntoResponse, ServerError> {
    let state = state.lock().await;
    KVStore::delete(&state.db, &key).map_err(|e| ServerError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::<()>::success(())))
}

// Tag 相关处理函数
#[derive(Deserialize)]
struct TagsRequest {
    names: String,
}

async fn create_tag(
    State(state): State<Arc<Mutex<AppState>>>,
    Json(payload): Json<TagsRequest>,
) -> Result<impl IntoResponse, ServerError> {
    let state = state.lock().await;

    // 分割字符串并去重
    let names: Vec<String> = payload
        .names
        .split(',')
        .map(|s| s.trim().to_string())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    if names.is_empty() {
        return Err(ServerError::BadRequest(
            "No valid tag names provided".into(),
        ));
    }

    // 批量创建标签
    for name in names {
        Tag::create(&state.db, &name).map_err(|e| ServerError::DatabaseError(e.to_string()))?;
    }

    Ok(Json(ApiResponse::<()>::success(())))
}

async fn get_all_tags(
    State(state): State<Arc<Mutex<AppState>>>,
) -> Result<impl IntoResponse, ServerError> {
    let state = state.lock().await;
    let tags = Tag::get_all(&state.db).map_err(|e| ServerError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(tags)))
}

async fn delete_tag(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(names): Path<String>,
) -> Result<impl IntoResponse, ServerError> {
    let state = state.lock().await;

    // 分割字符串并去重
    let names: Vec<String> = names
        .split(',')
        .map(|s| s.trim().to_string())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    if names.is_empty() {
        return Err(ServerError::BadRequest(
            "No valid tag names provided".into(),
        ));
    }

    // 批量删除标签
    for name in names {
        Tag::delete(&state.db, &name).map_err(|e| ServerError::DatabaseError(e.to_string()))?;
    }

    Ok(Json(ApiResponse::<()>::success(())))
}

async fn update_tag_last_used_at(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(names): Path<String>,
) -> Result<impl IntoResponse, ServerError> {
    let state = state.lock().await;

    // 分割字符串并去重
    let names: Vec<String> = names
        .split(',')
        .map(|s| s.trim().to_string())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    if names.is_empty() {
        return Err(ServerError::BadRequest(
            "No valid tag names provided".into(),
        ));
    }

    // 批量更新标签的最后使用时间
    for name in names {
        Tag::update_last_used_at(&state.db, &name)
            .map_err(|e| ServerError::DatabaseError(e.to_string()))?;
    }

    Ok(Json(ApiResponse::<()>::success(())))
}

// RepeatTask 相关处理函数
async fn create_repeat_task(
    State(state): State<Arc<Mutex<AppState>>>,
    Json(mut task): Json<RepeatTask>,
) -> Result<impl IntoResponse, ServerError> {
    task.created_at = Utc::now();
    task.updated_at = Utc::now();

    let state = state.lock().await;
    RepeatTask::create(&state.db, &task).map_err(|e| ServerError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(task)))
}

async fn get_repeat_task(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ServerError> {
    let state = state.lock().await;
    let task = RepeatTask::get_by_id(&state.db, &id)
        .map_err(|e| ServerError::DatabaseError(e.to_string()))?
        .ok_or_else(|| ServerError::NotFound("RepeatTask not found".into()))?;

    Ok(Json(ApiResponse::success(task)))
}

async fn get_all_repeat_tasks(
    State(state): State<Arc<Mutex<AppState>>>,
) -> Result<impl IntoResponse, ServerError> {
    let state = state.lock().await;
    let tasks =
        RepeatTask::get_all(&state.db).map_err(|e| ServerError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(tasks)))
}

async fn get_active_repeat_tasks(
    State(state): State<Arc<Mutex<AppState>>>,
) -> Result<impl IntoResponse, ServerError> {
    let state = state.lock().await;
    let tasks = RepeatTask::get_active_tasks(&state.db)
        .map_err(|e| ServerError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(tasks)))
}

async fn update_repeat_task(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(id): Path<String>,
    Json(mut task): Json<RepeatTask>,
) -> Result<impl IntoResponse, ServerError> {
    task.id = id;
    task.updated_at = Utc::now();

    let state = state.lock().await;
    task.update(&state.db)
        .map_err(|e| ServerError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(task)))
}

async fn delete_repeat_task(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ServerError> {
    let state = state.lock().await;
    RepeatTask::delete(&state.db, &id).map_err(|e| ServerError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::<()>::success(())))
}

async fn update_repeat_task_status(
    State(state): State<Arc<Mutex<AppState>>>,
    Path((id, status)): Path<(String, i32)>,
) -> Result<impl IntoResponse, ServerError> {
    let state = state.lock().await;
    RepeatTask::update_status(&state.db, &id, status)
        .map_err(|e| ServerError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::<()>::success(())))
}

// Todo 相关处理函数
async fn create_todo(
    State(state): State<Arc<Mutex<AppState>>>,
    Json(mut todo): Json<Todo>,
) -> Result<impl IntoResponse, ServerError> {
    todo.created_at = Utc::now();
    todo.updated_at = Utc::now();

    let state = state.lock().await;
    Todo::create(&state.db, &todo).map_err(|e| ServerError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(todo)))
}

async fn get_todo(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ServerError> {
    let state = state.lock().await;
    let todo = Todo::get_by_id(&state.db, &id)
        .map_err(|e| ServerError::DatabaseError(e.to_string()))?
        .ok_or_else(|| ServerError::NotFound("Todo not found".into()))?;

    Ok(Json(ApiResponse::success(todo)))
}

async fn get_all_todos(
    State(state): State<Arc<Mutex<AppState>>>,
) -> Result<impl IntoResponse, ServerError> {
    let state = state.lock().await;
    let todos = Todo::get_all(&state.db).map_err(|e| ServerError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(todos)))
}

async fn update_todo(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(id): Path<String>,
    Json(mut todo): Json<Todo>,
) -> Result<impl IntoResponse, ServerError> {
    todo.id = id;
    todo.updated_at = Utc::now();

    let state = state.lock().await;
    todo.update(&state.db)
        .map_err(|e| ServerError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(todo)))
}

async fn delete_todo(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ServerError> {
    let state = state.lock().await;
    Todo::delete(&state.db, &id).map_err(|e| ServerError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::<()>::success(())))
}


static HTTP_SERVER: OnceCell<HttpServer> = OnceCell::new();
static SERVER_PORT: AtomicU16 = AtomicU16::new(0);

pub fn start_http_server(port: u16, db: Arc<SafeConnection>) -> Result<(), String> {

    if let Some(server) = HTTP_SERVER.get() {
        let current_port = SERVER_PORT.load(Ordering::Relaxed);
        if current_port == port {
            return Ok(());
        } else {
            return Err(format!(
                "HTTP server already running on port {}",
                current_port
            ));
        }
    }

    let server = HttpServer::new(db);
    let server_clone = server.clone();

    SERVER_PORT.store(port, Ordering::Relaxed);

    tauri::async_runtime::spawn(async move {
        if let Err(e) = server_clone.start(port).await {
            log::error!("HTTP server failed to start: {}", e);
        }
    });

    match HTTP_SERVER.set(server) {
        Ok(_) => Ok(()),
        Err(_) => Err("Failed to store HTTP server instance".into()),
    }
}

pub fn stop_http_server() -> Result<(), ServerError> {
    if let Some(server) = HTTP_SERVER.get() {
        tauri::async_runtime::block_on(async {
            server.stop().await;
        });
        SERVER_PORT.store(0, Ordering::Relaxed);
        Ok(())
    } else {
        Err(ServerError::StartupError("HTTP server not running".into()))
    }
}

async fn create_notification(
    State(state): State<Arc<Mutex<AppState>>>,
    Json(mut notification): Json<NotificationRecord>,
) -> Result<impl IntoResponse, ServerError> {
    notification.created_at = Utc::now();

    let state = state.lock().await;
    NotificationRecord::create(&state.db, &notification)
        .map_err(|e| ServerError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(notification)))
}

async fn get_notification(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ServerError> {
    let state = state.lock().await;
    let notification = NotificationRecord::get_by_id(&state.db, &id)
        .map_err(|e| ServerError::DatabaseError(e.to_string()))?
        .ok_or_else(|| ServerError::NotFound("Notification not found".into()))?;

    Ok(Json(ApiResponse::success(notification)))
}

// async fn get_all_notifications(State(state): State<Arc<Mutex<AppState>>>) -> Result<
//     impl IntoResponse,
//     ServerError
// > {
//     let state = state.lock().await;
//     let notifications = NotificationRecord::get_all(&state.db).map_err(|e|
//         ServerError::DatabaseError(e.to_string())
//     )?;

//     Ok(Json(ApiResponse::success(notifications)))
// }

async fn get_unread_notifications(
    State(state): State<Arc<Mutex<AppState>>>,
) -> Result<impl IntoResponse, ServerError> {
    let state = state.lock().await;
    let notifications = NotificationRecord::get_unread(&state.db)
        .map_err(|e| ServerError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(notifications)))
}

async fn update_notification(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(id): Path<String>,
    Json(mut notification): Json<NotificationRecord>,
) -> Result<impl IntoResponse, ServerError> {
    notification.id = id;

    let state = state.lock().await;
    notification
        .update(&state.db)
        .map_err(|e| ServerError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(notification)))
}

async fn delete_notification(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ServerError> {
    let state = state.lock().await;
    NotificationRecord::delete(&state.db, &id)
        .map_err(|e| ServerError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::<()>::success(())))
}

async fn mark_notification_as_read(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ServerError> {
    let state = state.lock().await;
    NotificationRecord::mark_as_read(&state.db, &id)
        .map_err(|e| ServerError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::<()>::success(())))
}

async fn mark_notification_as_read_by_type(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(type_): Path<i32>,
) -> Result<impl IntoResponse, ServerError> {
    let state = state.lock().await;
    NotificationRecord::mark_as_read_by_type(&state.db, type_)
        .map_err(|e| ServerError::DatabaseError(e.to_string()))?;
    Ok(Json(ApiResponse::<()>::success(())))
}

// 将所有通知标记为已读
async fn mark_all_notifications_as_read(
    State(state): State<Arc<Mutex<AppState>>>,
) -> Result<impl IntoResponse, ServerError> {
    let state = state.lock().await;
    NotificationRecord::mark_all_as_read(&state.db)
        .map_err(|e| ServerError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::<()>::success(())))
}

#[derive(Debug, Deserialize)]
pub struct QueryFieldParams {
    field: String,
    value: String,
    #[serde(default = "default_exact_match")]
    exact_match: bool,
}

fn default_exact_match() -> bool {
    false
}

async fn query_matter_by_field(
    State(state): State<Arc<Mutex<AppState>>>,
    Query(params): Query<QueryFieldParams>,
) -> Result<impl IntoResponse, ServerError> {
    // 验证字段名是否合法
    let valid_fields = vec![
        "id",
        "title",
        "description",
        "tags",
        "priority",
        "type",
        "reserved_1",
        "reserved_2",
        "reserved_3",
        "reserved_4",
        "reserved_5",
    ];

    if !valid_fields.contains(&params.field.as_str()) {
        return Err(ServerError::BadRequest(format!(
            "Invalid field name: {}. Valid fields are: {}",
            params.field,
            valid_fields.join(", ")
        )));
    }

    let state = state.lock().await;
    let matters =
        Matter::query_by_field(&state.db, &params.field, &params.value, params.exact_match)
            .map_err(|e| ServerError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(matters)))
}
