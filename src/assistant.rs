use std::{fs, io};
use std::path::{PathBuf};
use std::sync::Arc;

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

use openai_api::{OpenAi, OpenAIApiError};
//use thiserror::Error;
pub mod config;

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
    input_file: String,
    #[arg(long)]
    output_dir: String,

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
        Commands::AskAi {markers: None}
    }
}

impl Cli {
    fn get_markers(&self) -> Option<Vec<String>>{
        match &self.command {
            Commands::AskAi{markers: m} => m.clone(),
        }
    }
}

impl Default for Cli {
    fn default() -> Self {
        Cli {yaml:"service.yaml".to_string(),
            key:"openai".to_string(),
            name:"ai assistant".to_string(),
            input_file: "input.txt".to_string(),
            prompt_file: "prompt.txt".to_string(),
            output_dir: "output".to_string(),
            command: Commands::default(),
        }
    }
}



pub fn main() -> Result<(), AssistantError> {
    let args = Cli::parse();
    println!("{:?}", args);
    let config_content = fs::read_to_string(&args.yaml)?;
    let config: OpenAi = config::read_config(&args.key, &config_content)?;

    let prompt = fs::read_to_string(&args.prompt_file)?;


    let settings = Settings::default();
    let updated_settings = Settings {
        flags: (args, config, prompt),
        ..settings
    };

    Ok(Model::run(updated_settings)?)

}





#[derive(Debug, Clone)]
enum Message {
    Connected(Result<(Client<OpenAIConfig>, ThreadObject, AssistantObject), AssistantError>),
    OpenFile(AreaIndex),
    FileOpened(Result<(AreaIndex, (PathBuf, Arc<String>)), (AreaIndex, Error)>),
    ActionPerformed(AreaIndex, text_editor::Action),
    AskAi,
    Answered(Result<String, OpenAIApiError>),
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
    client: Option<Client<OpenAIConfig>>,
    access: Option<(ThreadObject, AssistantObject)>,
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




impl Application for Model {
    type Message = Message;
    type Theme = Theme;
    type Executor = iced::executor::Default;
    type Flags = (Cli, OpenAi, String);

    fn  new(flags: (Cli, OpenAi, String)) -> (Model, Command<Message>) {
        let prompt_path = PathBuf::from(&flags.0.prompt_file);
        let input_path = PathBuf::from(&flags.0.input_file);
        let prompt = EditArea::default();
        let input = EditArea::default();
        let result = EditArea::default();
        let name = flags.0.name.clone();
        (
            Model {env: flags.0, client: None, access: None,
                   edit_areas: vec![prompt, input, result]
            },
            Command::batch(vec![
                Command::perform(load_file(AreaIndex::Prompt, prompt_path), Message::FileOpened),
                Command::perform(load_file(AreaIndex::Input, input_path), Message::FileOpened),
                Command::perform(connect(flags.1.clone(), name, flags.2.clone()), Message::Connected)
            ])
        )
    }

    fn title(&self) -> String {
        "title".to_string()
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Connected(val) => {
                match val {
                    Ok((c, t, a)) => {
                        self.access = Some((t, a));
                        self.client = Some(c);
                        Command::none()
                    },
                    Err(_err) => Command::none(),

                }
            },
            Message::OpenFile(_idx) => {

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
                println!("{}", &input);
                let thread = self.access.as_ref().unwrap().0.clone();
                let assistant = self.access.as_ref().unwrap().1.clone();
                let client = self.client.as_ref().unwrap().clone();
                Command::perform(openai_api::ask(client, thread, assistant,
                                                 self.edit_areas[AreaIndex::Input as usize].content.text()),
                                 Message::Answered)
            },
            Message::Answered(res) => {
                match res {

                    Ok(text) =>{
                        let option_markers = self.env.get_markers();
                        let code = match option_markers {
                            None => {
                                &text
                            },
                            Some(markers) => {
                                let contents = split_code(&text, &markers);
                                let mut result = "";
                                for c in contents {
                                    match c {
                                        Mark::Marker{text:_} => (),
                                        Mark::Content{text} => {
                                            result = text;
                                            break;
                                        }
                                    }
                                }
                                result
                            }
                        };

                        let content = text_editor::Content::with_text(code);
                        let default = EditArea::default();
                        self.edit_areas[AreaIndex::Result as usize] = EditArea{
                            content,
                            ..default
                        };

                    }
                    _ => println!("FAILED"),
                }
                Command::none()
            }
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
                , text_editor(&vec.get(AreaIndex::Result as usize).unwrap().content),
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



async fn connect(config: OpenAi, name: String, prompt: String )
                 -> Result<(Client<OpenAIConfig>, ThreadObject, AssistantObject), AssistantError>{
    let client = openai_api::create_opeai_client(config);
    let (th, ass) = openai_api::setup_assistant(name, &client, prompt).await?;
    Ok((client, th, ass))
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


#[derive(Debug, PartialEq)]
enum Mark<'a> {
    Marker{text: &'a str},
    Content{text: &'a str},
}

fn split_code<'a>(source:&'a str, markers:&Vec<String>) -> Vec<Mark<'a>> {
    let mut curr_pos:usize = 0;
    let max = source.len();
    let mut result = Vec::new();

    for marker in markers {
        if let Some(pos) = source[curr_pos..max].find(marker.as_str()) {
            if 0 != pos {
                result.push(Mark::Content {text: &source[curr_pos..(curr_pos + pos)]});
                curr_pos += pos;
            }
            // Only marker exists from start.
            result.push(Mark::Marker{text: &source[curr_pos..(curr_pos + marker.len())]});
            curr_pos += marker.len();
        } else {
            // not marker found. This might be a error.
        }

    }
    if curr_pos < max {
        result.push(Mark::Content{text: &source[curr_pos..max]});
    }
    result
}


#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_split_mark_only() {
        let input = r#"```start
```"#.to_string();
        let markers = vec!["```start".to_string(), "```".to_string()];

        let res = split_code(&input, &markers);
        assert_eq!(res.len(), 3);
        assert_eq!(res.get(0), Some(&Mark::Marker{text:"```start"}));
        assert_eq!(res.get(1), Some(&Mark::Content{text:"\n"}));
        assert_eq!(res.get(2), Some(&Mark::Marker{text:"```"}));
    }
    #[test]
    fn test_split_mark_backquotes() {
        let input = r#"```start
asdf
```"#.to_string();
        let markers = vec!["```start```".to_string()];

        let res = split_code(&input, &markers);
        assert_eq!(res.len(), 1);
        assert_eq!(res.get(0), Some(&Mark::Content{text:"```start\nasdf\n```"}));
    }

    #[test]
    fn test_split_mark_and_content() {
        let input = r#"asdf
```start
hjklm
```
xyzw
"#.to_string();
        let markers = vec!["```start".to_string(), "```".to_string()];

        let res = split_code(&input, &markers);
        assert_eq!(res.len(), 5);
        assert_eq!(res.get(0), Some(&Mark::Content{text:"asdf\n"}));
        assert_eq!(res.get(1), Some(&Mark::Marker{text:"```start"}));
        assert_eq!(res.get(2), Some(&Mark::Content{text:"\nhjklm\n"}));
        assert_eq!(res.get(3), Some(&Mark::Marker{text:"```"}));
        assert_eq!(res.get(4), Some(&Mark::Content{text:"\nxyzw\n"}));

    }
}