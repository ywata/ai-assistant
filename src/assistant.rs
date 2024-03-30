use crate::response_content::get_content;
use crate::response_content::Mark;
use crate::scenario::Prompt;
use crate::scenario::Renderer;
use crate::scenario::Workflow;
use crate::scenario::{get_item, Input};
use crate::scenario::{parse_scenario, Item};
use log::warn;
use openai_api::ask;
use openai_api::{AiService, AssistantName, CClient};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Debug;

use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::{fs, io};

use regex::Regex;
use std::process::Output;

use iced::widget::{
    self, checkbox, column, horizontal_space, row, text_editor, Button, Column, Text,
};
use iced::{font, Alignment, Application, Command, Element, Settings, Theme};

use thiserror::Error;

use clap::{Parser, Subcommand};
use log::{debug, error, info};

use tokio::sync::Mutex;

use openai_api::{connect, Context, OpenAIApiError, OpenAi};

use crate::compile::compile;

use handlebars::Handlebars;

//use thiserror::Error;
mod compile;
mod config;
mod openai_api;
mod response_content;
mod scenario;

#[derive(Clone, Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// yaml file to store credentials
    #[arg(long)]
    config_file: String,
    #[arg(long)]
    config_key: String,
    #[arg(long)]
    prompt_file: String,
    #[arg(long)]
    prompt_name: String,
    #[arg(long)]
    prompt_tag: String,
    #[arg(long)]
    workflow_file: Option<String>,
    #[arg(long)]
    output_dir: String,
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Clone, Debug, Subcommand)]
enum Commands {
    AskAi {
        #[arg(long)]
        markers: Option<Vec<String>>,
    },
}

impl Default for Commands {
    fn default() -> Self {
        Commands::AskAi { markers: None }
    }
}

impl Cli {
    fn get_markers(&self) -> Result<Vec<Regex>, regex::Error> {
        let res = match &self.command {
            Commands::AskAi { markers: Some(m) } => {
                let v: Result<Vec<_>, _> = m.iter().map(|s| Regex::new(s)).collect();
                v
            }
            _ => Ok(vec![]),
        };
        res
    }
}

impl Default for Cli {
    fn default() -> Self {
        Cli {
            config_file: "service.yaml".to_string(),
            config_key: "openai".to_string(),
            prompt_file: "prompt.txt".to_string(),
            prompt_name: "".to_string(),
            prompt_tag: "".to_string(),
            workflow_file: None,
            output_dir: "output".to_string(),
            command: Commands::default(),
        }
    }
}

pub fn main() -> Result<(), AssistantError> {
    env_logger::init();
    let args = Cli::parse();
    debug!("args:{:?}", args);
    let config_content = fs::read_to_string(&args.config_file)?;
    let config: OpenAi = config::read_config(Some(&args.config_key), &config_content)?;
    let prompt_content = fs::read_to_string(&args.prompt_file)?;
    let prompt_hash: Box<HashMap<String, Box<Prompt>>> =
        config::read_config(None, &prompt_content)?;
    let _markers = args.get_markers()?;
    let wf = if let Some(ref file) = &args.workflow_file {
        let workflow_content = fs::read_to_string(file)?;
        crate::config::read_config(None, &workflow_content)?
    } else {
        Workflow::default()
    };

    if let Some((prompts, workflow)) = parse_scenario(*prompt_hash, wf) {
        let workflow = load_template(workflow).unwrap();
        debug!("{:?}", workflow);
        let client: Option<CClient> = config.create_client();
        let settings_default = Settings {
            flags: (args.clone(), config, prompts, workflow, client),
            ..Default::default()
        };

        Ok(Model::run(settings_default)?)
    } else {
        error!("parse_scenario failed");
        Err(AssistantError::AppAccessError)
    }
}

#[derive(Clone, Debug)]
enum Message {
    Connected(Result<Context, OpenAIApiError>),
    LoadInput {
        name: String,
        tag: String,
    },

    QueryAi {
        name: String,
        tag: String,
    },
    Answered {
        answer: Result<(String, String, String), (String, OpenAIApiError)>,
    },
    SaveConversation {
        outut_dir: String,
    },
    Toggled(String, usize, bool),
    FontLoaded(Result<(), font::Error>),
    DoNothing,
}

#[derive(Debug)]
struct EditArea {
    content: text_editor::Content,
    is_dirty: bool,
    is_loading: bool,
}

impl Default for EditArea {
    fn default() -> Self {
        EditArea {
            content: text_editor::Content::new(),
            is_dirty: false,
            is_loading: false,
        }
    }
}

#[derive(Debug, Copy, Clone)]
enum AreaIndex {
    Prompt = 0,
    Input = 1,
    Result = 2,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
enum Content {
    Text(String),
    Json(String),
    Fsharp(String),
}

impl Content {
    fn get_text(&self) -> String {
        match self {
            Content::Text(text) => text.clone(),
            Content::Json(text) => text.clone(),
            Content::Fsharp(text) => text.clone(),
        }
    }
}

type Tag = String;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum Talk {
    OriginalInput {
        name: AssistantName,
        tag: Tag,
        message: Content,
    },
    ToAi {
        name: AssistantName,
        tag: Tag,
        message: Content,
    },
    FromAi {
        name: AssistantName,
        tag: Tag,
        message: Content,
    },
    ProcessedResponse {
        name: AssistantName,
        tag: Tag,
        message: Content,
    },
}

impl Talk {
    fn get_message<'a>(&self) -> Content {
        let n = match self {
            Talk::OriginalInput { message, .. } => message,
            Talk::ToAi { message, .. } => message,
            Talk::FromAi { message, .. } => message,
            Talk::ProcessedResponse { message, .. } => message,
        };
        n.clone()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct Request {
    path: String,
    template: Option<String>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct Response {
    path: String,
    template: Option<String>,
}

type RenderingContext<'a> = (&'a Handlebars<'a>, &'a Vec<Talk>, &'a String, &'a Input);
impl<'a> Renderer<RenderingContext<'a>, String> for Request {
    fn render(&self, talks: RenderingContext) -> String {
        let (hb, t, s, i) = talks;
        let mut data = BTreeMap::new();
        data.insert(
            "prefix".to_string(),
            i.prefix.clone().unwrap_or("".to_string()),
        );
        data.insert("text".to_string(), i.text.clone());
        debug!("{:?}", &self.path);
        debug!("{:?}", &self.template);
        debug!("{:?}", &hb);
        hb.render(&self.path, &data).unwrap_or("failed".to_string())
    }
}

impl<'a> Renderer<RenderingContext<'a>, String> for Response {
    fn render(&self, talks: RenderingContext) -> String {
        let (hb, t, s, i) = talks;
        let mut data = BTreeMap::new();
        data.insert(
            "prefix".to_string(),
            i.prefix.clone().unwrap_or("".to_string()),
        );
        data.insert("text".to_string(), i.text.clone());
        debug!("{:?}", &self.path);
        debug!("{:?}", &self.template);
        debug!("{:?}", &hb);
        hb.render(&self.path, &data).unwrap_or("failed".to_string())
    }
}

fn load_template<'a>(
    workflow: Workflow<RenderingContext<'a>, String, Request, Response>,
) -> Result<Workflow<RenderingContext<'a>, String, Request, Response>, AssistantError> {
    let mut wf: Workflow<RenderingContext<'a>, String, Request, Response> = HashMap::new();
    for (name, hmap) in workflow {
        let mut new_hmap = HashMap::new();
        for (tag, item) in hmap {
            let mut req_file = File::open(item.request.path.clone())
                .map_err(|_| AssistantError::FileOpenFailed("req open".to_string()))?;
            let mut req_template = String::new();
            req_file
                .read_to_string(&mut req_template)
                .map_err(|_| AssistantError::FileOpenFailed("req open".to_string()))?;

            let mut rsp_file = File::open(item.response.path.clone())
                .map_err(|_| AssistantError::FileOpenFailed("req read".to_string()))?;
            let mut rsp_template = String::new();
            rsp_file
                .read_to_string(&mut rsp_template)
                .map_err(|_| AssistantError::FileOpenFailed("req read".to_string()))?;

            let new_item = Item {
                request: Box::new(Request {
                    path: item.request.path.clone(),
                    template: Some(req_template.clone()),
                }),
                response: Box::new(Response {
                    path: item.response.path.clone(),
                    template: Some(rsp_template.clone()),
                }),
                ..item
            };
            new_hmap.insert(tag, new_item);
        }
        wf.insert(name.clone(), new_hmap);
    }
    Ok(wf)
}

fn register_template<'a>(
    handlebars: &mut Handlebars,
    workflow: &Workflow<RenderingContext<'a>, String, Request, Response>,
) -> () {
    for (_, hm) in workflow {
        for (_, item) in hm {
            let item_cloned = item.clone();
            item_cloned
                .request
                .template
                .map(|t| handlebars.register_template_string(&item.request.path, &t));
            item_cloned
                .response
                .template
                .map(|t| handlebars.register_template_string(&item.response.path, &t));
        }
    }
}

struct Model<'a> {
    env: Cli,
    prompts: HashMap<String, Box<Prompt>>,
    context: Option<Arc<Mutex<Context>>>,
    conversations: Vec<Talk>,
    // edit_area contaiins current view of conversation,
    // Prompt is set up on Thread creation time and it is not changed.
    // Input edit_area will be used for querying to AI.
    // Result edit_area will be used for displaying the result of AI.
    edit_areas: Vec<EditArea>,
    current: (String, String),
    workflow: Workflow<RenderingContext<'a>, String, Request, Response>,
    // handlebars is setup from workflow on new(). It stores  path -> template mapping.
    handlebars: Handlebars<'a>,
}

impl<'a> Model<'a> {
    fn push_talk(&mut self, talk: Talk) {
        self.conversations.push(talk);
    }
    fn get_talk(&self, cnstr: impl Fn(AssistantName, Content) -> Talk) -> Option<Talk> {
        for talk in self.conversations.iter().rev() {
            let talk_ = match talk {
                Talk::OriginalInput { name, message, .. } => {
                    cnstr(name.to_string().clone(), message.clone())
                }
                Talk::ToAi { name, message, .. } => {
                    cnstr(name.to_string().clone(), message.clone())
                }
                Talk::ProcessedResponse { name, message, .. } => {
                    cnstr(name.to_string().clone(), message.clone())
                }
                Talk::FromAi { name, message, .. } => {
                    cnstr(name.to_string().clone(), message.clone())
                }
            };
            if *talk == talk_ {
                return Some(talk.clone());
            }
        }
        None
    }
}

#[derive(Error, Clone, Debug)]
pub enum AssistantError {
    #[error("file already exists for the directory")]
    FileExists(),

    #[error("file open error")]
    FileOpenFailed(String),

    #[error("IO error")]
    IoError,

    #[error("API call error")]
    APIError,

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
        dbg!(&error);
        match error.kind() {
            io::ErrorKind::AlreadyExists => AssistantError::FileExists(),
            _ => AssistantError::IoError,
        }
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

async fn save_and_compile(output_path: PathBuf, code: String) -> Result<Output, AssistantError> {
    tokio::fs::write(&output_path, code).await?;
    let res = compile(output_path).await?;

    Ok(res)
}

fn set_editor_contents(area: &mut Vec<EditArea>, idx: AreaIndex, text: &str) {
    let default = EditArea::default();
    area[idx as usize] = EditArea {
        content: text_editor::Content::with_text(text),
        ..default
    };
}

impl<'a> Application for Model<'a> {
    type Message = Message;
    type Theme = Theme;
    type Executor = iced::executor::Default;
    type Flags = (
        Cli,
        OpenAi,
        HashMap<String, Box<Prompt>>,
        Workflow<RenderingContext<'a>, String, Request, Response>,
        Option<CClient>,
    );

    fn new(flags: <Model<'a> as iced::Application>::Flags) -> (Model<'a>, Command<Message>) {
        let name = flags.0.prompt_name.clone();
        let tag = flags.0.prompt_tag.clone();
        let workflow = flags.3;
        let prompt = EditArea::default();
        let input = EditArea::default();
        let result = EditArea::default();

        // As OpenAICOnfig and AzureConfig cannot co-exists in a generic way,
        // we need to use conditional compilation.
        // It might be good to place this in main() but it introduces a lot of
        // conditional compilation.
        #[cfg(not(azure_ai))]
        //let client = create_opeai_client(&flags.1);
        let client = flags.1.create_client();
        #[cfg(azure_ai)]
        let client = create_opeai_client(&flags.1);
        let assistant_names = flags.2.keys().cloned().collect::<Vec<_>>();
        // Initialize EditArea with loaded input.
        let mut commands: Vec<Command<Message>> = vec![
            // To triger initial sreen
            Command::perform(load_input(name.clone(), tag.clone()), |(name, tag)| {
                Message::LoadInput { name, tag }
            }),
            Command::perform(
                connect(
                    flags.1.clone(),
                    client.unwrap(),
                    assistant_names,
                    flags.2.clone(),
                ),
                Message::Connected,
            ),
        ];
        let mut handlebars = Handlebars::new();
        register_template(&mut handlebars, &workflow);
        #[cfg(feature = "load_font")]
        commands.push(
            font::load(include_bytes!("../fonts/UDEVGothic-Regular.ttf").as_slice())
                .map(Message::FontLoaded),
        );

        (
            Model {
                env: flags.0.clone(),
                prompts: flags.2.clone(),
                context: None,
                edit_areas: vec![prompt, input, result],
                current: (name.clone(), tag.clone()),
                workflow: workflow,
                conversations: vec![],
                handlebars: handlebars,
            },
            Command::<Message>::batch(commands),
        )
    }

    fn title(&self) -> String {
        "title".to_string()
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        info!("Message:{:?}", message);
        info!("current: {:?}", &self.current);
        let mut next_current: Option<(AssistantName, String)> = None;
        let command = match message {
            Message::Connected(Ok(ctx)) => {
                info!("Connected: {:?}", &ctx);
                self.context = Some(Arc::new(Mutex::new(ctx)));
                //next_current = Some((self.current.0.clone(), self.current.1.clone()));
                Command::none()
            }
            Message::Connected(Err(_)) => Command::none(),

            Message::LoadInput { name, tag } => {
                let item = get_item(&self.workflow, &name, &tag);
                let input: Option<(&String, Option<&Input>)> = self
                    .prompts
                    .get(&name)
                    .map(|p| (&p.instruction, p.inputs.get(&tag)));

                if let (Some(item), Some((instruction, Some(input)))) = (item, input) {
                    debug!("{:?}", item);
                    debug!("{:?}", input);
                    let input_displayed = item.request.render((
                        &self.handlebars,
                        &self.conversations,
                        &instruction,
                        &input,
                    ));
                    self.edit_areas[AreaIndex::Input as usize].content =
                        text_editor::Content::with_text(&input_displayed);
                    self.push_talk(Talk::ToAi {
                        name: name.clone(),
                        tag: tag.clone(),
                        message: Content::Text(input_displayed),
                    });

                    self.current.0 = name;
                    self.current.1 = tag;
                }

                Command::none()
            }

            Message::QueryAi { name, tag } => {
                if let Some(context) = self.context.clone() {
                    let pass_context = context.clone();
                    let pass_name = name.clone();
                    let pass_tag = tag.clone();
                    let input = self.edit_areas[AreaIndex::Input as usize].content.text();
                    self.push_talk(Talk::ToAi {
                        name,
                        tag,
                        message: Content::Text(input.clone()),
                    });

                    Command::perform(
                        ask(pass_context, pass_name, pass_tag, input),
                        move |answer| Message::Answered { answer },
                    )
                } else {
                    Command::none()
                }
            }
            Message::Answered { answer, .. } => {
                let command = Command::none();

                match answer {
                    Ok((name, tag, text)) => {
                        self.push_talk(Talk::FromAi {
                            name: name.clone(),
                            tag: tag.clone(),
                            message: Content::Text(text.clone()),
                        });
                    }
                    _ => error!("FAILED"),
                }
                command
            }

            Message::SaveConversation { outut_dir } => {
                let convs = self.conversations.clone();
                let output_dir_path = PathBuf::from(outut_dir);
                let _ = serde_yaml::to_string(&convs).map(|s| {
                    let output_path = output_dir_path.join("conversation.yaml");
                    fs::write(output_path, s)
                });

                Command::none()
            }
            Message::Toggled(_string, _usize, _bool) => Command::none(),
            Message::FontLoaded(_) => Command::none(),
            Message::DoNothing => Command::none(),
        };

        command
    }

    fn view(&self) -> Element<Message> {
        let vec = &self.edit_areas;
        debug!("view(): {:?}", vec);

        column![
            row![
                row(list_inputs(&self.prompts)
                    .into_iter()
                    .map(|(name, tag)| button(&name, &tag)
                        .on_press(Message::LoadInput { name, tag })
                        .into())),
                row![
                    horizontal_space(),
                    button("Ask AI", "").on_press(Message::QueryAi {
                        name: self.current.0.clone(),
                        tag: self.current.1.clone(),
                    }),
                ]
                .align_items(Alignment::End)
                .width(iced::Length::Fill),
            ],
            row![
                column![text_editor(
                    &vec.get(AreaIndex::Input as usize).unwrap().content
                ),],
                column![text_editor(
                    &vec.get(AreaIndex::Result as usize).unwrap().content
                )],
            ],
        ]
        .into()
    }
}

async fn load_input(p0: String, p1: String) -> (String, String) {
    (p0, p1)
}

fn extract_content(_model: &mut Model, contents: Vec<Mark>) -> Option<Content> {
    let json = String::from("json");
    let fsharp = String::from("fsharp");
    if let Some(Mark::Content {
        text,
        lang: Some(matcher),
    }) = get_content(contents.clone())
    {
        if matcher == json {
            let response = serde_json::from_str::<HashMap<String, Vec<String>>>(&text);
            if let Ok(_resp) = response {
                return Some(Content::Json(text));
            }
        } else if matcher == fsharp {
            return Some(Content::Fsharp(text));
        } else {
            warn!("Unknown matcher: {}", &text);
            return Some(Content::Text(text));
        }
        Some(Content::Text(text))
    } else {
        warn!("No splitted contents: {:?}", &contents);
        None
    }
}

fn list_inputs(prompts: &HashMap<String, Box<Prompt>>) -> Vec<(String, String)> {
    prompts
        .into_iter()
        .map(|(n, p)| {
            p.clone()
                .inputs
                .into_iter()
                .map(|(t, i)| (n.clone(), t.clone()))
        })
        .flatten()
        .collect()
}

impl From<reqwest::Error> for AssistantError {
    fn from(error: reqwest::Error) -> AssistantError {
        dbg!(error);

        AssistantError::APIError
    }
}

fn button<'a>(text: &str, tag: &str) -> widget::Button<'a, Message> {
    let title = text.to_string() + ":" + tag;
    Button::new(Text::new(title))
}

fn pick_selected(resp: &HashMap<String, Vec<(String, bool)>>) -> Vec<String> {
    let mut vec = Vec::new();
    for (_k, v) in resp.iter() {
        for (s, b) in v.iter() {
            if *b {
                vec.push(s.clone());
            }
        }
    }
    vec
}

fn to_checkboxes<'a>(
    resp: HashMap<String, Vec<(String, bool)>>,
) -> iced::widget::Column<'a, Message> {
    let mut col = Column::new();
    let mut keys: Vec<_> = resp.keys().cloned().collect();
    keys.sort();
    for k in keys {
        let v = &resp[&k.clone()];
        col = col.push(Text::new(k.clone()));

        for (i, (msg, b)) in v.iter().enumerate() {
            let copy_key = k.clone();

            let cb = checkbox(msg.clone(), *b)
                .on_toggle(move |b| Message::Toggled(copy_key.clone(), i, b));
            col = col.push(cb)
        }
    }
    col
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::config::read_config;
    #[test]
    fn test_convert_prompt() {
        let prompt_content = r#"
        instruction: |
          asdf
          asdf
        inputs:
          abc:
            text: |
             xyz
        "#
        .to_string();
        let prompt: Prompt = read_config(None, &prompt_content).unwrap();
        assert_eq!(prompt.instruction, "asdf\nasdf\n".to_string());
    }

    #[derive(Clone, Debug, Default, Deserialize)]
    struct T {
        name: String,
    }
    impl Renderer<&Vec<Talk>, String> for T {
        fn render(&self, _talks: &Vec<Talk>) -> String {
            "".to_string()
        }
    }
    #[derive(Clone, Debug, Deserialize)]
    enum S {
        S1,
        S2(String),
        S3 { name: String },
    }
    impl Renderer<&Vec<Talk>, String> for S {
        fn render(&self, _talks: &Vec<Talk>) -> String {
            "".to_string()
        }
    }
    impl Default for S {
        fn default() -> Self {
            S::S1
        }
    }
    #[test]
    fn test_convert_workflow() {
        let workflow_content = r#"
          king:
            k11:
              next: !Stop
              request:
                name: "asdf"
              response:
                name: ";asdf"
          queen:
            q11:
              next: !Next
                name: king
                tag: k11
              request:
                name: request
              response:
                name: response
        "#
        .to_string();
        let wf: Workflow<&Vec<Talk>, String, T, T> = read_config(None, &workflow_content).unwrap();

        assert_eq!(wf.len(), 2);
        assert_eq!(wf.get("king").unwrap().len(), 1);
    }
    #[test]
    fn test_parse_scenario() {
        let prompt_str = r#"
king:
  instruction: |
    This is instruction for king
  inputs:
    k1:
      prefix: k1_prefix
      text: input for king_k1
    k2:
      text: input for king_k2
queen:
  instruction: Queen's instruction
  inputs:
    q1:
      prefix: q1_prefix
      text: input for queen_q1
    q2:
      text: input for queen_q2
        "#;
        let prompts: HashMap<String, Box<Prompt>> = read_config(None, &prompt_str).unwrap();
        let workflow_str = r#"
  king:
    k1:
      next: !Stop
      request:
        name: asdf
      response:
        name: sdfg
    k2:
      next: !Stop
      request:
        name: asdf
      response:
        name: sdfa
  queen:
    q1:
      next: !Stop
      request:
        name: asdf
      response:
        name: adsf
    q2:
      next: !Stop
      request:
        name: adsf
      response:
        name: asdf
        "#;
        let wf: Workflow<&Vec<Talk>, String, T, T> = read_config(None, &workflow_str).unwrap();
        let parsed = parse_scenario(prompts, wf);
        assert_eq!(parsed.is_some(), true);
    }
}
