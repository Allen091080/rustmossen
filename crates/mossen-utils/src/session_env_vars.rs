//! # session_env_vars — 会话环境变量
//!
//! 对应 TypeScript `utils/sessionEnvVars.ts`。
//! 通过 /env 设置的会话作用域环境变量。

use std::collections::HashMap;
use std::sync::Mutex;

/// 会话环境变量映射。
static SESSION_ENV_VARS: std::sync::LazyLock<Mutex<HashMap<String, String>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// 获取会话环境变量映射。
pub fn get_session_env_vars() -> HashMap<String, String> {
    SESSION_ENV_VARS.lock().unwrap().clone()
}

/// 设置会话环境变量。
pub fn set_session_env_var(name: String, value: String) {
    SESSION_ENV_VARS.lock().unwrap().insert(name, value);
}

/// 删除会话环境变量。
pub fn delete_session_env_var(name: &str) {
    SESSION_ENV_VARS.lock().unwrap().remove(name);
}

/// 清除所有会话环境变量。
pub fn clear_session_env_vars() {
    SESSION_ENV_VARS.lock().unwrap().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_and_get() {
        clear_session_env_vars();
        set_session_env_var("TEST_VAR".to_string(), "test_value".to_string());
        let vars = get_session_env_vars();
        assert_eq!(vars.get("TEST_VAR"), Some(&"test_value".to_string()));
    }

    #[test]
    fn test_delete() {
        clear_session_env_vars();
        set_session_env_var("DELETE_ME".to_string(), "value".to_string());
        delete_session_env_var("DELETE_ME");
        let vars = get_session_env_vars();
        assert!(vars.get("DELETE_ME").is_none());
    }
}
