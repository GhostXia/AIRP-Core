use serde::{Deserialize, Serialize};

/// Tavern V2 规范的预设 Prompts。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TavernPrompt {
    pub identifier: String,
    pub name: String,
    /// SillyTavern 规范：缺少 `enabled` 字段时视为 `true`（默认启用）。
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// marker 类型 prompt 可能不含 role 字段。
    #[serde(default)]
    pub role: String,
    pub content: Option<String>,
    pub system_prompt: Option<bool>,
}

fn default_true() -> bool {
    true
}

/// Tavern V2 规范的预设配置包。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TavernPreset {
    pub prompts: Option<Vec<TavernPrompt>>,
    /// SillyTavern 预设级 temperature，作为 API 层默认值（可被 request body 覆盖）。
    pub temperature: Option<f32>,
    /// SillyTavern 预设级 max_tokens，字段名 `openai_max_tokens` 和 `max_tokens` 均接受。
    #[serde(alias = "openai_max_tokens")]
    pub max_tokens: Option<u32>,
    /// SillyTavern 预设级 model，作为 API 层默认值（可被 request body 覆盖）。
    #[serde(alias = "openai_model")]
    pub model: Option<String>,
}

/// Tavern V2 规范的角色卡内层数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterData {
    pub name: Option<String>,
    pub description: Option<String>,
    pub personality: Option<String>,
    pub scenario: Option<String>,
    pub first_mes: Option<String>,
    pub mes_template: Option<String>,
    pub system_prompt: Option<String>,
    /// 示例对话（SillyTavern mes_example 字段）。
    pub mes_example: Option<String>,
    /// ���个开场语（SillyTavern alternate_greetings）。
    #[serde(default)]
    pub alternate_greetings: Vec<String>,
    /// 角色卡内嵌世界书（SillyTavern character_book）。
    /// 保留为原始 JSON Value 以兼容不同 SillyTavern 版��的 entries 结构。
    pub character_book: Option<serde_json::Value>,
}

/// Tavern V2 规范的角色卡外层包装。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TavernCardV2 {
    pub spec: Option<String>,
    pub spec_version: Option<String>,
    pub data: CharacterData,
}
