//! `zforge knowledge get <ID…> | --module <section|path> | --grep <text>
//! [--extra] [--json]`: the full text of accepted knowledge items (and,
//! with `--extra`, matching sections of the reference docs) without
//! reading whole knowledge files. Read-only.

use crate::config;
use crate::knowledge::{lookup, Knowledge};
use anyhow::{anyhow, Result};

pub fn get(
    ids: Vec<String>,
    module: Option<String>,
    grep: Option<String>,
    extra: bool,
    json: bool,
) -> Result<()> {
    let config = config::load().map_err(|_| anyhow!("Config not found. Run: zforge init"))?;
    let k = Knowledge::open(&config);
    let q = lookup::Query {
        ids,
        module,
        grep,
        extra,
    };
    let answer = lookup::get(&k, &q, &config.knowledge_extra())?;
    if json {
        println!("{}", serde_json::to_string_pretty(&answer)?);
    } else {
        print!("{}", lookup::render(&answer));
    }
    Ok(())
}
