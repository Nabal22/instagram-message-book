use chrono::{DateTime, Local, Utc};
use regex::Regex;
use regex::Captures; // Importation du type Captures
use crate::MessageConverted;
use imessage_database::{tables::messages::{Message, BubbleType}, util::dates::get_offset};

/// Make necessary replacements so that the text is ready for insertion
/// into latex
fn latex_escape(text: String) -> String {
    // TODO: gotta be a more efficient way to do this
    let escaped = text 
        // first, a bunch of weird characters replaced with ascii
        .replace("’", "'")
        .replace("“", "\"")
        .replace("”", "\"")
        .replace("…", "...")
        // now, actual latex escapes
        .replace(r"\", r"\textbackslash\ ")
        .replace("$", r"\$")
        .replace("%", r"\%")
        .replace("&", r"\&")
        .replace("_", r"\_")
        .replace("^", r"\textasciicircum\ ")
        .replace("~", r"\textasciitilde\ ")
        .replace("#", r"\#")
        .replace(r"{", r"\{")
        .replace(r"}", r"\}")
        .replace("\n", "\\newline\n") // since a single newline in latex doesn't make a line break, need to add explicit newlines
        // this last one is the "variation selector" which I think determines whether an emoji
        // should be displayed big or inline. The latex font doesn't support it, so we just strip it out.
        // More info here: https://stackoverflow.com/questions/38100329/what-does-u-ufe0f-in-an-emoji-mean-is-it-the-same-if-i-delete-it
        .replace("\u{FE0F}", "");

    // Now, we wrap emojis in {\emojifont XX}. The latex template has a different font for emojis, and
    // this allows emojis to use that font
    // TODO: Somehow move this regex out so we only compile it once
    let emoji_regex = Regex::new(r"(\p{Extended_Pictographic}+)").expect("Couldn't compile demoji regex");
    let demojid = emoji_regex.replace_all(&escaped, "{\\emojifont $1}").into_owned();

    demojid

}

struct LatexMessage {
    is_from_me: bool,
    body_text: Option<String>,
    attachment_count: i32,
    date: DateTime<Utc>,
}

impl LatexMessage {
    fn render(self) -> String {
        let mut content = match self.body_text {
            Some(ref text) => latex_escape(text.to_string()), // probably not ideal to be cloning here
            None => "".to_string(),
        };

        // add attachment labels
        if self.attachment_count > 0 {
            if content.len() > 0 {
                // add some padding if there was text in the message with the attachment
                content.push_str("\\enskip")
            }
            content.push_str(format!("\\fbox{{{} Attachment{}}}", self.attachment_count, if self.attachment_count == 1 {""} else {"s"}).as_ref());
        } 

        let date_str = self.date.format("%B %e, %Y").to_string();

        let mut rendered = format!("\\markright{{{}}}\n", date_str);

        rendered.push_str(&match self.is_from_me {
            // god generating latex code is so annoying with the escapes
            true => format!("\\leftmsg{{{}}}\n\n", content),
            false => format!("\\rightmsg{{{}}}\n\n", content),
        });

        rendered
    }
}

pub fn render_message(msg: &MessageConverted) -> String {


    let mut latex_msg = LatexMessage { 
        is_from_me: msg.is_from_me, 
        body_text: msg.text.clone(),
        attachment_count: 0,
        date: msg.date,
    };

    // for part in parts {
    //     match part {
    //         BubbleType::Text(text) => { latex_msg.body_text = Some(text.to_owned())},
    //         BubbleType::Attachment => { latex_msg.attachment_count += 1 }
    //         _ => ()
    //     }
    // }

    latex_msg.render()
}