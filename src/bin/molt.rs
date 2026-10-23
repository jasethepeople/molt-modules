use molt_kernel::chronicle;
use molt_kernel::index::{EventIndex, EventQuery};
use molt_kernel::memory::{FileMemoryStore, MemoryKind};
use molt_kernel::observatory::Observatory;
use molt_kernel::store::{EventStore, FileEventStore};
use serde::Serialize;
use std::env;

fn option(args: &[String], name: &str, default: &str) -> String {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
        .unwrap_or_else(|| default.into())
}

fn option_ref<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].as_str())
}

fn print_json<T: Serialize>(value: T) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn value<T: Serialize, E: std::fmt::Display>(
    result: Result<T, E>,
) -> Result<serde_json::Value, String> {
    result
        .map_err(|error| error.to_string())
        .and_then(|result| serde_json::to_value(result).map_err(|error| error.to_string()))
}

fn usage() {
    eprintln!("usage: molt <history|events|search|chronicle|lineage|memory|resources|claims|evidence|challenges|replay|state-hash|verify-history> [options]\noptions: --store PATH --memory PATH --agent ID --type EVENT_TYPE --contains TEXT --kind episodic|semantic|evidence|relationship|lineage");
}

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    let command = args.get(1).ok_or_else(|| {
        usage();
        "missing command".to_owned()
    })?;
    let store_path = option(&args, "--store", "events.jsonl");
    let memory_path = option(&args, "--memory", "memory.jsonl");
    let store = FileEventStore::open(&store_path).map_err(|error| error.to_string())?;
    let memory = FileMemoryStore::open(&memory_path).map_err(|error| error.to_string())?;
    let observatory = Observatory::new(&store, &memory);
    let selected_agent = option_ref(&args, "--agent");
    let output = match command.as_str() {
        "history" => value(observatory.history(selected_agent, None))?,
        "events" => value(observatory.history(selected_agent, option_ref(&args, "--type")))?,
        "search" => {
            let events = store.all().map_err(|error| error.to_string())?;
            let index = EventIndex::build(&events);
            value(Ok::<_, String>(
                index
                    .search(EventQuery {
                        actor_id: selected_agent,
                        event_type: option_ref(&args, "--type"),
                        contains: option_ref(&args, "--contains"),
                    })
                    .into_iter()
                    .cloned()
                    .collect::<Vec<_>>(),
            ))?
        }
        "chronicle" => {
            let events = store.all().map_err(|error| error.to_string())?;
            value(Ok::<_, String>(chronicle::from_events(&events)))?
        }
        "lineage" => value(observatory.lineage(args.get(2).ok_or("lineage requires EVENT_ID")?))?,
        "memory" => {
            let kind = option_ref(&args, "--kind").and_then(|kind| match kind {
                "episodic" => Some(MemoryKind::Episodic),
                "semantic" => Some(MemoryKind::Semantic),
                "evidence" => Some(MemoryKind::Evidence),
                "relationship" => Some(MemoryKind::Relationship),
                "lineage" => Some(MemoryKind::Lineage),
                _ => None,
            });
            value(observatory.memory(selected_agent.ok_or("memory requires --agent ID")?, kind))?
        }
        "resources" => value(observatory.resources(selected_agent))?,
        "claims" => value(observatory.claims(selected_agent))?,
        "evidence" => value(observatory.evidence(selected_agent))?,
        "challenges" => value(observatory.challenges(selected_agent))?,
        "replay" => value(observatory.replay_at(args.get(2).ok_or("replay requires EVENT_ID")?))?,
        "state-hash" => {
            value(observatory.state_hash_at(args.get(2).ok_or("state-hash requires EVENT_ID")?))?
        }
        "verify-history" => value(observatory.verify_history())?,
        _ => {
            usage();
            return Err(format!("unknown command: {command}"));
        }
    };
    print_json(output)
}
