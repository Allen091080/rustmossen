//! 语义布尔类型
//!
//! 对应 TS `semanticBoolean.ts`。

/// 接受字符串 "true"/"false" 的布尔值。
///
/// 工具输入以模型生成的 JSON 形式到达。模型偶尔会引用布尔值——
/// `"replace_all":"false"` 而不是 `"replace_all":false`，
/// 而 z.boolean() 会以类型错误拒绝。
///
/// z.preprocess 仍然向 API schema 发出 {"type":"boolean"}，
/// 所以模型仍然被告知这是布尔值——字符串容错是客户端的隐式转换，
/// 不是公开的输入形状。
pub fn semantic_boolean() -> SemanticBoolean {
    SemanticBoolean
}

/// 语义布尔反序列化器。
#[derive(Debug)]
pub struct SemanticBoolean;

impl<'de> serde::de::Deserialize<'de> for SemanticBoolean {
    fn deserialize<D>(deserializer: D) -> Result<SemanticBoolean, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum BoolOrString {
            Bool(bool),
            Str(String),
        }

        let _value = BoolOrString::deserialize(deserializer)?;
        // 在这里我们简单地忽略解析的值，因为这个类型主要用于其反序列化行为
        Ok(SemanticBoolean)
    }
}
