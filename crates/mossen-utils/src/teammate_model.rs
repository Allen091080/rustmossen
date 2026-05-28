//! # teammate_model — 队友模型选择
//!
//! 对应 TypeScript `utils/swarm/teammateModel.ts`。
//! 队友使用的默认模型选择逻辑。

/// 获取硬编码的队友模型回退值。
/// 当用户从未在 /config 中设置 teammateDefaultModel 时，新队友使用此值。
/// 必须是 provider 感知的，以便 Bedrock/Vertex/Foundry 客户获得正确的模型 ID。
///
/// 对应 TS `MOSSEN_MAX_4_6_CONFIG[getAPIProvider()]`。
pub fn get_hardcoded_teammate_model_fallback() -> String {
    use crate::model_utils::{get_api_provider, APIProvider, ALL_MODEL_CONFIGS};
    let config = match ALL_MODEL_CONFIGS.get("max46") {
        Some(c) => c,
        None => return "mossen-max-4-6".to_string(),
    };
    match get_api_provider() {
        APIProvider::FirstParty => config.first_party.clone(),
        APIProvider::Bedrock => config.bedrock.clone(),
        APIProvider::Vertex => config.vertex.clone(),
        APIProvider::Foundry => config.foundry.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hardcoded_teammate_model_fallback() {
        let model = get_hardcoded_teammate_model_fallback();
        assert!(!model.is_empty());
    }
}
