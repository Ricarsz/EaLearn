use serde::{Deserialize, Serialize};

use crate::entry::Token;

/// 情景模式（剧本线）：一段会话就是一个节点图。
///
/// 节点之间的跳转只允许前向，不允许成环——成环会让 SRS 统计和
/// 「本段已完成」的判定失去意义。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dialog {
    pub id: String,
    pub title: String,
    /// 主题：gossip / smalltalk / dating / complaint / shopping ...
    pub theme: String,
    /// 起始节点 id。
    pub start: String,
    pub nodes: Vec<Node>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub speaker: Speaker,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tokens: Vec<Token>,
    /// 说完直接进入的下一节点。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    /// 用户可选回复。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<Choice>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Speaker {
    /// 对话方（对方发来的消息）
    A,
    /// 学习者
    B,
    /// 旁白 / 场景说明
    Narrator,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    pub text: String,
    pub next: String,
    /// 选了之后对方怎么回，纯展示用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply: Option<String>,
    /// 该选项是否在语法/礼貌度上不合适，选了也放行但给出提示。
    #[serde(default)]
    pub awkward: bool,
}

impl Dialog {
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn start_node(&self) -> Option<&Node> {
        self.node(&self.start)
    }

    /// 结构校验：所有跳转目标存在、无环、起始节点存在。
    pub fn is_well_formed(&self) -> bool {
        if self.start_node().is_none() {
            return false;
        }
        let ids: Vec<&str> = self.nodes.iter().map(|n| n.id.as_str()).collect();
        for node in &self.nodes {
            let mut targets: Vec<&str> = node.choices.iter().map(|c| c.next.as_str()).collect();
            if let Some(next) = &node.next {
                targets.push(next);
            }
            if targets.iter().any(|t| !ids.contains(t)) {
                return false;
            }
        }
        !self.has_cycle()
    }

    fn has_cycle(&self) -> bool {
        // 三色 DFS，节点数很小，不需要优化。
        #[derive(Clone, Copy, PartialEq)]
        enum Mark {
            White,
            Gray,
            Black,
        }
        let mut marks: Vec<Mark> = vec![Mark::White; self.nodes.len()];
        let index: Vec<&str> = self.nodes.iter().map(|n| n.id.as_str()).collect();

        fn visit(i: usize, nodes: &[Node], index: &[&str], marks: &mut [Mark]) -> bool {
            marks[i] = Mark::Gray;
            let node = &nodes[i];
            let mut targets: Vec<&str> = node.choices.iter().map(|c| c.next.as_str()).collect();
            if let Some(next) = &node.next {
                targets.push(next);
            }
            for t in targets {
                let Some(j) = index.iter().position(|x| *x == t) else {
                    continue;
                };
                match marks[j] {
                    Mark::Gray => return true,
                    Mark::White => {
                        if visit(j, nodes, index, marks) {
                            return true;
                        }
                    }
                    Mark::Black => {}
                }
            }
            marks[i] = Mark::Black;
            false
        }

        for i in 0..self.nodes.len() {
            if marks[i] == Mark::White && visit(i, &self.nodes, &index, &mut marks) {
                return true;
            }
        }
        false
    }
}
