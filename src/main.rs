use serde::{Deserialize, Serialize};
use std::fs;
use chrono::{DateTime, Local, NaiveDateTime, Utc};
use std::{path::{PathBuf, Path}, fs::{File, copy}, io::{Read, BufReader,  Write}};
use render::render_message;

mod render;

const TEMPLATE_DIR: &str = "templates";

#[derive(Debug, Deserialize, Serialize)]
struct Conversation {
    participants: Vec<Participant>,
    messages: Vec<MessageInsta>,
}

#[derive(Debug, Deserialize, Serialize)]
struct MessageInsta {
    sender_name: String,
    timestamp_ms: u64,
    content: Option<String>,
    audio_files: Option<Vec<AudioFile>>,
    share: Option<Share>,
}

struct MessageConverted {
    rowid: i64,
    guid: String,
    text: Option<String>,
    service: Option<String>,
    handle_id: Option<i64>,
    date: DateTime<Utc>,
    timestamp_ms : i64,
    date_read: i64,
    date_delivered: i64,
    is_from_me: bool,
    is_read: bool,
    item_type: i32,
    group_title: Option<String>,
    group_action_type: i32,
    associated_message_guid: Option<String>,
    associated_message_type: Option<i32>,
    balloon_bundle_id: Option<String>,
    expressive_send_style_id: Option<String>,
    thread_originator_guid: Option<String>,
    thread_originator_part: Option<String>,
    date_edited: i64,
    chat_id: Option<i64>,
    num_attachments: i32,
    deleted_from: Option<String>,
    num_replies: i32,
}

#[derive(Debug, Deserialize, Serialize)]
struct Participant {
    name: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct AudioFile {
    uri: String,
    creation_timestamp: u64,
}

#[derive(Debug, Deserialize, Serialize)]
struct Share {
    link: String,
    #[serde(rename = "share_text")]
    share_text: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output_dir = Path::new("output");
    if !output_dir.exists() {
        fs::create_dir(output_dir).expect("Impossible de créer le dossier output");
    }

    // Ouvrir et lire le fichier JSON
    let file = File::open("conv.json").expect("Impossible d'ouvrir le fichier");
    let reader = BufReader::new(file);
    let conversation: Conversation = serde_json::from_reader(reader).expect("Impossible de parser le JSON");

    let filtered_msgs = convert_json_to_messages(&serde_json::to_string(&conversation).unwrap());

    // sort the messages by date
    let mut filtered_msgs = filtered_msgs;
    filtered_msgs.sort_by(|a, b| a.date.cmp(&b.date));

    let mut chapters: Vec<String> = vec![]; // the names of the chapters created, like 2020-11
    let mut current_output_info: Option<(String, File)> = None; // chapter_name, File
    for mut msg in filtered_msgs {

        // this is a mess. Basically we are updating current_output_file to be correct for the current message.
        // Since the messages are in chronological order, marching through messages and updating this, creating
        // files as necessary, should work
        //let msg_date = Utc.timestamp_millis_opt(msg.timestamp_ms as i64).unwrap();

        let timestampNoConvert = msg.timestamp_ms as i64;
        let timestamp = timestampNoConvert / 1000;

        let naive = NaiveDateTime::from_timestamp(timestamp, 0);
    
        // Create a normal DateTime from the NaiveDateTime
        let msg_date: DateTime<Utc> = DateTime::from_utc(naive, Utc);

        let chapter_name = msg_date
            .format("ch-%Y-%m")
            .to_string();
        let out_fname = format!("{}.tex", &chapter_name);
        let create = match &current_output_info {
            None => {true},
            Some((ref name, _)) => {name != &chapter_name},
        };
        if create {
            println!("Starting chapter {}", &chapter_name);
            let out_path = Path::join(output_dir, &out_fname);
            let mut f = File::create(&out_path)
                // .expect("failed to create output file")
                .unwrap_or_else(|e| panic!("Failed to create output file: {} - {:?}", &out_path.to_string_lossy(), e));
            f.write(format!("\\chapter{{{}}}\n\n", msg_date.format("%B %Y").to_string()).as_bytes()).expect("Could not write to chapter file");
            current_output_info = Some((chapter_name.clone(), f));
            chapters.push(chapter_name);
        }

        // match msg.gen_text(&db) {
        //     Ok(_) => {
                // Successfully generated message, proceed with rendering and writing to output file
                let rendered = render_message(&msg);
                let mut output_file = &current_output_info.as_ref().expect("Current output info was none while processing message").1;
                output_file.write(rendered.as_bytes()).expect("Unable to write message to output file");
        //     }
        //     Err(err) => {
        //         // Handle the error gracefully, you can log it or ignore it depending on your requirements
        //         eprintln!("Failed to generate message: {:?}", err);
        //     }
        // }
    }

    // Once we create all the chapter files, we need to create the main.tex file to include them 
    let mut main_template_file = File::open([TEMPLATE_DIR, "main.tex.template"].iter().collect::<PathBuf>()).expect("Could not open template file");
    let mut main_template = String::new();
    main_template_file.read_to_string(&mut main_template).expect("could not read template main.tex");
    let mut main_tex_file = File::create(Path::join(output_dir, "main.tex")).expect("could not create main.tex in output dir");
    main_tex_file.write_all(main_template.as_bytes()).expect("Could not write main.tex");

    // now add the chapters to the main file
    chapters.iter()
        .for_each(|chapter_name| {
            main_tex_file.write(
                format!("\\include{{{}}}\n", chapter_name).as_bytes()
            )
            .expect("failed to write main file");
        });

    // and finish it with \end{document}
    // TODO: we should really do this with a templating engine
    main_tex_file.write(r"\end{document}".as_bytes()).expect("unable to finish main.tex");

    // finally, copy over the makefile
    copy([TEMPLATE_DIR, "Makefile"].iter().collect::<PathBuf>(), Path::join(output_dir, "Makefile")).expect("Could not copy makefile");

    Ok(())
}


fn convert_json_to_messages(json_data: &str) -> Vec<MessageConverted> {
    let json_obj: serde_json::Value = serde_json::from_str(json_data).unwrap();

    let messages = json_obj["messages"].as_array().unwrap_or_else(|| {
        panic!("Expected 'messages' array in JSON data.");
    });

    let mut result = Vec::new();

    for message in messages {
        let sender_name = message["sender_name"].as_str().unwrap_or("");
        let timestamp_ms = message["timestamp_ms"].as_i64().unwrap_or(0);
        let content = message["content"].as_str();

        let timestamp = timestamp_ms / 1000;

        let naive = NaiveDateTime::from_timestamp(timestamp, 0);
    
        // Create a normal DateTime from the NaiveDateTime
        let msg_date: DateTime<Utc> = DateTime::from_utc(naive, Utc);

        // Build a Message struct based on the JSON data
        let new_message = MessageConverted {
            rowid: 0, // Set appropriate value
            guid: "".to_string(), // Set appropriate value
            text: content.map(|c| c.to_string()),
            service: Some("iMessage".to_string()), // Assuming all messages are from iMessage
            handle_id: Some(380), // Set appropriate value
            date: msg_date,
            timestamp_ms: timestamp_ms, // Set appropriate value
            date_read: 0, // Set appropriate value
            date_delivered: 0, // Set appropriate value
            is_from_me: sender_name == "NABAL", // Set based on sender_name
            is_read: true, // Assuming all messages are read
            item_type: 0, // Set appropriate value
            group_title: None,
            group_action_type: 0, // Set appropriate value
            associated_message_guid: None,
            associated_message_type: Some(0), // Set appropriate value
            balloon_bundle_id: None,
            expressive_send_style_id: None,
            thread_originator_guid: None,
            thread_originator_part: None,
            date_edited: 0, // Set appropriate value
            chat_id: Some(420), // Set appropriate value
            num_attachments: 0, // Set appropriate value
            deleted_from: None,
            num_replies: 0, // Set appropriate value
        };

        result.push(new_message);
    }

    result
}