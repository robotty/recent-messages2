use chrono::{DateTime, TimeZone, Utc};
use itertools::Itertools;
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;
use std::fs::OpenOptions;

#[derive(Debug, Clone, Deserialize)]
pub struct StoredMessage {
    #[serde(deserialize_with = "from_utc_milliseconds")]
    pub time_received: DateTime<Utc>,
    pub message_source: String,
}

fn from_utc_milliseconds<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
where
    D: Deserializer<'de>,
{
    let millis = Deserialize::deserialize(deserializer)?;
    Ok(Utc.timestamp_millis(millis))
}

fn main() {
    let messages = load_messages_from_disk();

    let f = OpenOptions::new()
        .write(true)
        .append(false)
        .create(true)
        .truncate(true)
        .open("messages.csv")
        .unwrap();
    let mut csv_writer = csv::Writer::from_writer(f);

    let total = messages.len();
    let mut idx: usize = 0;
    print!("Exporting 0/{}", total);
    for (channel_login, messages) in messages {
        for message in messages {
            csv_writer
                .write_record(&[
                    &channel_login,
                    &message.time_received.to_rfc3339(),
                    &message.message_source,
                ])
                .unwrap();
        }
        idx += 1;
        print!("\rExporting {}/{}", idx, total);
    }
    println!();
}

fn load_messages_from_disk() -> HashMap<String, Vec<StoredMessage>> {
    tracing::info!("Loading snapshot of messages from disk...");
    let directory_contents = std::fs::read_dir("messages").expect("messages directory missing");

    let mut messages_map = HashMap::new();

    let dir_contents = directory_contents.collect_vec();

    let mut idx: usize = 0;
    let total = dir_contents.len();
    print!("Reading 0/{}", total);

    for dir_entry in dir_contents {
        let file_path = dir_entry.unwrap().path();
        if file_path
            .extension()
            .map(|ext| ext != "dat")
            .unwrap_or(true)
        {
            // either has an extension that is not `dat` or has no extension
            tracing::debug!(
                "Ignoring file {} from messages directory, extension is not `dat`",
                file_path.to_string_lossy()
            );
            continue;
        }

        let channel_login = file_path.file_stem().unwrap().to_str().unwrap().to_owned();

        let file = std::fs::File::open(file_path).unwrap();
        let channel_messages = rmp_serde::decode::from_read(file).unwrap();
        messages_map.insert(channel_login, channel_messages);

        idx += 1;
        print!("\rReading {}/{}", idx, total);
    }

    println!();

    messages_map
}
