use chrono::{DateTime, TimeZone, Utc};
use itertools::Itertools;
use serde::{Deserialize, Deserializer};
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
    let dir_contents = std::fs::read_dir("messages")
        .expect("messages directory missing")
        .collect_vec();
    let output_file = OpenOptions::new()
        .write(true)
        .append(false)
        .create(true)
        .truncate(true)
        .open("messages.csv")
        .unwrap();
    let mut csv_writer = csv::Writer::from_writer(output_file);

    let mut idx: usize = 0;
    let total = dir_contents.len();
    print!("Processing... 0/{}", total);

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
                file_path.display()
            );
            continue;
        }

        let channel_login = file_path.file_stem().unwrap().to_str().unwrap().to_owned();

        let file = std::fs::File::open(file_path).unwrap();
        let channel_messages: Vec<StoredMessage> = rmp_serde::decode::from_read(file).unwrap();

        for message in channel_messages {
            csv_writer
                .write_record(&[
                    &channel_login,
                    &message.time_received.to_rfc3339(),
                    &message.message_source,
                ])
                .unwrap();
        }

        idx += 1;
        print!("\rProcessing... {}/{}", idx, total);
    }

    println!(" Done");
}
