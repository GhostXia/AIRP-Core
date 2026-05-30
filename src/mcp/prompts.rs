use rmcp::ErrorData;

use super::AirpMcpServer;

/// 静态文本：filter_text prompt 内容。
pub(super) fn filter_text_prompt() -> String {
    String::from(
        "你是文本筛选 Agent。给定主 LLM 输出片段，你的任务：\n\
         1. 保留小说正文（叙述、对话、动作描写）\n\
         2. 剥除：<think>/<thought>/<status> 等元数据标签及其内容\n\
         3. 剥除：[卷评估]/[OOC]/[Author Note] 等编辑标记\n\
         4. 不改写正文一字\n\
         5. 输出格式：纯文本（无解释、无前缀）",
    )
}

/// 静态文本：state_update prompt 内容。
pub(super) fn state_update_prompt() -> String {
    String::from(
        "本轮回复结束前，请在 <state>{...JSON...}</state> 标签内输出当前游戏状态更新。\n\
         字段示例（缺失字段可省略）：\n\
         { \"hp\": 80, \"mp\": 50, \"time\": \"晌午\", \"location\": \"客栈大堂\", \
         \"npcs\": [\"张三\"], \"quest\": \"打听消息\" }\n\
         若本轮状态未变化，可省略此标签。",
    )
}

/// M_CA: Agent-driven 角色卡分析 workflow prompt。
/// 指导 Agent 读取卡 + 世界书，写入 analysis/profile.md + analysis/tier.json + style/guide.md + cot/strategy.md。
pub(super) fn analyze_character_card_prompt(character_id: &str) -> String {
    format!(
        "你是角色卡分析 Agent。请按以下步骤分析角色 `{cid}`：\n\
         \n\
         **步骤 0：歧义自检（必做，先于一切读取）**\n\
         在分析前，先确认 `{cid}` 确实是用户**本次明确指定**要拆解的对象。若用户描述模糊（如\"拆解我的角色卡\"未指明是哪一个），**必须停下来询问用户**，不得擅自假设：\n\
         - 要拆的是**已存入 airp 的角色**（`airp://characters` 列表中的条目）？还是\n\
         - 用户**本次新提供、尚未存入**的文件（如工作目录下的 .json / .png）？\n\
         若属后者，应先 `import_card` 导入再分析，或直接读取用户提供的文件 —— **切勿**把 `airp://characters` 列表里的历史条目无差别全部拆解。确认无误后再进入步骤 1。\n\
         \n\
         **步骤 1：读取角色卡**\n\
         读取资源 `airp://characters/{cid}/card`，获取完整 TavernV2 JSON。\n\
         \n\
         **步骤 2：读取世界书（可选）**\n\
         读取资源 `airp://characters/{cid}/world/lorebook`，分析关键词与条目。\n\
         \n\
         **步骤 3：生成分析产物**（依次调用 `write_character_artifact`）\n\
         - `analysis/profile.md` — 角色性格、背景、说话风格、禁忌/规则摘要\n\
         - `analysis/tier.json` — 复杂度分级 JSON（见下方格式）\n\
         - `style/guide.md` — 文风提炼（用词、句式、语气特征、典型句型示例）\n\
         - `cot/strategy.md` — RP CoT 策略（如何维持角色一致性、如何处理边界情形）\n\
         \n\
         **tier.json 格式**：\n\
         ```json\n\
         {{\n\
           \"tier\": 1,\n\
           \"label\": \"简单\",\n\
           \"reasoning\": \"角色规则简单，无复杂世界设定\",\n\
           \"lorebook_entries\": 0,\n\
           \"has_custom_rules\": false,\n\
           \"has_state_tracking\": false\n\
         }}\n\
         ```\n\
         等级说明：\n\
         - Tier 1（简单）：基础性格，无定制规则，lorebook ≤ 5 条\n\
         - Tier 2（中等）：有世界书 6~20 条，或有部分定制规则\n\
         - Tier 3（复杂）：lorebook > 20 条，或有状态追踪，或有复杂剧情分支\n\
         - Tier 4（极复杂）：多角色场景，定制输出结构，状态机\n\
         \n\
         **步骤 4：状态 schema 推断（M_LS LS-7，仅 has_state_tracking=true 时执行）**\n\
         若角色卡存在状态追踪（时间/数值/位置等动态字段），调用 `write_character_artifact` 写入：\n\
         - `state/schema.json` — 状态字段 schema，格式：\n\
         ```json\n\
         {{\n\
           \"fields\": [\n\
             {{\"key\": \"hp\", \"type\": \"number\", \"min\": 0, \"max\": 100, \"label\": \"生命值\"}},\n\
             {{\"key\": \"location\", \"type\": \"string\", \"label\": \"当前位置\"}},\n\
             {{\"key\": \"time\", \"type\": \"string\", \"label\": \"时间\"}}\n\
           ]\n\
         }}\n\
         ```\n\
         字段说明：`key`=状态 JSON 键名；`type`=number|string|array|boolean；\n\
         `min`/`max` 仅对 number 类型；`label` 为中文显示名称。\n\
         若 has_state_tracking=false，跳过此步骤。\n\
         \n\
         **步骤 5：验证**\n\
         读取 `airp://characters/{cid}/artifacts`，确认所有产物路径已出现。",
        cid = character_id
    )
}

/// M_CA: Agent-driven 预设分析 workflow prompt。
/// 指导 Agent 读取 preset.json，写入 analysis/summary.md + analysis/regex_scripts.json + style/instructions.md。
pub(super) fn analyze_preset_prompt(preset_id: &str) -> String {
    format!(
        "你是预设分析 Agent。请按以下步骤分析预设 `{pid}`：\n\
         \n\
         **步骤 0：歧义自检（必做，先于一切读取）**\n\
         在分析前，先确认 `{pid}` 确实是用户**本次明确指定**要拆解的对象。若用户描述模糊（如\"拆解我的预设\"未指明是哪一个），**必须停下来询问用户**，不得擅自假设：\n\
         - 要拆的是**已存入 airp 的预设**（`airp://presets` 列表中的条目）？还是\n\
         - 用户**本次新提供、尚未存入**的文件（如工作目录下的 .json）？\n\
         若属后者，应先 `import_preset` 导入再分析，或直接读取用户提供的文件 —— **切勿**把 `airp://presets` 列表里的历史条目无差别全部拆解。确认无误后再进入步骤 1。\n\
         \n\
         **步骤 1：读取预设**\n\
         读取资源 `airp://presets/{pid}/raw`，获取完整 SillyTavern Preset JSON 原文。\n\
         \n\
         **步骤 2：生成分析产物**（依次调用 `write_preset_artifact`）\n\
         - `analysis/summary.md` — Prompt 列表、顺序、各段用途说明\n\
         - `analysis/regex_scripts.json` — 正则过滤脚本 JSON 数组（每项含 name / pattern / flags / purpose）\n\
         - `style/instructions.md` — 提取出的写作指令（文风、格式要求、禁忌）\n\
         \n\
         **步骤 3：验证**\n\
         读取 `airp://presets/{pid}/artifacts`，确认所有产物路径已出现。",
        pid = preset_id
    )
}

impl AirpMcpServer {
    /// MCP-4: 装配 system prompt 公共逻辑（供 start_session 工具与 get_prompt 复用）。
    ///
    /// 加载 card / lorebook / preset → 调 orchestrator → 返完整 system prompt 字符串。
    pub(super) fn assemble_system_prompt(
        &self,
        character_id: &str,
        preset_id: Option<&str>,
        user_name: &str,
    ) -> Result<String, ErrorData> {
        crate::data_dir::validate_id_segment(character_id)
            .map_err(|e| ErrorData::invalid_params(format!("非法 character_id: {}", e), None))?;
        let char_dir = self.data_root.join("characters").join(character_id);
        let card_path = if char_dir.join("card").join("raw.json").exists() {
            char_dir.join("card").join("raw.json")
        } else if char_dir.join("card.json").exists() {
            char_dir.join("card.json")
        } else {
            return Err(ErrorData::invalid_params(
                format!("角色 {} 无 card.json", character_id),
                None,
            ));
        };
        let card_json = std::fs::read_to_string(&card_path)
            .map_err(|e| ErrorData::internal_error(format!("读 card 失败: {}", e), None))?;
        let card_json = crate::data_dir::strip_utf8_bom(&card_json).to_owned();

        let lb_path = crate::data_dir::char_world_lorebook_path(&self.data_root, character_id);
        let lorebook_json = if lb_path.exists() {
            std::fs::read_to_string(&lb_path)
                .ok()
                .map(|s| crate::data_dir::strip_utf8_bom(&s).to_owned())
        } else {
            None
        };

        let preset_json = preset_id.and_then(|pid| {
            let new_path = crate::data_dir::preset_json_path(&self.data_root, pid);
            let legacy = self.data_root.join("presets").join(format!("{}.json", pid));
            let p = if new_path.exists() { new_path } else { legacy };
            std::fs::read_to_string(&p)
                .ok()
                .map(|s| crate::data_dir::strip_utf8_bom(&s).to_owned())
        });

        let orch =
            crate::orchestrator::Orchestrator::new(Some(&card_json), lorebook_json.as_deref())
                .map_err(|e| {
                    ErrorData::internal_error(format!("Orchestrator 构造失败: {}", e), None)
                })?;

        let variables = std::collections::HashMap::new();
        let sp = orch.build_system_prompt_with_preset(
            &self.data_root,
            Some(character_id),
            user_name,
            &variables,
            "",
            preset_json.as_deref(),
            None,
            "",
        );
        Ok(sp)
    }
}
