//! # classifier_approvals — 分类器审批追踪
//!
//! 对应 TypeScript `utils/classifierApprovals.ts`。
//!
//! 追踪哪些工具使用被分类器自动批准。

use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// 分类器类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassifierType {
    Bash,
    AutoMode,
}

/// 分类器审批记录
#[derive(Debug, Clone)]
pub struct ClassifierApproval {
    pub classifier: ClassifierType,
    pub matched_rule: Option<String>,
    pub reason: Option<String>,
}

/// 信号订阅回调类型
type Listener = Box<dyn Fn() + Send + Sync>;

/// 分类器审批管理器
pub struct ClassifierApprovals {
    approvals: Mutex<HashMap<String, ClassifierApproval>>,
    checking: Mutex<HashSet<String>>,
    listeners: Mutex<Vec<Arc<Listener>>>,
    bash_classifier_enabled: bool,
    transcript_classifier_enabled: bool,
}

impl ClassifierApprovals {
    pub fn new(bash_classifier_enabled: bool, transcript_classifier_enabled: bool) -> Self {
        Self {
            approvals: Mutex::new(HashMap::new()),
            checking: Mutex::new(HashSet::new()),
            listeners: Mutex::new(Vec::new()),
            bash_classifier_enabled,
            transcript_classifier_enabled,
        }
    }

    /// 设置 bash 分类器审批
    pub fn set_classifier_approval(&self, tool_use_id: &str, matched_rule: &str) {
        if !self.bash_classifier_enabled {
            return;
        }
        self.approvals.lock().insert(
            tool_use_id.to_string(),
            ClassifierApproval {
                classifier: ClassifierType::Bash,
                matched_rule: Some(matched_rule.to_string()),
                reason: None,
            },
        );
    }

    /// 获取 bash 分类器审批
    pub fn get_classifier_approval(&self, tool_use_id: &str) -> Option<String> {
        if !self.bash_classifier_enabled {
            return None;
        }
        let approvals = self.approvals.lock();
        let approval = approvals.get(tool_use_id)?;
        if approval.classifier != ClassifierType::Bash {
            return None;
        }
        approval.matched_rule.clone()
    }

    /// 设置 yolo（自动模式）分类器审批
    pub fn set_yolo_classifier_approval(&self, tool_use_id: &str, reason: &str) {
        if !self.transcript_classifier_enabled {
            return;
        }
        self.approvals.lock().insert(
            tool_use_id.to_string(),
            ClassifierApproval {
                classifier: ClassifierType::AutoMode,
                matched_rule: None,
                reason: Some(reason.to_string()),
            },
        );
    }

    /// 获取 yolo 分类器审批
    pub fn get_yolo_classifier_approval(&self, tool_use_id: &str) -> Option<String> {
        if !self.transcript_classifier_enabled {
            return None;
        }
        let approvals = self.approvals.lock();
        let approval = approvals.get(tool_use_id)?;
        if approval.classifier != ClassifierType::AutoMode {
            return None;
        }
        approval.reason.clone()
    }

    /// 设置分类器检查中状态
    pub fn set_classifier_checking(&self, tool_use_id: &str) {
        if !self.bash_classifier_enabled && !self.transcript_classifier_enabled {
            return;
        }
        self.checking.lock().insert(tool_use_id.to_string());
        self.emit();
    }

    /// 清除分类器检查中状态
    pub fn clear_classifier_checking(&self, tool_use_id: &str) {
        if !self.bash_classifier_enabled && !self.transcript_classifier_enabled {
            return;
        }
        self.checking.lock().remove(tool_use_id);
        self.emit();
    }

    /// 检查是否正在分类器检查中
    pub fn is_classifier_checking(&self, tool_use_id: &str) -> bool {
        self.checking.lock().contains(tool_use_id)
    }

    /// 删除分类器审批
    pub fn delete_classifier_approval(&self, tool_use_id: &str) {
        self.approvals.lock().remove(tool_use_id);
    }

    /// 清除所有分类器审批和检查状态
    pub fn clear_classifier_approvals(&self) {
        self.approvals.lock().clear();
        self.checking.lock().clear();
        self.emit();
    }

    /// 订阅分类器检查状态变化
    pub fn subscribe_classifier_checking(&self, listener: impl Fn() + Send + Sync + 'static) {
        self.listeners.lock().push(Arc::new(Box::new(listener)));
    }

    /// 发射事件通知所有监听器
    fn emit(&self) {
        let listeners = self.listeners.lock().clone();
        for listener in &listeners {
            listener();
        }
    }
}
