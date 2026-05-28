//! Buddy 伙伴系统 — 对应 TS 的 buddy/ 目录。
//!
//! 基于 userId hash 的确定性宠物生成系统（含稀有度/属性/物种）。

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

// ─── Types (buddy/types.ts) ────────────────────────────────────────────────

/// 稀有度等级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Epic,
    Legendary,
}

/// 所有稀有度（按概率从高到低）。
pub const RARITIES: &[Rarity] = &[
    Rarity::Common,
    Rarity::Uncommon,
    Rarity::Rare,
    Rarity::Epic,
    Rarity::Legendary,
];

/// 稀有度权重。
pub fn rarity_weight(rarity: Rarity) -> u32 {
    match rarity {
        Rarity::Common => 60,
        Rarity::Uncommon => 25,
        Rarity::Rare => 10,
        Rarity::Epic => 4,
        Rarity::Legendary => 1,
    }
}

/// 稀有度星级显示。
pub fn rarity_stars(rarity: Rarity) -> &'static str {
    match rarity {
        Rarity::Common => "★",
        Rarity::Uncommon => "★★",
        Rarity::Rare => "★★★",
        Rarity::Epic => "★★★★",
        Rarity::Legendary => "★★★★★",
    }
}

/// 物种类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Species {
    Duck,
    Goose,
    Blob,
    Cat,
    Dragon,
    Cephalopod,
    Owl,
    Penguin,
    Turtle,
    Snail,
    Ghost,
    Axolotl,
    Capybara,
    Cactus,
    Robot,
    Rabbit,
    Mushroom,
    Chonk,
}

/// 所有物种。
pub const SPECIES: &[Species] = &[
    Species::Duck,
    Species::Goose,
    Species::Blob,
    Species::Cat,
    Species::Dragon,
    Species::Cephalopod,
    Species::Owl,
    Species::Penguin,
    Species::Turtle,
    Species::Snail,
    Species::Ghost,
    Species::Axolotl,
    Species::Capybara,
    Species::Cactus,
    Species::Robot,
    Species::Rabbit,
    Species::Mushroom,
    Species::Chonk,
];

/// 眼睛类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Eye {
    #[serde(rename = "·")]
    Dot,
    #[serde(rename = "✦")]
    Star,
    #[serde(rename = "×")]
    Cross,
    #[serde(rename = "◉")]
    Circle,
    #[serde(rename = "@")]
    At,
    #[serde(rename = "°")]
    Degree,
}

/// 所有眼睛样式。
pub const EYES: &[Eye] = &[
    Eye::Dot,
    Eye::Star,
    Eye::Cross,
    Eye::Circle,
    Eye::At,
    Eye::Degree,
];

/// 帽子类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Hat {
    None,
    Crown,
    Tophat,
    Propeller,
    Halo,
    Wizard,
    Beanie,
    Tinyduck,
}

/// 所有帽子样式。
pub const HATS: &[Hat] = &[
    Hat::None,
    Hat::Crown,
    Hat::Tophat,
    Hat::Propeller,
    Hat::Halo,
    Hat::Wizard,
    Hat::Beanie,
    Hat::Tinyduck,
];

/// 属性名称。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StatName {
    #[serde(rename = "DEBUGGING")]
    Debugging,
    #[serde(rename = "PATIENCE")]
    Patience,
    #[serde(rename = "CHAOS")]
    Chaos,
    #[serde(rename = "WISDOM")]
    Wisdom,
    #[serde(rename = "SNARK")]
    Snark,
}

/// 所有属性名。
pub const STAT_NAMES: &[StatName] = &[
    StatName::Debugging,
    StatName::Patience,
    StatName::Chaos,
    StatName::Wisdom,
    StatName::Snark,
];

/// 伙伴骨架（确定性部分，从 hash(userId) 推导）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionBones {
    pub rarity: Rarity,
    pub species: Species,
    pub eye: Eye,
    pub hat: Hat,
    pub shiny: bool,
    pub stats: HashMap<StatName, u32>,
}

/// 伙伴灵魂（模型生成，存储在配置中）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionSoul {
    pub name: String,
    pub personality: String,
}

/// 完整伙伴（骨架 + 灵魂 + 孵化时间）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Companion {
    #[serde(flatten)]
    pub bones: CompanionBones,
    #[serde(flatten)]
    pub soul: CompanionSoul,
    pub hatched_at: i64,
}

/// 存储的伙伴（灵魂 + 孵化时间）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCompanion {
    #[serde(flatten)]
    pub soul: CompanionSoul,
    pub hatched_at: i64,
}

// ─── Companion Generation (buddy/companion.ts) ─────────────────────────────

/// Roll 结果。
#[derive(Debug, Clone)]
pub struct Roll {
    pub bones: CompanionBones,
    pub inspiration_seed: u32,
}

/// Mulberry32 PRNG — 小型种子化伪随机数生成器。
struct Mulberry32 {
    state: u32,
}

impl Mulberry32 {
    fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> f64 {
        self.state = self.state.wrapping_add(0x6d2b79f5);
        let mut t = self.state;
        t = (t ^ (t >> 15)).wrapping_mul(1 | t);
        t = (t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t))) ^ t;
        ((t ^ (t >> 14)) as f64) / 4294967296.0
    }
}

/// FNV-1a 哈希（与 TS 的 hashString 等效）。
fn hash_string(s: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for byte in s.bytes() {
        h ^= byte as u32;
        h = h.wrapping_mul(16777619);
    }
    h
}

/// 从数组中随机选择。
fn pick<T: Copy>(rng: &mut Mulberry32, arr: &[T]) -> T {
    let idx = (rng.next() * arr.len() as f64) as usize;
    arr[idx.min(arr.len() - 1)]
}

/// 骰子掷稀有度。
fn roll_rarity(rng: &mut Mulberry32) -> Rarity {
    let total: u32 = RARITIES.iter().map(|r| rarity_weight(*r)).sum();
    let mut roll_val = rng.next() * total as f64;
    for &rarity in RARITIES {
        roll_val -= rarity_weight(rarity) as f64;
        if roll_val < 0.0 {
            return rarity;
        }
    }
    Rarity::Common
}

/// 稀有度属性下限。
fn rarity_floor(rarity: Rarity) -> u32 {
    match rarity {
        Rarity::Common => 5,
        Rarity::Uncommon => 15,
        Rarity::Rare => 25,
        Rarity::Epic => 35,
        Rarity::Legendary => 50,
    }
}

/// 骰子掷属性值。
fn roll_stats(rng: &mut Mulberry32, rarity: Rarity) -> HashMap<StatName, u32> {
    let floor = rarity_floor(rarity);
    let peak = pick(rng, STAT_NAMES);
    let mut dump = pick(rng, STAT_NAMES);
    while dump == peak {
        dump = pick(rng, STAT_NAMES);
    }

    let mut stats = HashMap::new();
    for &name in STAT_NAMES {
        let value = if name == peak {
            (floor + 50 + (rng.next() * 30.0) as u32).min(100)
        } else if name == dump {
            (floor as i32 - 10 + (rng.next() * 15.0) as i32).max(1) as u32
        } else {
            floor + (rng.next() * 40.0) as u32
        };
        stats.insert(name, value);
    }
    stats
}

const SALT: &str = "friend-2026-401";

/// 从 RNG 执行一次完整的 roll。
fn roll_from(rng: &mut Mulberry32) -> Roll {
    let rarity = roll_rarity(rng);
    let bones = CompanionBones {
        rarity,
        species: pick(rng, SPECIES),
        eye: pick(rng, EYES),
        hat: if rarity == Rarity::Common {
            Hat::None
        } else {
            pick(rng, HATS)
        },
        shiny: rng.next() < 0.01,
        stats: roll_stats(rng, rarity),
    };
    let inspiration_seed = (rng.next() * 1e9) as u32;
    Roll {
        bones,
        inspiration_seed,
    }
}

/// Roll 缓存。
static ROLL_CACHE: Lazy<Mutex<Option<(String, Roll)>>> = Lazy::new(|| Mutex::new(None));

/// 对给定 userId 执行确定性 roll。
///
/// 对应 TS 的 roll(userId)。
/// 结果被缓存，因为多个热路径频繁调用。
pub fn roll(user_id: &str) -> Roll {
    let key = format!("{}{}", user_id, SALT);
    let mut cache = ROLL_CACHE.lock().unwrap();
    if let Some((ref cached_key, ref cached_roll)) = *cache {
        if cached_key == &key {
            return cached_roll.clone();
        }
    }
    let value = roll_from(&mut Mulberry32::new(hash_string(&key)));
    *cache = Some((key, value.clone()));
    value
}

/// 使用指定种子执行 roll。
pub fn roll_with_seed(seed: &str) -> Roll {
    roll_from(&mut Mulberry32::new(hash_string(seed)))
}

/// 获取伙伴的 userId。
pub fn companion_user_id() -> String {
    let config = mossen_utils::config::get_global_config();
    config
        .oauth_account
        .as_ref()
        .map(|a| a.account_uuid.clone())
        .filter(|s| !s.is_empty())
        .or(config.user_id.clone())
        .unwrap_or_else(|| "anon".to_string())
}

// ─── Species constants (TS buddy/types.ts) ─────────────────────────────────
// 这些常量在 TS 中通过 String.fromCharCode 构造以躲避 excluded-strings 检查；
// 在 Rust 中按 ASCII 字面量等价表达即可。

pub const duck: &str = "duck";
pub const goose: &str = "goose";
pub const blob: &str = "blob";
pub const cat: &str = "cat";
pub const dragon: &str = "dragon";
pub const cephalopod: &str = "cephalopod";
pub const owl: &str = "owl";
pub const penguin: &str = "penguin";
pub const turtle: &str = "turtle";
pub const snail: &str = "snail";
pub const ghost: &str = "ghost";
pub const axolotl: &str = "axolotl";
pub const capybara: &str = "capybara";
pub const cactus: &str = "cactus";
pub const robot: &str = "robot";
pub const rabbit: &str = "rabbit";
pub const mushroom: &str = "mushroom";
pub const chonk: &str = "chonk";

/// 物种名字符串数组（兼容 TS）。
pub const SPECIES_NAMES: &[&str] = &[
    duck, goose, blob, cat, dragon, cephalopod, owl, penguin, turtle, snail, ghost, axolotl,
    capybara, cactus, robot, rabbit, mushroom, chonk,
];

pub const EYE_CHARS: &[&str] = &["·", "✦", "×", "◉", "@", "°"];

pub const HAT_NAMES: &[&str] = &[
    "none",
    "crown",
    "tophat",
    "propeller",
    "halo",
    "wizard",
    "beanie",
    "tinyduck",
];

pub const STAT_LABELS: &[&str] = &["DEBUGGING", "PATIENCE", "CHAOS", "WISDOM", "SNARK"];

pub const RARITY_STARS: &[(&str, &str)] = &[
    ("common", "★"),
    ("uncommon", "★★"),
    ("rare", "★★★"),
    ("epic", "★★★★"),
    ("legendary", "★★★★★"),
];

pub const RARITY_COLORS: &[(&str, &str)] = &[
    ("common", "inactive"),
    ("uncommon", "success"),
    ("rare", "permission"),
    ("epic", "autoAccept"),
    ("legendary", "warning"),
];

// ────────────────────────────────────────────────────────────────────────────
// buddy/sprites.ts — sprite 渲染
// ────────────────────────────────────────────────────────────────────────────

/// 渲染一个 sprite 为 ASCII 字符串。
pub fn render_sprite(species: &str, frame_index: usize) -> String {
    // 简化实现：返回一个 ASCII art。真实实现是按物种返回多帧动画。
    let frames = match species {
        "duck" => &[r#"  __
( o>"#],
        "cat" => &[r"=^.^="],
        "robot" => &[r"[o_o]"],
        _ => &[":3"],
    };
    let idx = frame_index % frames.len().max(1);
    frames[idx].to_string()
}

pub fn renderSprite(species: &str, frame_index: usize) -> String {
    render_sprite(species, frame_index)
}

/// 物种 sprite 总帧数。
pub fn sprite_frame_count(species: &str) -> usize {
    match species {
        "duck" | "goose" | "blob" | "cat" | "dragon" => 4,
        _ => 2,
    }
}

pub fn spriteFrameCount(species: &str) -> usize {
    sprite_frame_count(species)
}

/// 渲染一个表情。
pub fn render_face(eye: &str) -> String {
    format!("({0} {0})", eye)
}

pub fn renderFace(eye: &str) -> String {
    render_face(eye)
}

// ────────────────────────────────────────────────────────────────────────────
// buddy/CompanionSprite.tsx — UI 组件等价物
// ────────────────────────────────────────────────────────────────────────────

pub const MIN_COLS_FOR_FULL_SPRITE: usize = 30;

/// CompanionSprite 占用的列数。
pub fn companion_reserved_columns(species: &str) -> usize {
    match species {
        "duck" | "goose" => 6,
        "cephalopod" | "capybara" => 9,
        "dragon" | "mushroom" => 10,
        _ => 5,
    }
}

pub fn companionReservedColumns(species: &str) -> usize {
    companion_reserved_columns(species)
}

/// Companion sprite 渲染结果（终端字符串）。
pub struct CompanionSprite;

impl CompanionSprite {
    pub fn render(species: &str, frame: usize) -> String {
        render_sprite(species, frame)
    }
}

/// 浮动气泡。
pub struct CompanionFloatingBubble;

impl CompanionFloatingBubble {
    pub fn render(message: &str, max_cols: usize) -> String {
        if message.len() > max_cols {
            format!("[{}...]", &message[..max_cols.saturating_sub(5).max(1)])
        } else {
            format!("[{}]", message)
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// buddy/useBuddyNotification.tsx — 通知钩子
// ────────────────────────────────────────────────────────────────────────────

/// 当前是否处于 buddy 介绍/teaser 窗口（启动后某个时间段）。
pub fn is_buddy_teaser_window() -> bool {
    // 简化：默认不开启
    false
}

pub fn isBuddyTeaserWindow() -> bool {
    is_buddy_teaser_window()
}

/// buddy 是否处于活动状态。
pub fn is_buddy_live() -> bool {
    get_companion().is_some()
}

pub fn isBuddyLive() -> bool {
    is_buddy_live()
}

/// 通知信息。
#[derive(Debug, Clone)]
pub struct BuddyNotification {
    pub message: String,
    pub kind: String,
}

/// `useBuddyNotification` Rust 等价：返回当前 buddy 通知（无则 None）。
pub fn use_buddy_notification() -> Option<BuddyNotification> {
    if !is_buddy_live() {
        return None;
    }
    None
}

pub fn useBuddyNotification() -> Option<BuddyNotification> {
    use_buddy_notification()
}

/// 在消息文本中找出 buddy 触发位置（@buddy 等）。
pub fn find_buddy_trigger_positions(text: &str) -> Vec<usize> {
    text.match_indices("@buddy").map(|(i, _)| i).collect()
}

pub fn findBuddyTriggerPositions(text: &str) -> Vec<usize> {
    find_buddy_trigger_positions(text)
}

/// 获取当前用户的伙伴。
///
/// 从 hash(userId) 重新生成骨架，与存储的灵魂合并。
/// 骨架不持久化，因此物种重命名和 SPECIES 数组编辑不会破坏已存储的伙伴。
pub fn get_companion() -> Option<Companion> {
    let config = mossen_utils::config::get_global_config();
    let stored = config.companion.as_ref()?;
    let Roll { bones, .. } = roll(&companion_user_id());
    // 从 flattened extra map 提取 soul 和 hatched_at
    let soul = stored
        .extra
        .get("soul")
        .and_then(|v| serde_json::from_value::<CompanionSoul>(v.clone()).ok())
        .unwrap_or_else(|| CompanionSoul {
            name: String::new(),
            personality: String::new(),
        });
    let hatched_at = stored
        .extra
        .get("hatchedAt")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    Some(Companion {
        bones,
        soul,
        hatched_at,
    })
}
