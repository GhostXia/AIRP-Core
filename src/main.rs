use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;

use airp_core::chat_pipeline;
use airp_core::config::AppConfig;
use airp_core::daemon::{
    create_router, ChatCompletionRequest, DaemonState, MutableConfig, UserProfile,
};
use airp_core::error::AirpError;
use airp_core::types::CharacterId;

#[derive(Parser)]
#[command(
    name = "airp-core",
    author = "AIRP Team",
    version = "0.1.0",
    about = "AIRP Stateful Streaming Gateway CLI"
)]
struct Cli {
    /// 配置文件路径，默认会在当前目录下创建/加载 config.json
    #[arg(short, long, default_value = "config.json")]
    config: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug, Clone)]
enum Commands {
    /// 启动守护进程 (Daemon SSE 网关)
    Daemon {
        /// 监听端口，若不指定则使用配置文件中的 daemon_port 端口 (默认 8000)
        #[arg(short, long)]
        port: Option<u16>,
    },
    /// M_MCP MCP-1: 以 MCP Server 模式运行（stdio JSON-RPC）。
    ///
    /// Claude Code / Cursor / 任何 MCP-aware client 通过启子进程方式连接：
    /// 在 client 配置中加 `{"command": "airp-core", "args": ["mcp"]}`。
    Mcp {
        /// 数据根目录，默认沿用 AIRP_DATA_DIR 环境变量或 ./data
        #[arg(long)]
        data_dir: Option<PathBuf>,
    },
    /// AUDIT-3: 列出所有 MCP 工具（含 description / input schema / annotations）。
    /// 输出 JSON，可用于 CI 校验 README / docs 与代码同步。
    ListTools {
        /// 输出格式：`json`（默认）或 `summary`（简表，仅 name + side_effect）。
        #[arg(long, default_value = "json")]
        format: String,
    },
    /// 一次性调用单个 MCP 工具（同步、不起 server），用于 agent shell 自动化。
    ///
    /// 例：
    ///   airp-core tool ping
    ///   airp-core tool list_sessions --json '{"character_id":"alice"}'
    ///   airp-core tool append_message --json '{"character_id":"alice","role":"user","content":"hi"}'
    Tool {
        /// 工具名（用 `airp-core list-tools --format summary` 查看可用名）。
        name: String,
        /// 工具入参 JSON 对象字符串；缺省 `{}`。
        #[arg(long, default_value = "{}")]
        json: String,
        /// 数据根目录，默认沿用 AIRP_DATA_DIR 或 ./data。
        #[arg(long)]
        data_dir: Option<PathBuf>,
    },
    /// 在终端控制台直接运行单次流式过滤
    Run {
        /// 角色卡：PNG 路径 / JSON 文件路径 / `{...}` 内联 JSON / data/characters/{id} 文件夹名
        #[arg(short, long)]
        character: Option<String>,

        /// 世界书 JSON 文件的任意路径
        #[arg(short, long)]
        lorebook: Option<String>,

        /// 用户当前输入的 Message 提示词
        #[arg(short, long)]
        message: String,

        /// 正则过滤表达式列表 (如: "<thought>[\\s\\S]*?<\\/thought>")
        #[arg(short, long)]
        filters: Vec<String>,

        /// 运行时用户名称，默认为 "User"
        #[arg(long, default_value = "User")]
        user_name: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // M1.9 / M2 前置：初始化 tracing subscriber，让 daemon 里 warn/error/debug 可见。
    // 支持 AIRP_LOG 环境变量自定义级别，默认 info。
    let log_level = std::env::var("AIRP_LOG").unwrap_or_else(|_| "info".to_string());
    // M_MCP MCP-1: tracing 输出必须走 stderr，否则会污染 MCP stdio JSON-RPC 通道。
    // Daemon 的 println! / Run 的流式正文仍走 stdout，互不影响。
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(log_level))
        .with_target(false)
        .with_writer(std::io::stderr)
        .try_init();

    // 1. 解析命令行参数
    let cli = Cli::parse();

    // 2. 配置三层合并（M4.3）：
    //    layer 1 = AppConfig::default() （程序默认）
    //    layer 2a = `config.json`（向后兼容）
    //    layer 2b = `data/settings.json`（用户层，覆盖上一层非空字段）
    //    layer 3 = 环境变量
    //    layer 4 = HTTP 请求 body（在 chat_pipeline::prepare_pipeline 里处理）
    let mut app_config = AppConfig::load_or_create(&cli.config)?;
    let data_root = airp_core::data_dir::resolve_data_root();
    airp_core::data_dir::ensure_data_dirs(&data_root)?;
    app_config.merge_data_settings(&data_root)?;
    app_config.override_with_env();
    // M0 F-12 / 5.0b：全部合并完成后 fast-fail 校验跨字段不变量
    app_config.validate()?;
    tracing::info!(
        endpoint = %app_config.endpoint,
        model = %app_config.model,
        has_api_key = app_config.api_key.is_some(),
        data_root = %data_root.display(),
        "config loaded"
    );

    // M0 F-01：构造一份共享 HTTP 客户端，整个进程生命周期复用其连接池。
    let http_client = reqwest::Client::new();

    match cli.command {
        Commands::Daemon { port } => {
            let daemon_port = port.unwrap_or(app_config.daemon_port);

            let state = Arc::new(DaemonState {
                data_root,
                http_client: http_client.clone(),
                config: std::sync::RwLock::new(MutableConfig {
                    provider: app_config.provider,
                    endpoint: app_config.endpoint,
                    api_key: app_config.api_key,
                    model: app_config.model,
                    volume_config: app_config.volume,
                    access_api_key: app_config.access_api_key,
                    engine: app_config.engine,
                    quota: app_config.quota,
                }),
                // MCP-7: resource subscription registry shared with mcp_http_router
                state_subs: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            });

            let router = create_router(state);
            let addr = format!("127.0.0.1:{}", daemon_port);
            let listener = TcpListener::bind(&addr).await?;

            println!("AIRP-Core Gateway running at http://{}", addr);
            println!("Open your browser and visit the address above.");
            // M6.4：限流层需要 `ConnectInfo<SocketAddr>` 来识别客户端 IP；
            // 用 `into_make_service_with_connect_info` 让 axum 把 peer 地址注入扩展。
            axum::serve(
                listener,
                router.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await?;
        }
        Commands::Mcp { data_dir } => {
            // M_MCP MCP-1: stdio JSON-RPC 主循环。
            // 关键：所有 tracing 必须走 stderr，避免污染 stdout 的 JSON-RPC 通道。
            //
            // AUDIT-9: graceful shutdown on Ctrl+C. We hold a cancellation token
            // from the running service and trigger it from a separate signal task,
            // so the main `service.waiting()` future resolves cleanly (rmcp flushes
            // pending writes + closes transport) instead of getting SIGKILL'd
            // mid-JSONL-append.
            use rmcp::{transport::stdio, ServiceExt};
            let root = data_dir
                .or_else(|| std::env::var("AIRP_DATA_DIR").ok().map(PathBuf::from))
                .unwrap_or_else(|| PathBuf::from("data"));
            // 确保数据根目录结构存在
            airp_core::data_dir::ensure_data_dirs(&root)?;
            tracing::info!(data_root = ?root, "MCP-1: 启动 MCP Server (stdio transport)");
            let server = airp_core::mcp::AirpMcpServer::new(root);
            let service = server.serve(stdio()).await?;

            // AUDIT-9: spawn signal handler that triggers graceful shutdown.
            let cancel_token = service.cancellation_token();
            let signal_task = tokio::spawn(async move {
                if tokio::signal::ctrl_c().await.is_ok() {
                    tracing::info!("AUDIT-9: 收到 Ctrl+C，请求 MCP 服务优雅停机");
                    cancel_token.cancel();
                }
            });

            let quit_reason = service.waiting().await?;
            // Abort the signal task if waiting() returned for any other reason
            // (e.g. client closed stdin). Idempotent — no-op if signal already fired.
            signal_task.abort();
            tracing::info!(?quit_reason, "MCP stdio server 已退出");
        }
        Commands::Tool { name, json, data_dir } => {
            // Single-shot MCP tool invocation. No HTTP / stdio server lifetime.
            let root = data_dir
                .or_else(|| std::env::var("AIRP_DATA_DIR").ok().map(PathBuf::from))
                .unwrap_or_else(|| PathBuf::from("data"));
            airp_core::data_dir::ensure_data_dirs(&root)?;
            let server = airp_core::mcp::AirpMcpServer::new(root);
            match server.call_tool_sync(&name, &json) {
                Ok(out) => println!("{}", out),
                Err(e) => {
                    eprintln!("error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::ListTools { format } => {
            // AUDIT-3: enumerate tools from rmcp ToolRouter so docs can be
            // verified against code as the single source of truth.
            let server = airp_core::mcp::AirpMcpServer::new(PathBuf::from("data"));
            let tools = server.enumerate_tools();
            match format.as_str() {
                "summary" => {
                    println!("AIRP MCP tools — {} total", tools.len());
                    for t in &tools {
                        let side_effect = match &t.annotations {
                            Some(a) if a.read_only_hint == Some(true) => "readonly",
                            Some(a) if a.destructive_hint == Some(true) => "destructive",
                            Some(a) if a.idempotent_hint == Some(true) => "mutate-idempotent",
                            Some(_) => "mutate",
                            None => "(no annotations)",
                        };
                        println!("  {:<32}  {}", t.name, side_effect);
                    }
                }
                _ => {
                    // Default JSON output
                    let json = serde_json::to_string_pretty(&tools)?;
                    println!("{}", json);
                }
            }
        }
        Commands::Run {
            character,
            lorebook,
            message,
            filters,
            user_name,
        } => {
            // M4.5：将 character / lorebook 解析为可被 pipeline 直接消费的形式，
            // 然后构造与 daemon 完全相同的 ChatCompletionRequest + DaemonState，
            // 走 prepare_pipeline → run_pipeline_to_stdout 单一路径。

            // A. 角色卡：解析为内联 JSON 字符串（pipeline 识别 `{...}` 开头）
            //    或保留为 data/characters/{id} 文件夹名（CharacterId）。
            //    M5.0a：character_id 用 newtype，构造时即触发 validate_id_segment。
            let (character_id, character_card_id): (Option<CharacterId>, Option<String>) =
                match character {
                    None => (None, None),
                    Some(ref s) if s.trim().starts_with('{') => (None, Some(s.clone())),
                    Some(ref s) if s.to_lowercase().ends_with(".png") => {
                        let json = airp_core::png_parser::parse_png_character_card(s)?;
                        (None, Some(json))
                    }
                    Some(ref s)
                        if std::path::Path::new(s)
                            .extension()
                            .and_then(|e| e.to_str())
                            .map(|e| e.eq_ignore_ascii_case("json"))
                            .unwrap_or(false)
                            && std::path::Path::new(s).exists() =>
                    {
                        let json = std::fs::read_to_string(s)?;
                        (None, Some(json))
                    }
                    Some(s) => (Some(CharacterId::new(s)?), None),
                };

            // B. 世界书：从任意路径读为内联 JSON 传入 `lorebook_path`
            //    （prepare_pipeline 识别 `{...}` 开头跳过路径校验）
            let lorebook_inline: Option<String> = match lorebook {
                None => None,
                Some(ref p) => Some(std::fs::read_to_string(p)?),
            };

            // C. DaemonState：与 daemon 模式相同的字段集合
            let state = Arc::new(DaemonState {
                data_root,
                http_client: http_client.clone(),
                config: std::sync::RwLock::new(MutableConfig {
                    provider: app_config.provider,
                    endpoint: app_config.endpoint,
                    api_key: app_config.api_key,
                    model: app_config.model,
                    volume_config: app_config.volume,
                    access_api_key: None, // Run 命令不涉及 HTTP 鉴权
                    engine: app_config.engine,
                    quota: app_config.quota,
                }),
                state_subs: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            });

            // D. ChatCompletionRequest
            let payload = ChatCompletionRequest {
                character_id,
                character_card_id,
                lorebook_path: lorebook_inline,
                user_profile: UserProfile {
                    name: user_name,
                    variables: HashMap::new(),
                },
                message,
                messages_history: None,
                regex_filters: if filters.is_empty() {
                    None
                } else {
                    Some(filters)
                },
                preset_id: None,
                enabled_presets: None,
                session_id: None,
                provider: None,
                endpoint: None,
                api_key: None,
                model: None,
                temperature: None,
                max_tokens: None,
                scene_id: None,
                user_id: None,
            };

            // E. 走 pipeline（与 daemon 完全相同路径）
            let pipeline = chat_pipeline::prepare_pipeline(&payload, &state)
                .map_err(|e: AirpError| -> Box<dyn std::error::Error> { Box::new(e) })?;

            chat_pipeline::run_pipeline_to_stdout(pipeline).await?;
        }
    }

    Ok(())
}
