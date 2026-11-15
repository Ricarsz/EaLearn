//! EaLearn 领域内核。
//!
//! 这里放所有与界面无关的判断：pack 的模型与校验、题型判分、SRS 调度。
//! 不依赖 Tauri、不依赖任何渲染框架，因此可以单测，也可以被 CLI 复用。

pub mod article;
pub mod dialog;
pub mod entry;
pub mod exercise;
pub mod import;
pub mod manifest;
pub mod pack;
pub mod plan;
pub mod srs;
pub mod validate;

pub use entry::{Entry, Token};
pub use exercise::{Exercise, Grade, GradeItem};
pub use manifest::Manifest;
pub use pack::Pack;
pub use plan::{plan_mode, plan_review_first, Coverage, DueItem, ExerciseKind, Mode};

/// 当前支持的 pack schema 版本。破坏性字段变更时递增。
pub const SCHEMA_VERSION: u32 = 1;
