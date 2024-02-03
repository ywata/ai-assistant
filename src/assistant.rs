use std::{fs, io};
use std::path::{PathBuf};
use std::sync::{Arc};

use std::borrow::Borrow;

use std::process::Output;
use regex::{Regex};

use iced::widget::{self,
                   column, row, text_editor};
use iced::{
    Application, Command, Element, Settings, Theme,
};
use async_openai::{
    config::OpenAIConfig,
    Client,
};
use async_openai::types::{AssistantObject, ThreadObject};
use thiserror::Error;

use clap::{Parser, Subcommand};
use serde::Deserialize;
use tokio::sync::Mutex;

use openai_api::{connect, Context, Conversation, OpenAi, OpenAIApiError};

use crate::compile::compile;
use crate::scenario::Prompt;

//use thiserror::Error;
pub mod config;
mod scenario;
mod compile;

#[derive(Clone, Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// yaml file to store credentials
    #[arg(long)]
    yaml: String,
    #[arg(long)]
    key: String,
    #[arg(long)]
    name: String,
    #[arg(long)]
    prompt_file: String,
    #[arg(long)]
    output_dir: String,
    #[arg(long)]
    tag: String,

    #[clap(subcommand)]
    command: Commands,
}


#[derive(Clone, Debug, Subcommand)]
enum Commands {
    AskAi{
        #[arg(long)]
        markers: Option<Vec<String>>,
    }
}

impl Default for Commands {
    fn default() -> Self {
        Commands::AskAi {markers: None,}
    }
}

impl Cli {
    fn get_markers(&self) -> Result<Vec<Regex>, regex::Error>{
        let res = match &self.command {
            Commands::AskAi{markers: Some(m)} => {
                let v: Result<Vec<_>, _> = m.iter().map(|s| Regex::new(s)).collect();
                v
            },
            _ => Ok(vec![Regex::new("asdf").unwrap()]),
        };
        res
    }
}

impl Default for Cli {
    fn default() -> Self {
        Cli {yaml:"service.yaml".to_string(),
            key:"openai".to_string(),
            name:"ai assistant".to_string(),
            prompt_file: "prompt.txt".to_string(),
            output_dir: "output".to_string(),
            tag: "default".to_string(),
            command: Commands::default(),
        }
    }
}



pub fn main() -> Result<(), AssistantError> {
    let args = Cli::parse();
    println!("{:?}", args);
    let config_content = fs::read_to_string(&args.yaml)?;
    let config: OpenAi = config::read_config(Some(&args.key), &config_content)?;
    let prompt_content = fs::read_to_string(&args.prompt_file)?;
    let prompt: Prompt = config::read_config(None, &prompt_content)?;

    let _markers = args.get_markers()?;


    let settings = Settings::default();
    let updated_settings = Settings {
        flags: (args, config, prompt),
        ..settings
    };

    Ok(Model::run(updated_settings)?)

}





#[derive(Clone, Debug)]
enum Message {
    Connected(Result<Context, OpenAIApiError>),
    InputLoaded(Option<String>),
    OpenFile(AreaIndex),
    FileOpened(Result<(AreaIndex, (PathBuf, Arc<String>)), (AreaIndex, Error)>),
    ActionPerformed(AreaIndex, text_editor::Action),
    AskAi,
    Answered(Result<String, OpenAIApiError>),
    Compiled(Result<Output, AssistantError>),
}

#[derive(Debug)]
struct EditArea {
    path: Option<PathBuf>,
    content: text_editor::Content,
    is_dirty: bool,
    is_loading: bool,
}

impl Default for EditArea {
    fn default() -> Self {
        EditArea {
            path: None,
            content: text_editor::Content::new(),
            is_dirty: false,
            is_loading: false,
        }
    }
}

impl EditArea {
    fn default_path(self, path: &String) -> Self{
        let default: EditArea = EditArea::default();
        let pbuf : PathBuf = PathBuf::from(path);
        
        EditArea{
            path: Some(pbuf),
            ..default
        }
    }
}


#[derive(Debug, Copy, Clone)]
enum AreaIndex{
    Prompt = 0,
    Input = 1,
    Result = 2,
}

#[derive(Debug)]
struct Model {
    env: Cli,
    context: Option<Arc<Mutex<Context>>>,
    edit_areas: Vec<EditArea>,
}


#[derive(Error, Clone, Debug)]
pub enum AssistantError {
    #[error("file already exists for the directory")]
    FileExists(),
    #[error("API access failed")]
    AppAccessError,
}

impl From<iced::Error> for AssistantError {
    fn from(err: iced::Error) -> AssistantError {
        dbg!(err);
        AssistantError::AppAccessError
    }
}

impl From<openai_api::OpenAIApiError> for AssistantError {
    fn from(error: openai_api::OpenAIApiError) -> AssistantError {
        dbg!(error);
        AssistantError::AppAccessError
    }
}
impl From<std::io::Error> for AssistantError {
    fn from(error: std::io::Error) -> AssistantError {
        dbg!(error);
        AssistantError::AppAccessError
    }
}
impl From<config::ConfigError> for AssistantError {
    fn from(error: config::ConfigError) -> AssistantError {
        dbg!(error);
        AssistantError::AppAccessError
    }
}

impl From<regex::Error> for AssistantError {
    fn from(error: regex::Error) -> AssistantError {
        dbg!(error);
        AssistantError::AppAccessError
    }
}



async fn save_and_compile(output_path:PathBuf, code: String) -> Result<Output, AssistantError> {
    let _write_res = tokio::fs::write(&output_path, code).await?;
    let res = compile(output_path).await?;

    Ok(res)
}

async fn load_input(prompt: Prompt, tag: String) -> Option<String> {
    prompt.inputs.iter().find(|i| i.tag == tag).map(|i| i.text.clone())
}

fn get_content(contents: Vec<Mark>) -> Option<Mark> {
    let mut res = None;
    for c in contents {
        match c {
            Mark::Content {..} => {
                res = Some(c);
                break;
            },
            _ =>  (),
        }
    }
    res
}

impl Application for Model {
    type Message = Message;
    type Theme = Theme;
    type Executor = iced::executor::Default;
    type Flags = (Cli, OpenAi, Prompt);

    fn  new(flags: (Cli, OpenAi, Prompt)) -> (Model, Command<Message>) {
        //let prompt_path = PathBuf::from(&flags.0.prompt_file);
        //let input_path = PathBuf::from(&flags.0.input_file);
        let default = EditArea::default();
        let content = text_editor::Content::with_text(&flags.2.instruction);
        let mut prompt = EditArea{
            content,
            ..default
        };
        let default = EditArea::default();
        let mut input = EditArea::default();
        let result = EditArea::default();
        let name = flags.0.name.clone();

        (
            Model {env: flags.0.clone(), context: None,
                   edit_areas: vec![prompt, input, result]
            },
            Command::batch(vec![
                Command::perform(connect(flags.1.clone(), name, flags.2.instruction.clone()), Message::Connected),
                Command::perform(load_input(flags.2, flags.0.tag.clone()), Message::InputLoaded),
            ])
        )
    }

    fn title(&self) -> String {
        "title".to_string()
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Connected(Ok(ctx)) => {
                self.context = Some(Arc::new(Mutex::new(ctx)));
                Command::none()
            },
            Message::OpenFile(_idx) => {

                Command::none()
            },
            Message::Connected(Err(_)) | Message::InputLoaded(None)=> {
                Command::none()
            },

            Message::InputLoaded(Some(text)) => {
                let default = EditArea::default();
                self.edit_areas[AreaIndex::Input as usize] = EditArea{
                    content: text_editor::Content::with_text(&text),
                    ..default
                };
                Command::none()
            },
            Message::FileOpened(result) => {
                if let Ok((idx, (path, contents))) = result {
                    let content = text_editor::Content::with_text(&contents);
                    let default = EditArea::default();
                    self.edit_areas[idx as usize] = EditArea{
                        path: Some(path),
                        content,
                        ..default
                    };
                }
                Command::none()
            },
            Message::ActionPerformed(idx, action) => {
                self.edit_areas[idx as usize].content.perform(action);
                Command::none()
            },
            Message::AskAi => {
                let input = self.edit_areas[AreaIndex::Input as usize].content.text();
                if let Some(context) = self.context.clone() {
                    let pass_context = context.clone();
                    let _handle = tokio::spawn(async move {
                        let mut ctx = context.lock().await;
                        let res = ctx.add_conversation(Conversation::ToAi { message: input });
                        res
                    });

                    Command::perform(openai_api::ask(pass_context,
                                                     self.edit_areas[AreaIndex::Input as usize].content.text()),
                                     Message::Answered)

                } else {
                    Command::none()
                }
            },
            Message::Answered(res) => {
                let mut command = Command::none();
                match res {

                    Ok(text) =>{
                        let opt_markers = self.env.get_markers();
                        let mut content = text_editor::Content::with_text("");
                        let context = self.context.clone().unwrap();
                        let cloned_text = text.clone();
                        let _handle = tokio::spawn(async move {
                            let mut ctx = context.lock().await;
                            let res = ctx.add_conversation(Conversation::FromAi { message: cloned_text });
                            res
                        });

                        match opt_markers {
                            Ok(markers) => {
                                let contents = split_code(&text, &markers.clone()).clone();
                                let json = String::from("json");
                                let fsharp = String::from("fsharp");

                                if let Some(Mark::Content{text:text, lang: Some(matcher)}) = get_content(contents) {
                                    if matcher == json {
                                        let response = serde_json::from_str::<Response>(&text);
                                        if let Ok(_resp) = response {
                                            content = text_editor::Content::with_text(&text);
                                        }
                                    } else if matcher == fsharp {
                                        content = text_editor::Content::with_text(&text);
                                        let mut path = PathBuf::from(&self.env.output_dir);
                                        path.push("sample.fs");
                                        command = Command::perform(save_and_compile(path, text), Message::Compiled);
                                    } else {
                                        //
                                    }

                                }
                            },
                            _ => (),
                        }

                        let default = EditArea::default();
                        self.edit_areas[AreaIndex::Result as usize] = EditArea{
                            content,
                            ..default
                        };
                    }
                    _ => println!("FAILED"),
                }
                command
            },
            Message::Compiled(msg) => {
                println!("{:?}", msg);
                Command::none()
            },

        }
    }

    fn view(&self) -> Element<Message> {
        let  vec = &self.edit_areas;
        row![
            column![
                text_editor(&vec.get(AreaIndex::Prompt as usize).unwrap().content)
                  .on_action(|action|Message::ActionPerformed(AreaIndex::Prompt, action)),
                text_editor(&vec.get(AreaIndex::Input as usize).unwrap().content)
                  .on_action(|action|Message::ActionPerformed(AreaIndex::Input, action)),
            ],
            column![
                button("ask ai")
                .on_press(Message::AskAi)
                , text_editor(&vec.get(AreaIndex::Result as usize).unwrap().content)
                //.on_action(|action|Message::ActionPerformed(AreaIndex::Result, action)),
                ]
        ].into()
    }
}



async fn load_file<T: Copy>(idx: T, path: PathBuf) -> Result<(T, (PathBuf, Arc<String>)), (T, Error)> {
    let contents = tokio::fs::read_to_string(&path)
        .await
        .map(Arc::new)
        .map_err(|error| (idx, Error::IoError(error.kind())))?;

    Ok((idx, (path.clone(), contents)))
}




#[derive(Clone, Debug, Deserialize)]
struct Response {
    missing: Vec<String>,
    possible: Vec<String>,
}



#[derive(Debug, Clone)]
enum Error {
    APIError,
    DialogClosed,
    IoError(io::ErrorKind),
}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Error {
        dbg!(error);

        Error::APIError
    }
}

fn button(text: &str) -> widget::Button<'_, Message> {
    widget::button(text).padding(1)
}


#[derive(Clone, Debug, PartialEq)]
enum Mark{
    Marker{text: String, lang: Option<String>,},
    Content{text: String, lang: Option<String>, },
}

fn split_code(source:&str, markers:&Vec<regex::Regex>) -> Vec<Mark> {
    let mut curr_pos:usize = 0; // index to source
    let max = source.len();
    let mut result = Vec::new();
    let mut lang = None;
    for marker in markers {
        // source is searched starting from curr_pos to max
        if let Some(matched) = marker.captures(&source[curr_pos..max]) {
            let all_matched = matched.get(0).unwrap();

            let pos = all_matched.range().start; // position in [curr..pos.. <max]
            if 0 != pos {
                // As there is some text before marker, it becomes Content
                result.push(Mark::Content {text: String::from(&source[curr_pos..(curr_pos + pos)]), lang: lang.clone()});
                curr_pos += pos;
            }
            lang = matched.get(1).map(|m| String::from(&source[curr_pos + m.range().start - pos..(curr_pos + m.range().end - pos)]));
            let r = all_matched.range();
            let len = r.end - r.start;

            result.push(Mark::Marker{text: String::from(&source[curr_pos..curr_pos + len]), lang: lang.clone()});
            curr_pos += len;
        } else {
            // not marker found. This might be a error.
        }

    }
    if curr_pos < max {
        result.push(Mark::Content{text: String::from(&source[curr_pos..max]), lang: lang.clone()});
    }

    result
}


#[cfg(test)]
mod test {
    use crate::config::read_config;
    use super::*;
    #[test]
    fn test_split_mark_only() {
        let input = r#"```start
```"#.to_string();
        let markers = vec!["```(start)".to_string(), "```".to_string()];
        let mut cli = Cli::default();
        cli = Cli{
            command: Commands::AskAi {markers: Some(markers)},
            ..cli
        };
        let rex_markers= cli.get_markers();

        let res = split_code(&input, &rex_markers.unwrap());
        assert_eq!(res.len(), 3);
        assert_eq!(res.get(0), Some(&Mark::Marker{text:"```start".to_string(), lang: Some("start".to_string())}));
        assert_eq!(res.get(1), Some(&Mark::Content{text:"\n".to_string(), lang: Some("start".to_string())}));
        assert_eq!(res.get(2), Some(&Mark::Marker{text:"```".to_string(), lang: None}));
    }
    #[test]
    fn test_split_mark_backquotes() {
        let input = r#"```start
asdf
```"#.to_string();
        let markers = vec!["```start```".to_string()];
        let mut cli = Cli::default();
        cli = Cli{
            command: Commands::AskAi {markers: Some(markers)},
            ..cli
        };
        let rex_markers= cli.get_markers();
        let res = split_code(&input, &rex_markers.unwrap());
        assert_eq!(res.len(), 1);
        assert_eq!(res.get(0), Some(&Mark::Content{text:"```start\nasdf\n```".to_string(), lang: None}));
    }

    #[test]
    fn test_split_mark_and_content() {
        let input = r#"asdf
```start
hjklm
```
xyzw
"#.to_string();
        let markers = vec!["```(start)".to_string(), "```".to_string()];
        let mut cli = Cli::default();
        cli = Cli{
            command: Commands::AskAi {markers: Some(markers)},
            ..cli
        };
        let rex_markers= cli.get_markers();

        let res = split_code(&input, &rex_markers.unwrap());
        assert_eq!(res.len(), 5);
        assert_eq!(res.get(0), Some(&Mark::Content{text:"asdf\n".to_string(), lang: None}));
        assert_eq!(res.get(1), Some(&Mark::Marker{text:"```start".to_string(), lang: Some("start".to_string())}));
        //assert_eq!(res.get(2), Some(&Mark::Content{text:"\nhjklm\n".to_string(), lang: Some("start".to_string())}));
        //assert_eq!(res.get(3), Some(&Mark::Marker{text:"```".to_string(), lang: None}));
        //assert_eq!(res.get(4), Some(&Mark::Content{text:"\nxyzw\n".to_string(), lang: None}));

    }
    #[test]
    fn test_regex() {
        let rex_str = r#"^([a-zA-Z]+)[0-9]+"#;
        let rex = Regex::new(rex_str).unwrap();

        let input = r#"abcd123x"#;

        if let Some(m1) = rex.captures(input) {
            let g0 = m1.get(0).unwrap().as_str();
            let g1 = m1.get(1).unwrap().as_str();
            assert_eq!(g0, "abcd123");
            assert_eq!(g1, "abcd");
            return;
        }
        assert_eq!(true, false);
    }
    #[test]
    fn test_convert_prompt() {
        let prompt_content = r#"
        instruction: |
          asdf
          asdf
        inputs:
          - tag: abc
            text: |
             xyz
        "#.to_string();
        let prompt: Prompt = read_config(None, &prompt_content).unwrap();
        assert_eq!(prompt.instruction, "asdf\nasdf\n".to_string());
    }
}