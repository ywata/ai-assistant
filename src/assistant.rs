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

    let settings = Settings::default();
    let updated_settings = Settings {
        flags: (args, config),
        ..settings
    };

    Ok(Model::run(updated_settings)?)

}





#[derive(Debug, Clone)]
enum Message {
    Connected(Result<(Client<OpenAIConfig>, ThreadObject, AssistantObject), AssistantError>),
    OpenPromptFile,
    OpenInputFile,
    PromptFileOpened(Result<(PathBuf, Arc<String>), Error>),
    InputFileOpened(Result<(PathBuf, Arc<String>), Error>),
    ActionPerformed(text_editor::Action),

}

#[derive(Debug)]
struct Model {
    env: Cli,
    client: Option<Client<OpenAIConfig>>,
    access: Option<(ThreadObject, AssistantObject)>,
    prompt: text_editor::Content,
    input: text_editor::Content,
    result: text_editor::Content,
    is_loading: bool,
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
    type Flags = (Cli, OpenAi);

    fn new(flags: (Cli, OpenAi)) -> (Model, Command<Message>) {
        let prompt_file = PathBuf::from(&flags.0.prompt_file);
        let input_file = PathBuf::from(&flags.0.input_file);
        (
            Model {env: flags.0, client: None, access: None,
                prompt: text_editor::Content::new(),
                input: text_editor::Content::new(),
                result: text_editor::Content::new(),
                is_loading: false,
            },
            Command::batch(vec![
                Command::perform(load_file(prompt_file), Message::PromptFileOpened),
                Command::perform(load_file(input_file), Message::InputFileOpened),
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
                    Err(err) => Command::none(),

                }
            },
            Message::OpenPromptFile =>{
                if self.is_loading {
                    Command::none()
                } else {
                    self.is_loading = true;
                    Command::perform(open_file(), Message::PromptFileOpened)
                }
            },
            Message::OpenInputFile =>{
                if self.is_loading {
                    Command::none()
                } else {
                    self.is_loading = true;
                    Command::perform(open_file(), Message::InputFileOpened)
                }
            },
            Message::PromptFileOpened(result) => {
                self.is_loading = false;
                if let Ok((path, contents)) = result {
                    self.prompt = text_editor::Content::with_text(&contents);
                }
                Command::none()
            },
            Message::InputFileOpened(result) => {
                self.is_loading = false;
                if let Ok((path, contents)) = result {
                    self.input = text_editor::Content::with_text(&contents);
                }
                Command::none()
            },
            Message::ActionPerformed(action) => {
                self.input.perform(action);
                Command::none()
            },
        }
    }

    fn view(&self) -> Element<Message> {
        row![
            column![
                text_editor(&self.prompt),
                text_editor(&self.input)
                .on_action(Message::ActionPerformed),
            ],
            text_editor(&self.result),
        ].into()
    }
}
async fn open_file() -> Result<(PathBuf, Arc<String>), Error> {
    let picked_file = rfd::AsyncFileDialog::new()
        .set_title("Open a text file...")
        .pick_file()
        .await
        .ok_or(Error::DialogClosed)?;

    load_file(picked_file.path().to_owned()).await
}

async fn load_file(path: PathBuf) -> Result<(PathBuf, Arc<String>), Error> {
    let contents = tokio::fs::read_to_string(&path)
        .await
        .map(Arc::new)
        .map_err(|error| Error::IoError(error.kind()))?;

    Ok((path, contents))
}



async fn connect(config: OpenAi, name: String, prompt: String )
                 -> Result<(Client<OpenAIConfig>, ThreadObject, AssistantObject), AssistantError>{
    let client = openai_api::create_opeai_client(config);
    let (th, ass)
        = openai_api::setup_assistant(name, &client, prompt).await?;
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
