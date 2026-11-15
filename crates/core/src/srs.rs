//! FSRS-5 调度。
//!
//! 纯函数，不读时钟：调用方传入 `now`（Unix 秒），便于测试与回放。
//! 参数用 FSRS-5 默认权重，未做用户个性化训练。

use serde::{Deserialize, Serialize};

/// 0..=4 的评分。`Again` 表示遗忘。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Rating {
    Again = 1,
    Hard = 2,
    Good = 3,
    Easy = 4,
}

impl Rating {
    pub fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            1 => Rating::Again,
            2 => Rating::Hard,
            3 => Rating::Good,
            4 => Rating::Easy,
            _ => return None,
        })
    }

    fn w(self, weights: &[f64; 19]) -> f64 {
        weights[self as usize - 1]
    }
}

pub const FSRS5_DEFAULT_WEIGHTS: [f64; 19] = [
    0.40255, 1.18385, 3.173, 15.69105, 7.1949, 0.5345, 1.4604, 0.0046, 1.54575, 0.1192, 1.01925,
    1.9395, 0.11, 0.29605, 2.2698, 0.2315, 2.9898, 0.51655, 0.6621,
];

const DECAY: f64 = -0.5;
const FACTOR: f64 = 19.0 / 81.0;
const MAX_STABILITY: f64 = 36500.0;
const MIN_STABILITY: f64 = 0.01;
const MAX_INTERVAL_DAYS: f64 = 36500.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardState {
    /// 记忆稳定性（天）。
    pub stability: f64,
    /// 难度 1–10。
    pub difficulty: f64,
    pub reps: u32,
    pub lapses: u32,
    /// 上次复习时间（Unix 秒）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_review: Option<i64>,
    /// 下次到期时间（Unix 秒）。
    pub due: i64,
}

impl Default for CardState {
    fn default() -> Self {
        Self {
            stability: 0.0,
            difficulty: 0.0,
            reps: 0,
            lapses: 0,
            last_review: None,
            due: 0,
        }
    }
}

impl CardState {
    /// 到期时（R = desired_retention）的间隔天数。
    pub fn interval_days(&self, desired_retention: f64) -> f64 {
        next_interval(self.stability, desired_retention)
    }
}

#[derive(Debug, Clone)]
pub struct Scheduler {
    pub weights: [f64; 19],
    pub desired_retention: f64,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self {
            weights: FSRS5_DEFAULT_WEIGHTS,
            desired_retention: 0.9,
        }
    }
}

impl Scheduler {
    /// 复习一次并返回新状态。
    pub fn review(&self, state: &CardState, rating: Rating, now: i64) -> CardState {
        let w = &self.weights;
        let elapsed_days = match state.last_review {
            Some(last) if now > last => (now - last) as f64 / 86_400.0,
            _ => 0.0,
        };

        let (stability, difficulty) = if state.reps == 0 {
            (
                rating.w(w).clamp(MIN_STABILITY, MAX_STABILITY),
                init_difficulty(rating, w),
            )
        } else {
            let r = retrievability(state.stability, elapsed_days);
            let d = next_difficulty(state.difficulty, rating, w);
            let s = if elapsed_days < 1.0 {
                // 同日重复：FSRS-5 的短期记忆项
                state.stability * (w[17] * (rating as i32 as f64 - 3.0 + w[18])).exp()
            } else if rating == Rating::Again {
                forget_stability(state.stability, state.difficulty, r, w)
            } else {
                recall_stability(state.stability, state.difficulty, r, rating, w)
            };
            (s.clamp(MIN_STABILITY, MAX_STABILITY), d)
        };

        let days = next_interval(stability, self.desired_retention);
        CardState {
            stability,
            difficulty,
            reps: state.reps + 1,
            lapses: state.lapses + u32::from(rating == Rating::Again),
            last_review: Some(now),
            due: now + (days * 86_400.0).round() as i64,
        }
    }
}

/// 当前可回忆概率。
pub fn retrievability(stability: f64, elapsed_days: f64) -> f64 {
    if stability <= 0.0 {
        return 0.0;
    }
    (1.0 + FACTOR * elapsed_days.max(0.0) / stability).powf(DECAY)
}

/// 达标 retention 对应的间隔天数。
pub fn next_interval(stability: f64, desired_retention: f64) -> f64 {
    let r = desired_retention.clamp(0.7, 0.99);
    let days = stability / FACTOR * (r.powf(1.0 / DECAY) - 1.0);
    days.clamp(1.0, MAX_INTERVAL_DAYS)
}

fn init_difficulty(rating: Rating, w: &[f64; 19]) -> f64 {
    let g = rating as i32 as f64;
    (w[4] - (w[5] * (g - 1.0)).exp() + 1.0).clamp(1.0, 10.0)
}

fn next_difficulty(d: f64, rating: Rating, w: &[f64; 19]) -> f64 {
    let g = rating as i32 as f64;
    let delta = -w[6] * (g - 3.0);
    // 线性阻尼：难度越高，同样的评分带来的变化越小
    let damped = d + delta * (10.0 - d) / 9.0;
    // 均值回归到「Good 的初始难度」
    let target = init_difficulty(Rating::Good, w);
    (w[7] * target + (1.0 - w[7]) * damped).clamp(1.0, 10.0)
}

fn recall_stability(s: f64, d: f64, r: f64, rating: Rating, w: &[f64; 19]) -> f64 {
    let hard_penalty = if rating == Rating::Hard { w[15] } else { 1.0 };
    let easy_bonus = if rating == Rating::Easy { w[16] } else { 1.0 };
    s * (1.0
        + w[8].exp()
            * (11.0 - d)
            * s.powf(-w[9])
            * ((w[10] * (1.0 - r)).exp() - 1.0)
            * hard_penalty
            * easy_bonus)
}

fn forget_stability(s: f64, d: f64, r: f64, w: &[f64; 19]) -> f64 {
    w[11] * d.powf(-w[12]) * ((s + 1.0).powf(w[13]) - 1.0) * (w[14] * (1.0 - r)).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;

    #[test]
    fn first_review_sets_initial_stability() {
        let s = Scheduler::default();
        let st = s.review(&CardState::default(), Rating::Good, 0);
        assert_eq!(st.reps, 1);
        assert!((st.stability - FSRS5_DEFAULT_WEIGHTS[2]).abs() < 1e-9);
        assert!(st.due > 0);
        // Good 的首次间隔应在 2–5 天
        let days = (st.due - 0) as f64 / DAY as f64;
        assert!(
            (2.0..=5.0).contains(&days),
            "首次 Good 间隔 {days} 天，不合理"
        );
    }

    #[test]
    fn easy_schedules_later_than_good() {
        let s = Scheduler::default();
        let good = s.review(&CardState::default(), Rating::Good, 0);
        let easy = s.review(&CardState::default(), Rating::Easy, 0);
        assert!(easy.due > good.due);
    }

    #[test]
    fn repeated_good_grows_stability() {
        let s = Scheduler::default();
        let mut st = s.review(&CardState::default(), Rating::Good, 0);
        let first = st.stability;
        for k in 1..5 {
            st = s.review(&st, Rating::Good, k * 4 * DAY);
        }
        assert!(st.stability > first * 2.0, "连续答对稳定性没有明显增长");
        assert_eq!(st.lapses, 0);
    }

    #[test]
    fn lapse_reduces_stability_and_counts() {
        let s = Scheduler::default();
        let mut st = s.review(&CardState::default(), Rating::Easy, 0);
        let before = st.stability;
        st = s.review(&st, Rating::Again, 10 * DAY);
        assert!(st.stability < before);
        assert_eq!(st.lapses, 1);
    }

    #[test]
    fn difficulty_moves_with_rating() {
        let s = Scheduler::default();
        let hard = s.review(&CardState::default(), Rating::Hard, 0);
        let easy = s.review(&CardState::default(), Rating::Easy, 0);
        assert!(hard.difficulty > easy.difficulty);
        assert!((1.0..=10.0).contains(&hard.difficulty));
        assert!((1.0..=10.0).contains(&easy.difficulty));
    }

    #[test]
    fn retrievability_decays() {
        assert!((retrievability(10.0, 0.0) - 1.0).abs() < 1e-9);
        assert!(retrievability(10.0, 10.0) < 0.95);
        assert!(retrievability(10.0, 100.0) < 0.7);
    }

    #[test]
    fn longer_interval_when_retention_target_lower() {
        let a = next_interval(10.0, 0.9);
        let b = next_interval(10.0, 0.8);
        assert!(b > a);
    }
}
