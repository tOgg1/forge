//! Random agent name generation ported from Go `internal/names/cartoon_names.go`.

use rand::Rng;

static BASE_ADJECTIVES: &[&str] = &[
    "agile",
    "amped",
    "atomic",
    "beefy",
    "bold",
    "brisk",
    "bright",
    "brutal",
    "calm",
    "clean",
    "clever",
    "cool",
    "crisp",
    "daring",
    "dashing",
    "deft",
    "dynamic",
    "eager",
    "electric",
    "epic",
    "exact",
    "fearless",
    "fierce",
    "fiery",
    "flashy",
    "focused",
    "frank",
    "friendly",
    "frisky",
    "fun",
    "furious",
    "gallant",
    "game",
    "gentle",
    "gifted",
    "glorious",
    "golden",
    "gritty",
    "grounded",
    "happy",
    "hardy",
    "hearty",
    "heroic",
    "honest",
    "humble",
    "hungry",
    "icy",
    "jaunty",
    "keen",
    "kind",
    "laser",
    "lively",
    "lucid",
    "lucky",
    "lunar",
    "mellow",
    "merry",
    "mighty",
    "modest",
    "nimble",
    "noble",
    "open",
    "patient",
    "peppy",
    "perky",
    "playful",
    "polished",
    "punchy",
    "quick",
    "quiet",
    "radiant",
    "rapid",
    "ready",
    "regal",
    "relentless",
    "resolute",
    "restless",
    "robust",
    "rosy",
    "rugged",
    "savage",
    "savvy",
    "serene",
    "sharp",
    "shrewd",
    "slick",
    "solid",
    "spry",
    "steady",
    "stellar",
    "stoic",
    "sturdy",
    "sunny",
    "swift",
    "tactful",
    "tidy",
    "tough",
    "tranquil",
    "trusty",
    "upbeat",
    "valiant",
    "vivid",
    "warm",
    "wild",
    "witty",
    "zany",
    "zesty",
    "zippy",
    "zealous",
    "brave",
    "breezy",
    "buoyant",
    "canny",
    "chipper",
    "classic",
    "cosmic",
    "crafty",
    "fleet",
    "fresh",
    "jolly",
];

static GIVEN_NAMES: &[&str] = &[
    "homer",
    "marge",
    "bart",
    "lisa",
    "maggie",
    "abe",
    "ned",
    "maude",
    "rod",
    "todd",
    "milhouse",
    "martin",
    "nelson",
    "ralph",
    "clancy",
    "seymour",
    "edna",
    "waylon",
    "monty",
    "apu",
    "lenny",
    "carl",
    "moe",
    "barney",
    "willie",
    "troy",
    "kent",
    "bob",
    "krusty",
    "otto",
    "patty",
    "selma",
    "agnes",
    "frink",
    "jacqueline",
    "luann",
    "jimbo",
    "dolph",
    "kearney",
    "gil",
    "stan",
    "kyle",
    "eric",
    "kenny",
    "butters",
    "wendy",
    "randy",
    "sharon",
    "shelley",
    "ike",
    "tweek",
    "craig",
    "clyde",
    "jimmy",
    "timmy",
    "pip",
    "henrietta",
    "bebe",
    "red",
    "token",
    "garrison",
    "mackey",
    "chef",
    "gerald",
    "sheila",
    "liane",
    "peter",
    "lois",
    "meg",
    "chris",
    "stewie",
    "brian",
    "glenn",
    "cleveland",
    "joe",
    "bonnie",
    "mort",
    "herbert",
    "consuela",
    "adam",
    "carter",
    "tom",
    "jerome",
    "ida",
    "rupert",
    "jillian",
    "tricia",
    "seamus",
    "patrick",
    "bertram",
];

static FAMILY_NAMES: &[&str] = &[
    "simpson",
    "flanders",
    "burns",
    "smithers",
    "wiggum",
    "vanhouten",
    "skinner",
    "krabappel",
    "hibbert",
    "lovejoy",
    "brockman",
    "szyslak",
    "gumble",
    "frink",
    "quimby",
    "prince",
    "tatum",
    "terwilliger",
    "spuckler",
    "bouvier",
    "cho",
    "muntz",
    "nahasapeemapetilon",
    "wolfcastle",
    "krustofsky",
    "mcclure",
    "chalmers",
    "jones",
    "kearney",
    "landers",
    "marsh",
    "broflovski",
    "mccormick",
    "cartman",
    "stotch",
    "garrison",
    "mackey",
    "valmer",
    "tucker",
    "black",
    "testaburger",
    "tweek",
    "donovan",
    "anderson",
    "mcrae",
    "griffin",
    "quagmire",
    "brown",
    "swanson",
    "pewterschmidt",
    "goldman",
    "west",
    "simmons",
    "takanawa",
    "quahog",
    "pawtucket",
    "spooner",
    "longbottom",
    "mccoy",
];

static EXTRA_SINGLE_NAMES: &[&str] = &[
    "itchy",
    "scratchy",
    "snowball",
    "santaslittlehelper",
    "comicbook",
    "towelie",
    "manbearpig",
    "giantchicken",
    "greasedupdeafguy",
];

/// Build the deduplicated single names list (given + family + extra), preserving insertion order.
fn build_single_names() -> Vec<&'static str> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for &name in GIVEN_NAMES
        .iter()
        .chain(FAMILY_NAMES.iter())
        .chain(EXTRA_SINGLE_NAMES.iter())
    {
        if !name.is_empty() && seen.insert(name) {
            result.push(name);
        }
    }
    result
}

/// Generate a random two-part name: `adjective-singlename`.
pub fn random_loop_name_two_part<R: Rng>(rng: &mut R) -> String {
    let singles = build_single_names();
    let adj = BASE_ADJECTIVES[rng.gen_range(0..BASE_ADJECTIVES.len())];
    let name = singles[rng.gen_range(0..singles.len())];
    format!("{adj}-{name}")
}

/// Generate a random three-part name: `adjective-given-family`.
pub fn random_loop_name_three_part<R: Rng>(rng: &mut R) -> String {
    let adj = BASE_ADJECTIVES[rng.gen_range(0..BASE_ADJECTIVES.len())];
    let given = GIVEN_NAMES[rng.gen_range(0..GIVEN_NAMES.len())];
    let family = FAMILY_NAMES[rng.gen_range(0..FAMILY_NAMES.len())];
    format!("{adj}-{given}-{family}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn two_part_name_format() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let name = random_loop_name_two_part(&mut rng);
        let parts: Vec<&str> = name.split('-').collect();
        assert!(parts.len() >= 2, "expected at least 2 parts: {name}");
        assert!(
            BASE_ADJECTIVES.contains(&parts[0]),
            "first part should be adjective: {name}"
        );
    }

    #[test]
    fn three_part_name_format() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let name = random_loop_name_three_part(&mut rng);
        let parts: Vec<&str> = name.split('-').collect();
        assert_eq!(parts.len(), 3, "expected 3 parts: {name}");
        assert!(
            BASE_ADJECTIVES.contains(&parts[0]),
            "first part should be adjective: {name}"
        );
        assert!(
            GIVEN_NAMES.contains(&parts[1]),
            "second part should be given name: {name}"
        );
        assert!(
            FAMILY_NAMES.contains(&parts[2]),
            "third part should be family name: {name}"
        );
    }

    #[test]
    fn single_names_are_deduplicated() {
        let singles = build_single_names();
        let mut seen = std::collections::HashSet::new();
        for name in &singles {
            assert!(seen.insert(name), "duplicate single name: {name}");
        }
    }

    #[test]
    fn names_are_lowercase_kebab() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(99);
        for _ in 0..50 {
            let name = random_loop_name_two_part(&mut rng);
            assert!(
                name.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
                "invalid chars in: {name}"
            );
        }
    }
}
