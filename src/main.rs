use std::{path::{PathBuf, Path}, fs::{File, create_dir_all}, io::{Read, Write}, rc::Rc};

use clap::Parser;
use imessage_database::{tables::{table::{get_connection, Table, DEFAULT_PATH_IOS, MESSAGE_ATTACHMENT_JOIN, MESSAGE, RECENTLY_DELETED, CHAT_MESSAGE_JOIN}, messages::Message, chat::Chat}, error::table::TableError, util::dates::get_offset};
use anyhow::Result;
use render::render_message;
use rusqlite::types::Value;

mod render;

const TEMPLATE_DIR: &str = "templates";

// default ios sms.db path is <backup-path>/3d/3d0d7e5fb2ce288813306e4d4636395e047a3d2

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    // TODO: add support for mac db
    backup_path: PathBuf,
    /// Phone number of the conversation to export, of the form '+15555555555'
    recipient: String,
    /// The directory to create the .tex files in
    #[arg(short, long, default_value = "output")]
    output_dir: PathBuf,
}


fn iter_messages(db_path: &PathBuf, chat_identifier: &str, output_dir: &PathBuf) -> Result<(), TableError> {
    let db = get_connection(db_path).unwrap();

    let mut chat_stmt = Chat::get(&db)?;
    let chats: Vec<Chat> = chat_stmt
        .query_map([], |row| Chat::from_row(row))
        .unwrap()
        .filter_map(|c| c.ok())
        .filter(|c| c.chat_identifier == chat_identifier)
        .collect(); // we collect these into a vec since there should only be a couple, we don't need to stream them
    
    let chat_ids: Vec<i32> = chats.iter().map(|c| c.rowid).collect();

    // let mut msg_stmt = Message::get(&db)?;
    // using rarray as in the example at https://docs.rs/rusqlite/0.29.0/rusqlite/vtab/array/index.html to check if chat is ok
    // SQL almost entirely taken from imessage-database Message::get, with added filtering
    rusqlite::vtab::array::load_module(&db).expect("failed to load module");
    let mut msg_stmt = db.prepare(&format!(
        "SELECT
                 *,
                 c.chat_id,
                 (SELECT COUNT(*) FROM {MESSAGE_ATTACHMENT_JOIN} a WHERE m.ROWID = a.message_id) as num_attachments,
                 (SELECT b.chat_id FROM {RECENTLY_DELETED} b WHERE m.ROWID = b.message_id) as deleted_from,
                 (SELECT COUNT(*) FROM {MESSAGE} m2 WHERE m2.thread_originator_guid = m.guid) as num_replies
             FROM
                 message as m
                 LEFT JOIN {CHAT_MESSAGE_JOIN} as c ON m.ROWID = c.message_id
             WHERE
                 c.chat_id IN rarray(?1)
             ORDER BY
                 m.date
             LIMIT
                 10000;
            "
        )).expect("unable to build messages query");

    // unfortunately I don't think there is an easy way to add a WHERE clause
    // to the statement generated by Message::get.
    // So instead I generated my own SQL statement, based on Message::get
    // and I need to pass in the valid chat ids
    let chat_id_values = Rc::new(chat_ids.iter().copied().map(Value::from).collect::<Vec<Value>>());
    let msgs = msg_stmt
        .query_map([chat_id_values], |row| Message::from_row(row))
        .unwrap()
        .filter_map(|m| m.ok());
        // .filter(|m| m.chat_id.is_some_and(|id| chat_ids.contains(&id))); // not needed with new sql filtering

    chats.iter().for_each(|c| println!("Found chat {:?}", c));

    // need to create output dir first, so we can create files inside it
    create_dir_all(output_dir).expect("Could not create output directory");

    let filtered_msgs = msgs
        .filter(|m| !m.is_reaction() && !m.is_announcement() && !m.is_shareplay());

    let mut chapters: Vec<String> = vec![]; // the names of the chapters created, like 2020-11
    let mut current_output_info: Option<(String, File)> = None; // chapter_name, File
    for mut msg in filtered_msgs {

        // this is a mess. Basically we are updating current_output_file to be correct for the current message.
        // Since the messages are in chronological order, marching through messages and updating this, creating
        // files as necessary, should work
        let msg_date = msg.date(&get_offset()).expect("could not find date for message");
        let chapter_name = msg_date
            .format("%Y-%m")
            .to_string();
        let out_fname = format!("{}.tex", &chapter_name);
        let create = match &current_output_info {
            None => {true},
            Some((ref name, _)) => {name != &chapter_name},
        };
        if create {
            if current_output_info.is_some() {
                println!("Finished chapter {}", &chapter_name);
            }
            let out_path = Path::join(output_dir, &out_fname);
            let mut f = File::create(&out_path)
                // .expect("failed to create output file")
                .unwrap_or_else(|e| panic!("Failed to create output file: {} - {:?}", &out_path.to_string_lossy(), e));
            f.write(format!("\\chapter{{{}}}\n\n", msg_date.format("%B %Y").to_string()).as_bytes()).expect("Could not write to chapter file");
            current_output_info = Some((chapter_name.clone(), f));
            chapters.push(chapter_name);
        }

        msg.gen_text(&db).expect("failed to generate message");


        // this will need to be much more complicated eventually to handle images, reactions, ... ugh thought this project would be simple
        let rendered = render_message(&msg);

        let mut output_file = &current_output_info.as_ref().expect("Current output info was none while processing message").1;
        output_file.write(rendered.as_bytes()).expect("Unable to write message to output file");

        // println!("Added message {:?} to output file {:?}", msg, current_output_info.as_ref().map(|x| &x.0));
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

    Ok(())
}

fn main() {
    let args = Args::parse();
    let mut backup_path = args.backup_path.clone();
    backup_path.push(DEFAULT_PATH_IOS);
    iter_messages(&backup_path, &args.recipient, &args.output_dir).expect("failed :(");



    println!("Hello!")
}
