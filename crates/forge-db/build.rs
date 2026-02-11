use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Default)]
struct MigrationFiles {
    slug: String,
    up: Option<String>,
    down: Option<String>,
}

fn main() {
    let manifest_dir = PathBuf::from(env_var("CARGO_MANIFEST_DIR"));
    let migrations_dir = manifest_dir.join("../../old/go/internal/db/migrations");

    println!("cargo:rerun-if-changed={}", migrations_dir.display());

    let mut by_version: BTreeMap<i32, MigrationFiles> = BTreeMap::new();

    let entries = match fs::read_dir(&migrations_dir) {
        Ok(entries) => entries,
        Err(err) => {
            panic!(
                "forge-db build: read migrations dir {}: {err}",
                migrations_dir.display()
            );
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => panic!("forge-db build: read_dir entry: {err}"),
        };
        let path = entry.path();
        if path.is_dir() {
            continue;
        }
        let file_name = match path.file_name().and_then(|v| v.to_str()) {
            Some(name) => name.to_string(),
            None => continue,
        };
        println!("cargo:rerun-if-changed={}", path.display());

        let parsed = match parse_migration_filename(&file_name) {
            Some(parsed) => parsed,
            None => continue,
        };

        let slot = by_version.entry(parsed.version).or_default();
        slot.slug = parsed.slug;
        match parsed.direction {
            Direction::Up => slot.up = Some(file_name),
            Direction::Down => slot.down = Some(file_name),
        }
    }

    let out_dir = PathBuf::from(env_var("OUT_DIR"));
    let out_path = out_dir.join("migrations.rs");
    let mut out = match fs::File::create(&out_path) {
        Ok(file) => file,
        Err(err) => panic!("forge-db build: create {}: {err}", out_path.display()),
    };

    if let Err(err) = writeln!(
        out,
        "/// Generated; do not edit. Source: internal/db/migrations\n\
         #[derive(Clone, Copy, Debug)]\n\
         pub struct EmbeddedMigration {{\n\
           pub version: i32,\n\
           pub description: &'static str,\n\
           pub up_sql: &'static str,\n\
           pub down_sql: &'static str,\n\
         }}\n\
         \n\
         pub static MIGRATIONS: &[EmbeddedMigration] = &["
    ) {
        panic!("forge-db build: write header: {err}");
    }

    for (version, files) in by_version {
        let description = files.slug.replace('_', " ");
        let up_sql = include_expr(&files.up);
        let down_sql = include_expr(&files.down);
        if let Err(err) = writeln!(
            out,
            "  EmbeddedMigration {{ version: {version}, description: {desc:?}, up_sql: {up}, down_sql: {down} }},",
            desc = description,
            up = up_sql,
            down = down_sql
        ) {
            panic!("forge-db build: write migration {version}: {err}");
        }
    }

    if let Err(err) = writeln!(out, "];") {
        panic!("forge-db build: write footer: {err}");
    }
}

fn env_var(key: &str) -> String {
    match env::var(key) {
        Ok(value) => value,
        Err(err) => panic!("forge-db build: missing env {key}: {err}"),
    }
}

#[derive(Debug, Clone, Copy)]
enum Direction {
    Up,
    Down,
}

#[derive(Debug, Clone)]
struct ParsedName {
    version: i32,
    slug: String,
    direction: Direction,
}

fn parse_migration_filename(name: &str) -> Option<ParsedName> {
    let (version_part, rest) = name.split_once('_')?;
    let version: i32 = version_part.parse().ok()?;

    if let Some(slug) = rest.strip_suffix(".up.sql") {
        return Some(ParsedName {
            version,
            slug: slug.to_string(),
            direction: Direction::Up,
        });
    }
    if let Some(slug) = rest.strip_suffix(".down.sql") {
        return Some(ParsedName {
            version,
            slug: slug.to_string(),
            direction: Direction::Down,
        });
    }
    None
}

fn include_expr(file_name: &Option<String>) -> String {
    match file_name {
        Some(file) => {
            let rel = format!("/../../old/go/internal/db/migrations/{file}");
            format!("include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), {rel:?}))",)
        }
        None => "\"\"".to_string(),
    }
}
