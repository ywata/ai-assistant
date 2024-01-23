use std::{fs, io};
use std::path::{PathBuf};
use std::sync::Arc;

use iced::widget::{self,
                   column, container, image, row,
                   text, text_editor};
use iced::{
    Application, Command, Element, Length, Settings, Theme,
};
use async_openai::{
    config::OpenAIConfig,
    Client,
};
use async_openai::types::{AssistantObject, ThreadObject};
use thiserror::Error;

use clap::{Parser, Subcommand};
use serde::{Deserialize};
use openai_api::{OpenAi, Saver, OpenAIApiError};
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
    #[arg(long)]
    markers: Option<Vec<String>>,

    #[clap(subcommand)]
    command: Commands,
}


#[derive(Clone, Debug, Subcommand)]
enum Commands {
    AskAi,

}


impl Default for Cli {
    fn default() -> Self {
        Cli {yaml:"service.yaml".to_string(),
            key:"openai".to_string(),
            name:"ai assistant".to_string(),
            input_file: "input.txt".to_string(),
            prompt_file: "prompt.txt".to_string(),
            output_dir: "output".to_string(),
            markers:None,
            command: Commands::AskAi
        }
    }
}



pub fn main() -> Result<(), AssistantError> {
    let args = Cli::parse();

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
        let edit_area = EditArea{
            path: Some(pbuf),
            ..default
        };
        edit_area
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
                        println!("Connected: {:?}", (&c, &t));
                        self.access = Some((t, a));
                        self.client = Some(c);
                        Command::none()
                    },
                    Err(err) => Command::none(),

                }
            },
            Message::OpenFile(idx) => {

                Command::none()
            },
            Message::FileOpened(result) => {
                if let Ok((idx, (path, contents))) = result {
                    let content = text_editor::Content::with_text(&contents);
                    let default = EditArea::default();
                    self.edit_areas[idx as usize] = EditArea{
                        path: Some(path),
                        content: content,
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
                        let content = text_editor::Content::with_text(&text);
                        let default = EditArea::default();
                        self.edit_areas[AreaIndex::Result as usize] = EditArea{
                            content: content,
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
/*async fn open_file<T: Copy>(idx: T) -> Result<(T, (PathBuf, Arc<String>)), (T, Error)> {
    let picked_file = rfd::AsyncFileDialog::new()
        .set_title("Open a text file...")
        .pick_file()
        .await
        .ok_or((idx, Error::DialogClosed))?;

    load_file(idx, picked_file.path().to_owned()).await
}*/

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
