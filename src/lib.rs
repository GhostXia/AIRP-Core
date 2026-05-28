//! # AIRP-Core: 有状态流式 SSE 网关
//!
//! 角色扮演（RP）后端：在客户端与上游 OpenAI 兼容 API 之间插入一层守护进程，
//! 负责注入角色卡 / 世界书 / 预设 / 历史卷上下文，并对流式响应进行 FSM 过滤
//! 与 XML 解包（`<think>` / `<action>` 分离）。
//!
//! ## 公开模块（外部消费者可直接 use）
//! - [`config`] · [`daemon`] · [`chat_pipeline`] · [`data_dir`] · [`png_parser`]
//! - [`adapter`] · [`chat_store`] · [`error`] · [`orchestrator`]
//!
//! ## 内部模块（pub(crate)：实现细节，不对外保证 API 稳定）
//! - `fsm` · `xml_unpacker` · `auto_converter`
//! - `volume_store` · `volume_manager` · `index_parser`
//!
//! 设计概览参见仓库根目录 `REFACTOR_PLAN.md`。

pub mod adapter;
pub mod chat_pipeline;
pub mod chat_store;
pub mod config;
pub mod daemon;
pub mod data_dir;
pub mod error;
pub mod mcp;
pub mod orchestrator;
pub mod png_parser;
pub mod quota;
pub mod scene;
pub mod sync;
pub mod types;

// M0 F-50 / 6.0n：实现细节模块收紧为 pub(crate)，仅 crate 内部互调。
// 这些模块不被 main.rs / 外部消费者直接引用，未来重构无 API 兼容包袱。
// 注：M4.5 完成后 `fsm` 不再被 main.rs 直接引用，已降为 pub(crate)。
pub(crate) mod auto_converter;
pub(crate) mod fsm;
pub(crate) mod index_parser;
pub(crate) mod preset_regex;
pub(crate) mod volume_manager;
pub(crate) mod volume_store;
pub(crate) mod xml_unpacker;
