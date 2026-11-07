//! Style-matched feminine names for the characters a casting invents.
//!
//! The draw engine names an OC by picking from one shared list, whatever she looks like. Studio
//! replaces that name when the batch is launched: the style comes from the family and the P1 tags,
//! the surname sometimes echoes the hair colour, and a name already taken is never reused. Existing
//! characters (bank, validated, targeted, rotations, duos) keep their name. The engine's name stays
//! in `engine_name`; the stem is untouched because the engine's own tools look shots up by stem.

use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

#[derive(Deserialize)]
struct Style {
    teintes: bool,
    prenoms: Vec<String>,
    noms: Vec<String>,
}

#[derive(Deserialize)]
struct Rule {
    style: String,
    tags: Vec<String>,
}

#[derive(Deserialize)]
struct Pools {
    familles: HashMap<String, String>,
    motifs_forts: Vec<Rule>,
    motifs: Vec<Rule>,
    styles: HashMap<String, Style>,
    teintes: HashMap<String, Vec<String>>,
}

const FALLBACK_STYLE: &str = "humain";
/// Random draws before accepting a first name already used in the batch, then before walking
/// every combination of the style.
const VARIED_DRAWS: u32 = 128;
const DRAWS: u32 = 256;

fn pools() -> &'static Pools {
    static POOLS: OnceLock<Pools> = OnceLock::new();
    POOLS.get_or_init(|| {
        serde_json::from_str(include_str!("casting_names.json")).expect("casting_names.json is valid")
    })
}

/// Lowercase words separated by single spaces, so `(fox ears:1.2)` matches the phrase `fox ears`.
fn tag_words(p1: &str) -> String {
    let text: String = p1
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    format!(" {} ", text.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn matching_rule<'a>(rules: &'a [Rule], words: &str) -> Option<&'a str> {
    rules
        .iter()
        .find(|rule| rule.tags.iter().any(|tag| words.contains(&format!(" {tag} "))))
        .map(|rule| rule.style.as_str())
}

/// Style of a character: a precise look wins (egyptian goddess, miko, moth girl), then the family,
/// then the P1 tags for the families that say nothing of the look (human, unknown).
pub fn style_for(famille: &str, p1: &str) -> &'static str {
    let pools = pools();
    let words = tag_words(p1);
    if let Some(style) = matching_rule(&pools.motifs_forts, &words) {
        return style;
    }
    match pools.familles.get(famille).map(String::as_str) {
        Some(style) if style != FALLBACK_STYLE => style,
        _ => matching_rule(&pools.motifs, &words).unwrap_or(FALLBACK_STYLE),
    }
}

/// Three independent numbers, stable for a given draw seed, stem and attempt.
fn draw(graine: u64, stem: &str, attempt: u32) -> [u64; 3] {
    let hash = Sha256::digest(format!("{graine}:{stem}:{attempt}"));
    let word = |at: usize| {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&hash[at..at + 8]);
        u64::from_le_bytes(bytes)
    };
    [word(0), word(8), word(16)]
}

fn full_name(first: &str, last: &str) -> Option<String> {
    (!first.eq_ignore_ascii_case(last)).then(|| format!("{first} {last}"))
}

/// A free name in `style`, or None when every combination of the style is taken.
fn pick_name(
    style: &Style,
    tint: &[String],
    graine: u64,
    stem: &str,
    taken: &HashSet<String>,
    used_first: &HashSet<String>,
) -> Option<(String, String)> {
    let free = |name: &String| !taken.contains(&name.to_lowercase());
    for attempt in 0..DRAWS {
        let [a, b, c] = draw(graine, stem, attempt);
        let first = &style.prenoms[a as usize % style.prenoms.len()];
        if attempt < VARIED_DRAWS && used_first.contains(first) {
            continue;
        }
        let last = if !tint.is_empty() && c % 3 == 0 {
            &tint[b as usize % tint.len()]
        } else {
            &style.noms[b as usize % style.noms.len()]
        };
        if let Some(name) = full_name(first, last).filter(|name| free(name)) {
            return Some((name, first.clone()));
        }
    }
    style.prenoms.iter().find_map(|first| {
        style
            .noms
            .iter()
            .find_map(|last| full_name(first, last).filter(|name| free(name)))
            .map(|name| (name, first.clone()))
    })
}

/// Renames the invented characters of a batch. `taken` holds the names already in use.
pub fn assign_names(shots: &mut [Value], graine: u64, taken: &HashSet<String>) {
    let pools = pools();
    let mut taken: HashSet<String> = taken.iter().map(|name| name.to_lowercase()).collect();
    let mut used_first = HashSet::new();
    for shot in shots.iter_mut() {
        if shot["note"].as_str() != Some("oc") || shot["negatif_fige"] == json!(true) {
            continue;
        }
        let famille = shot["famille"].as_str().unwrap_or_default().to_owned();
        let p1 = shot["casting"]["p1"]
            .as_str()
            .or(shot["p1"].as_str())
            .unwrap_or_default()
            .to_owned();
        let stem = shot["stem"].as_str().unwrap_or_default().to_owned();
        let style_name = style_for(&famille, &p1);
        let Some(style) = pools.styles.get(style_name) else {
            continue;
        };
        let tint: &[String] = if style.teintes {
            pools
                .teintes
                .get(crate::casting::hair_colour(&p1))
                .map(Vec::as_slice)
                .unwrap_or_default()
        } else {
            &[]
        };
        // Every combination taken: the engine's own (unique) name stays.
        let Some((name, first)) = pick_name(style, tint, graine, &stem, &taken, &used_first) else {
            continue;
        };
        taken.insert(name.to_lowercase());
        used_first.insert(first);
        shot["engine_name"] = shot["name"].clone();
        shot["name"] = json!(name);
        shot["name_style"] = json!(style_name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oc(stem: &str, famille: &str, p1: &str) -> Value {
        json!({"stem":stem,"name":"Engine Name","famille":famille,"note":"oc","casting":{"p1":p1}})
    }

    #[test]
    fn pools_are_large_clean_and_without_duplicates() {
        // Nothing childish: every name must read as an adult woman.
        const REFUSED: [&str; 14] = [
            "child", "baby", "babe", "kid", "loli", "little", "petit", "junior", "teen", "girl",
            "doll", "bunny", "kitt", "young",
        ];
        let pools = pools();
        let mut combinations = 0usize;
        let check = |list: &[String]| {
            let unique: HashSet<String> = list.iter().map(|n| n.to_lowercase()).collect();
            assert_eq!(unique.len(), list.len(), "duplicate in {list:?}");
            for name in list {
                let lower = name.to_lowercase();
                assert!(!REFUSED.iter().any(|word| lower.contains(word)), "refused name: {name}");
            }
        };
        for (key, style) in &pools.styles {
            check(&style.prenoms);
            check(&style.noms);
            assert!(style.prenoms.len() >= 29, "{key}: {} first names", style.prenoms.len());
            assert!(style.noms.len() >= 19, "{key}: {} surnames", style.noms.len());
            combinations += style.prenoms.len() * style.noms.len();
        }
        for list in pools.teintes.values() {
            check(list);
        }
        assert!(combinations > 20_000, "{combinations} combinations");
        for style in pools.familles.values() {
            assert!(pools.styles.contains_key(style), "{style}");
        }
        for rule in pools.motifs_forts.iter().chain(&pools.motifs) {
            assert!(pools.styles.contains_key(&rule.style), "{}", rule.style);
        }
    }

    #[test]
    fn style_follows_the_family_then_the_look() {
        assert_eq!(style_for("elf", "1girl, solo, elf, (pointy ears:1.2)"), "elf");
        assert_eq!(style_for("dem", "1girl, succubus"), "dem");
        assert_eq!(style_for("div", "egyptian goddess, (gold usekh collar:1.1)"), "egypte");
        assert_eq!(style_for("div", "miko, (paper charm hair ornament:1.1)"), "japon");
        assert_eq!(style_for("kmn", "moth girl, (fluffy moth antennae:1.2)"), "insecte");
        assert_eq!(style_for("hum", "1girl, gothic, (black lipstick:1.1)"), "edgy");
        assert_eq!(style_for("hum", "1girl, long hair"), "humain");
        assert_eq!(style_for("inconnue", "1girl"), "humain");
        // A word inside another word is not a match.
        assert_eq!(style_for("hum", "1girl, shelf, onion, demonstration"), "humain");
    }

    #[test]
    fn invented_characters_get_a_styled_stable_unique_name() {
        let batch = || -> Vec<Value> {
            (0..30).map(|i| oc(&format!("elf-{i:05}"), "elf", "1girl, elf, long hair")).collect()
        };
        let (mut first, mut second) = (batch(), batch());
        assign_names(&mut first, 42, &HashSet::new());
        assign_names(&mut second, 42, &HashSet::new());
        assert_eq!(first, second, "same seed, same names");
        let style = &pools().styles["elf"];
        let names: HashSet<&str> = first.iter().map(|s| s["name"].as_str().unwrap()).collect();
        assert_eq!(names.len(), first.len(), "unique in the batch");
        let firsts: HashSet<&str> = names.iter().map(|n| n.split(' ').next().unwrap()).collect();
        assert_eq!(firsts.len(), first.len(), "first names vary while the pool allows it");
        for shot in &first {
            let name = shot["name"].as_str().unwrap();
            assert!(style.prenoms.iter().any(|p| name.starts_with(p.as_str())), "{name}");
            assert_eq!(shot["engine_name"], "Engine Name");
            assert_eq!(shot["name_style"], "elf");
        }
    }

    #[test]
    fn a_taken_name_is_never_reused_even_when_the_pool_runs_out() {
        let style = &pools().styles["egypte"];
        let mut taken = HashSet::new();
        for first in &style.prenoms {
            for last in &style.noms {
                taken.insert(format!("{first} {last}"));
            }
        }
        let open = format!("{} {}", style.prenoms[3], style.noms[2]);
        taken.remove(&open);
        let mut shots = vec![oc("div-1", "div", "egyptian goddess"), oc("div-2", "div", "egyptian goddess")];
        assign_names(&mut shots, 7, &taken);
        assert_eq!(shots[0]["name"], json!(open), "the only free combination");
        assert_eq!(shots[1]["name"], "Engine Name", "nothing free: the engine's name stays");
    }

    #[test]
    fn existing_characters_keep_their_name() {
        let mut shots = vec![
            json!({"stem":"a","name":"Banque","famille":"hum","note":"repull","casting":{"p1":""}}),
            json!({"stem":"b","name":"Validee","famille":"hum","note":"oc","negatif_fige":true,"casting":{"p1":""}}),
            json!({"stem":"c","name":"A x B","famille":"girl_x_girl","note":"duo/girl_x_girl","casting":{"p1":""}}),
        ];
        let before = shots.clone();
        assign_names(&mut shots, 1, &HashSet::new());
        assert_eq!(shots, before);
    }

    #[test]
    fn the_surname_can_echo_the_hair_colour() {
        let tints = &pools().teintes["white"];
        let mut shots: Vec<Value> = (0..60)
            .map(|i| oc(&format!("elf-{i}"), "elf", "1girl, elf, (white hair:1.2)"))
            .collect();
        assign_names(&mut shots, 3, &HashSet::new());
        let echoed = shots
            .iter()
            .filter(|s| tints.iter().any(|t| s["name"].as_str().unwrap().ends_with(t.as_str())))
            .count();
        assert!((5..=40).contains(&echoed), "{echoed} of 60");
    }
}
