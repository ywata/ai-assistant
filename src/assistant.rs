use std::{fs, io};
use std::path::{PathBuf};

use iced::widget::{self, column, container, image, row, text};
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

    #[clap(subcommand)]
    command: Commands,
}


#[derive(Clone, Debug, Subcommand)]
enum Commands {
    AskAi {
        #[clap(required = true, )]
        input: String,
        #[clap(required = true, )]
        prompt: String,
        //#[clap(required = true, )]
        output_dir: String,
        #[arg(long)]
        markers: Option<Vec<String>>
    },
}

impl Default for Cli {
    fn default() -> Self {
        Cli {yaml:"service.yaml".to_string(),
            key:"openai".to_string(),
            name:"ai assistant".to_string(),
            command: Commands::AskAi{
                input: "input.txt".to_string(),
                prompt: "prompt.txt".to_string(),
                output_dir: "output".to_string(),
                markers:None}
        }
    }
}

trait LlmInput {
    fn get_input(&self) -> io::Result<String>;
    fn get_prompt(&self) -> io::Result<String>;
    fn get_output_dir(&self, dir:Option<&str>) -> Option<String>;
}

impl LlmInput for Commands {
    fn get_input(&self) -> io::Result<String> {
        match self {
            Commands::AskAi{input, ..} => {
                fs::read_to_string(input)
            },
        }
    }
    fn get_prompt(&self) -> io::Result<String> {
        match self {
            Commands::AskAi{prompt,..} => {
                fs::read_to_string(prompt)
            },
        }
    }
    fn get_output_dir(&self, dir:Option<&str>) -> Option<String>{

        let p = match self {
            Commands::AskAi{output_dir, ..} => {
                output_dir.clone()
            },
        };
        let mut path = PathBuf::from(p);
        if let Some(child) = dir {
            let child_name = PathBuf::from(&child);
            path = path.join(child_name);
        } else {

        }

        path.to_str().map(|s|s.to_string())
    }
}


pub fn main() -> Result<(), AssistantError> {
    let args = Cli::parse();

    let config_content = fs::read_to_string(&args.yaml)?;
    let config: OpenAi = config::read_config(&args.key, &config_content)?;
    let prompt = &args.command.get_prompt()?;
    //let prompt = std::fs::read_to_string(&args.command.get_prompt()?);

    let settings = Settings::default();
    let updated_settings = Settings {
        flags: (args, config, prompt.clone()),
        ..settings
    };

    Model::run(updated_settings);

    Ok(())
}



#[derive(Debug, Clone)]
struct Model {
    env: Cli,
    client: Option<Client<OpenAIConfig>>,
    thread: Option<ThreadObject>,
    assistant: Option<AssistantObject>,
}


#[derive(Debug, Clone)]
enum Message {
    Connected(
        Result<(Client<OpenAIConfig>, ThreadObject, AssistantObject), AssistantError>),
}

#[derive(Error, Clone, Debug)]
pub enum AssistantError {
    #[error("file already exists for the directory")]
    FileExists(),
    #[error("API access failed")]
    AppAccessError,
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





async fn connect(config: OpenAi, name: String, prompt: String )
                 -> Result<(Client<OpenAIConfig>, ThreadObject, AssistantObject), AssistantError>{
    let client = openai_api::create_opeai_client(config);
    let (th, ass)
        = openai_api::setup_assistant(name, &client, prompt).await?;
    Ok((client, th, ass))
}



impl Application for Model {
    type Message = Message;
    type Theme = Theme;
    type Executor = iced::executor::Default;
    type Flags = (Cli, OpenAi, String);

    fn new(flags: (Cli, OpenAi, String)) -> (Model, Command<Message>) {
        let name = flags.0.name.clone();
        (
            Model {env: flags.0, client: None, thread: None, assistant: None},
            Command::perform(connect(flags.1, name, flags.2 ), Message::Connected),
        )
    }

    fn title(&self) -> String {
        "title".to_string()
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Connected(val) => {
                println!("{:?}", val);
                Command::none()
            },
        }
    }

    fn view(&self) -> Element<Message> {
        let content = "asdf";

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x()
            .center_y()
            .into()
    }
}

#[derive(Debug, Clone)]
enum Error {
    APIError,
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
