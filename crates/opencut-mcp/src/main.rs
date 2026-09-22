use std::path::{Path, PathBuf};
use std::sync::Arc;

use opencut_core::{Clip, FfmpegError, Project, ProjectError, TimelineItem};
use rmcp::{
    ErrorData, ServerHandler, ServiceExt,
    handler::server::{
        router::tool::ToolRouter,
        wrapper::{Json, Parameters},
    },
    model::{Implementation, ServerCapabilities, ServerConfig},
    schemars, tool, tool_handler, tool_router,
    transport::{
        stdio,
        streamable_http_server::{
            StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
        },
    },
};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

fn project_error(err: ProjectError) -> ErrorData {
    ErrorData::invalid_params(err.to_string(), None)
}

fn ffmpeg_error(err: FfmpegError) -> ErrorData {
    ErrorData::internal_error(err.to_string(), None)
}

fn no_project() -> ErrorData {
    ErrorData::invalid_params(
        "no project is open in this session. Call new_project first.",
        None,
    )
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct NewProjectRequest {
    /// Name for the new project. Replaces any project already open in this session.
    name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ImportClipRequest {
    /// Absolute path to a video file on disk.
    path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct TrimClipRequest {
    /// Id of the clip to trim, from a previous import_clip or get_project call.
    clip_id: String,
    /// Seconds into the clip where playback should start.
    in_point: f64,
    /// Seconds into the clip where playback should stop.
    out_point: f64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct RemoveClipRequest {
    clip_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ReorderClipRequest {
    /// Current position of the timeline item to move, 0-based.
    from: usize,
    /// Position it should occupy after the move, 0-based.
    to: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ExportProjectRequest {
    /// Where to write the rendered file, e.g. "C:/Users/you/output.mp4".
    output_path: String,
}

/// What a tool call hands back: the whole project state, so a caller never
/// needs a separate get_project round trip just to see the effect of an edit.
#[derive(Debug, Serialize, schemars::JsonSchema)]
struct ProjectView {
    name: String,
    clips: Vec<Clip>,
    timeline: Vec<TimelineItem>,
    total_duration: f64,
}

impl From<&Project> for ProjectView {
    fn from(project: &Project) -> Self {
        Self {
            name: project.name.clone(),
            clips: project.clips.clone(),
            timeline: project.timeline.clone(),
            total_duration: project.total_duration(),
        }
    }
}

/// One project open at a time, matching how the web and desktop shells work
/// today. `None` until new_project is called, so a stray tool call before
/// that gets a clear error rather than silently touching a phantom project.
#[derive(Clone)]
struct OpenCutServer {
    project: Arc<Mutex<Option<Project>>>,
    // Read by the #[tool_router]/#[tool_handler] macro expansion, which the
    // dead-code lint cannot see through.
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

impl OpenCutServer {
    fn new() -> Self {
        Self {
            project: Arc::new(Mutex::new(None)),
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl OpenCutServer {
    #[tool(
        description = "Start a new, empty project, replacing any project already open in this session."
    )]
    async fn new_project(
        &self,
        Parameters(NewProjectRequest { name }): Parameters<NewProjectRequest>,
    ) -> Json<ProjectView> {
        let mut guard = self.project.lock().await;
        let project = Project::new(name);
        let view = ProjectView::from(&project);
        *guard = Some(project);
        Json(view)
    }

    #[tool(
        description = "Import a video file from disk into the current project's media pool and append it to the end of the timeline."
    )]
    async fn import_clip(
        &self,
        Parameters(ImportClipRequest { path }): Parameters<ImportClipRequest>,
    ) -> Result<Json<Clip>, ErrorData> {
        let path_buf = PathBuf::from(&path);
        let info = opencut_core::ffmpeg::probe(&path_buf).map_err(ffmpeg_error)?;
        let name = path_buf
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone());
        let clip = Clip::new(name, path_buf, info);

        let mut guard = self.project.lock().await;
        let project = guard.as_mut().ok_or_else(no_project)?;
        project.add_clip(clip.clone());
        Ok(Json(clip))
    }

    #[tool(
        description = "Change a clip's in and out points. Applies to every place that clip appears on the timeline."
    )]
    async fn trim_clip(
        &self,
        Parameters(TrimClipRequest {
            clip_id,
            in_point,
            out_point,
        }): Parameters<TrimClipRequest>,
    ) -> Result<Json<ProjectView>, ErrorData> {
        let mut guard = self.project.lock().await;
        let project = guard.as_mut().ok_or_else(no_project)?;
        project
            .trim_clip(&clip_id, in_point, out_point)
            .map_err(project_error)?;
        Ok(Json(ProjectView::from(&*project)))
    }

    #[tool(
        description = "Remove a clip from the project's media pool and every place it appears on the timeline."
    )]
    async fn remove_clip(
        &self,
        Parameters(RemoveClipRequest { clip_id }): Parameters<RemoveClipRequest>,
    ) -> Result<Json<ProjectView>, ErrorData> {
        let mut guard = self.project.lock().await;
        let project = guard.as_mut().ok_or_else(no_project)?;
        project.remove_clip(&clip_id).map_err(project_error)?;
        Ok(Json(ProjectView::from(&*project)))
    }

    #[tool(description = "Move a timeline item to a new position, shifting the rest of the cut.")]
    async fn reorder_clip(
        &self,
        Parameters(ReorderClipRequest { from, to }): Parameters<ReorderClipRequest>,
    ) -> Result<Json<ProjectView>, ErrorData> {
        let mut guard = self.project.lock().await;
        let project = guard.as_mut().ok_or_else(no_project)?;
        project.reorder(from, to).map_err(project_error)?;
        Ok(Json(ProjectView::from(&*project)))
    }

    #[tool(
        description = "Read the current project: every clip in the media pool, the timeline order and trim points, and the total cut length."
    )]
    async fn get_project(&self) -> Result<Json<ProjectView>, ErrorData> {
        let guard = self.project.lock().await;
        let project = guard.as_ref().ok_or_else(no_project)?;
        Ok(Json(ProjectView::from(project)))
    }

    #[tool(
        description = "Render the current project's timeline to a single video file at the given path. Video only; no audio track yet."
    )]
    async fn export_project(
        &self,
        Parameters(ExportProjectRequest { output_path }): Parameters<ExportProjectRequest>,
    ) -> Result<String, ErrorData> {
        let guard = self.project.lock().await;
        let project = guard.as_ref().ok_or_else(no_project)?;
        opencut_core::ffmpeg::export(project, Path::new(&output_path)).map_err(ffmpeg_error)?;
        Ok(format!(
            "Exported {:.1}s to {output_path}",
            project.total_duration()
        ))
    }
}

#[tool_handler]
impl ServerHandler for OpenCutServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "Edit a video project: new_project, import_clip, trim_clip, remove_clip, \
                 reorder_clip, get_project, export_project. One project is open at a time; \
                 new_project replaces it.",
            )
            .with_server_info(Implementation::new("opencut", env!("CARGO_PKG_VERSION")))
    }
}

/// Checks the `Authorization: Bearer <token>` header against the token this
/// server was started with. The MCP tool layer has no auth concept of its
/// own; this is the only thing standing between the public internet and
/// export_project once this binary runs as a hosted HTTP service.
///
/// ponytail: verified manually with curl (no-token, wrong-token, correct-token
/// -> 401/401/200), not by an automated test. Add a reqwest-based test
/// alongside smoke.rs's stdio one before this ever carries real traffic.
async fn require_bearer_token(
    axum::extract::State(expected_token): axum::extract::State<Arc<str>>,
    headers: axum::http::HeaderMap,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, axum::http::StatusCode> {
    let provided = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    if provided == Some(&*expected_token) {
        Ok(next.run(request).await)
    } else {
        Err(axum::http::StatusCode::UNAUTHORIZED)
    }
}

/// Serves over Streamable HTTP with bearer-token auth, for hosts (Cowork,
/// or any MCP client without a way to spawn a local process) that cannot
/// reach a stdio server on this machine.
async fn serve_http() -> anyhow::Result<()> {
    let token: Arc<str> = std::env::var("OPENCUT_MCP_BEARER_TOKEN")
        .map_err(|_| {
            anyhow::anyhow!(
                "OPENCUT_MCP_TRANSPORT=http requires OPENCUT_MCP_BEARER_TOKEN to be set. \
                 Generate one and set it as an env var on both this server and every client \
                 that calls it."
            )
        })?
        .into();
    let port: u16 = std::env::var("OPENCUT_MCP_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8090);
    // Empty by default: StreamableHttpServerConfig's own default (loopback
    // only) still applies, so a bare `cargo run` stays safe. Set this once
    // the service has a real public hostname (see Cargo.toml / README for
    // where Railway reports it).
    let allowed_hosts: Vec<String> = std::env::var("OPENCUT_MCP_ALLOWED_HOSTS")
        .map(|value| {
            value
                .split(',')
                .map(|host| host.trim().to_string())
                .filter(|host| !host.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let mut config = StreamableHttpServerConfig::default();
    if !allowed_hosts.is_empty() {
        config.allowed_hosts = allowed_hosts;
    }

    let mcp_service: StreamableHttpService<OpenCutServer, LocalSessionManager> =
        StreamableHttpService::new(
            || Ok(OpenCutServer::new()),
            Arc::new(LocalSessionManager::default()),
            config,
        );

    let router = axum::Router::new().nest_service("/mcp", mcp_service).layer(
        axum::middleware::from_fn_with_state(token, require_bearer_token),
    );

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    eprintln!("opencut-mcp listening on :{port}/mcp (bearer token required)");
    axum::serve(listener, router).await?;
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let transport = std::env::var("OPENCUT_MCP_TRANSPORT").unwrap_or_else(|_| "stdio".to_string());
    match transport.as_str() {
        "http" => serve_http().await,
        "stdio" => {
            let service = OpenCutServer::new().serve(stdio()).await?;
            service.waiting().await?;
            Ok(())
        }
        other => {
            anyhow::bail!("unknown OPENCUT_MCP_TRANSPORT {other:?}: expected \"stdio\" or \"http\"")
        }
    }
}
