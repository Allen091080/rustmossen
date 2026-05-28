//! # Spinner Verbs (spinnerVerbs.ts)
//!
//! Spinner 动词列表及选择函数。

use regex::Regex;

use once_cell::sync::Lazy;

/// Check if a spinner verb is compatible with the given language tag.
pub fn is_spinner_verb_compatible_with_language(verb: Option<&str>, language_tag: &str) -> bool {
    match verb {
        None => false,
        Some(v) => {
            static HAN_RE: Lazy<Regex> =
                Lazy::new(|| Regex::new(r"[\p{Han}\x{3000}-\x{303f}\x{ff00}-\x{ffef}]").unwrap());
            let has_han = HAN_RE.is_match(v);
            if language_tag == "zh" {
                has_han
            } else {
                !has_han
            }
        }
    }
}

/// Get spinner verbs for the given language tag.
/// `custom_verbs` and `custom_mode` correspond to settings.spinnerVerbs config.
pub fn get_spinner_verbs<'a>(
    language_tag: &'a str,
    custom_verbs: Option<&'a [String]>,
    custom_mode: Option<&'a str>,
) -> Vec<&'a str> {
    let defaults: &[&str] = if language_tag == "zh" {
        SPINNER_VERBS_ZH
    } else {
        SPINNER_VERBS
    };

    match (custom_verbs, custom_mode) {
        (None, _) => defaults.to_vec(),
        (Some(verbs), Some("replace")) => {
            if verbs.is_empty() {
                defaults.to_vec()
            } else {
                verbs.iter().map(|s| s.as_str()).collect()
            }
        }
        (Some(verbs), _) => {
            let mut result: Vec<&str> = defaults.to_vec();
            for v in verbs {
                result.push(v.as_str());
            }
            result
        }
    }
}

/// Get a spinner verb compatible with the current language.
/// Falls back to a random verb from defaults if incompatible.
pub fn get_spinner_verb_for_language<'a>(
    verb: Option<&'a str>,
    language_tag: &str,
    fallback_verb: Option<&'a str>,
) -> &'a str {
    if is_spinner_verb_compatible_with_language(verb, language_tag) {
        return verb.unwrap();
    }
    if let Some(fb) = fallback_verb {
        return fb;
    }
    if language_tag == "zh" {
        "处理中"
    } else {
        "Working"
    }
}

// Spinner verbs for loading messages
pub const SPINNER_VERBS: &[&str] = &[
    "Accomplishing",
    "Actioning",
    "Actualizing",
    "Architecting",
    "Baking",
    "Beaming",
    "Beboppin'",
    "Befuddling",
    "Billowing",
    "Blanching",
    "Bloviating",
    "Boogieing",
    "Boondoggling",
    "Booping",
    "Bootstrapping",
    "Brewing",
    "Bunning",
    "Burrowing",
    "Calculating",
    "Canoodling",
    "Caramelizing",
    "Cascading",
    "Catapulting",
    "Cerebrating",
    "Channeling",
    "Channelling",
    "Choreographing",
    "Churning",
    "Coalescing",
    "Cogitating",
    "Combobulating",
    "Composing",
    "Computing",
    "Concocting",
    "Considering",
    "Contemplating",
    "Cooking",
    "Crafting",
    "Creating",
    "Crunching",
    "Crystallizing",
    "Cultivating",
    "Deciphering",
    "Deliberating",
    "Determining",
    "Dilly-dallying",
    "Discombobulating",
    "Doing",
    "Doodling",
    "Drizzling",
    "Ebbing",
    "Effecting",
    "Elucidating",
    "Embellishing",
    "Enchanting",
    "Envisioning",
    "Evaporating",
    "Fermenting",
    "Fiddle-faddling",
    "Finagling",
    "Flambéing",
    "Flibbertigibbeting",
    "Flowing",
    "Flummoxing",
    "Fluttering",
    "Forging",
    "Forming",
    "Frolicking",
    "Frosting",
    "Gallivanting",
    "Galloping",
    "Garnishing",
    "Generating",
    "Gesticulating",
    "Germinating",
    "Gitifying",
    "Grooving",
    "Gusting",
    "Harmonizing",
    "Hashing",
    "Hatching",
    "Herding",
    "Honking",
    "Hullaballooing",
    "Hyperspacing",
    "Ideating",
    "Imagining",
    "Improvising",
    "Incubating",
    "Inferring",
    "Infusing",
    "Ionizing",
    "Jitterbugging",
    "Julienning",
    "Kneading",
    "Leavening",
    "Levitating",
    "Manifesting",
    "Marinating",
    "Meandering",
    "Metamorphosing",
    "Misting",
    "Moonwalking",
    "Moseying",
    "Mulling",
    "Mustering",
    "Musing",
    "Nebulizing",
    "Nesting",
    "Newspapering",
    "Noodling",
    "Nucleating",
    "Orbiting",
    "Orchestrating",
    "Osmosing",
    "Perambulating",
    "Percolating",
    "Perusing",
    "Philosophising",
    "Photosynthesizing",
    "Pollinating",
    "Pondering",
    "Pontificating",
    "Pouncing",
    "Precipitating",
    "Prestidigitating",
    "Processing",
    "Proofing",
    "Propagating",
    "Puttering",
    "Puzzling",
    "Quantumizing",
    "Razzle-dazzling",
    "Razzmatazzing",
    "Recombobulating",
    "Reticulating",
    "Roosting",
    "Ruminating",
    "Sautéing",
    "Scampering",
    "Schlepping",
    "Scurrying",
    "Seasoning",
    "Shenaniganing",
    "Shimmying",
    "Simmering",
    "Skedaddling",
    "Sketching",
    "Slithering",
    "Smooshing",
    "Sock-hopping",
    "Spelunking",
    "Spinning",
    "Sprouting",
    "Stewing",
    "Sublimating",
    "Swirling",
    "Swooping",
    "Symbioting",
    "Synthesizing",
    "Tempering",
    "Thinking",
    "Thundering",
    "Tinkering",
    "Tomfoolering",
    "Topsy-turvying",
    "Transfiguring",
    "Transmuting",
    "Twisting",
    "Undulating",
    "Unfurling",
    "Unravelling",
    "Vibing",
    "Waddling",
    "Wandering",
    "Warping",
    "Whatchamacalliting",
    "Whirlpooling",
    "Whirring",
    "Whisking",
    "Wibbling",
    "Working",
    "Wrangling",
    "Zesting",
    "Zigzagging",
];

pub const SPINNER_VERBS_ZH: &[&str] = &[
    "处理中",
    "构思中",
    "分析中",
    "推演中",
    "梳理中",
    "规划中",
    "实现中",
    "编写中",
    "修改中",
    "重构中",
    "检查中",
    "排查中",
    "验证中",
    "调试中",
    "测试中",
    "整理中",
    "汇总中",
    "打磨中",
    "润色中",
    "联调中",
    "整合中",
    "对齐中",
    "回溯中",
    "定位中",
    "比对中",
    "推敲中",
    "准备中",
    "加载中",
    "思考中",
    "工作中",
    "编排中",
    "搭建中",
    "设计中",
    "布局中",
    "拆解中",
    "检索中",
    "查找中",
    "扫描中",
    "读取中",
    "解析中",
    "识别中",
    "计算中",
    "推导中",
    "求解中",
    "估算中",
    "研判中",
    "生成中",
    "起草中",
    "撰写中",
    "绘制中",
    "勾勒中",
    "转换中",
    "迁移中",
    "映射中",
    "转写中",
    "优化中",
    "精炼中",
    "压缩中",
    "提炼中",
    "收敛中",
    "编译中",
    "构建中",
    "打包中",
    "装配中",
    "清理中",
    "收尾中",
    "归档中",
    "提交中",
    "保存中",
    "学习中",
    "酝酿中",
    "揣摩中",
    "调度中",
    "协调中",
    "串联中",
    "分派中",
    "配齐中",
];
