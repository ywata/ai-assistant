use log::warn;
use openai_api::ask;
use openai_api::{AiService, AssistantName, CClient};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Debug;
use std::ops::Deref;
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
use log::{debug, error, info, trace};

use tokio::sync::Mutex;

use openai_api::{connect, Context, OpenAIApiError, OpenAi};

use crate::compile::compile;

use openai_api::scenario::{parse_defined_key, parse_scenario, Directive, Prompt, Workflow};

//use thiserror::Error;
mod compile;
pub mod config;

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
    prompt_key: String,
    #[arg(long)]
    workflow_file: Option<String>,
    #[arg(long)]
    output_dir: String,
    #[arg(long)]
    auto: Option<usize>,
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
            prompt_key: "".to_string(),
            workflow_file: None,
            output_dir: "output".to_string(),
            auto: None,
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
    let prompt_hash: HashMap<String, Prompt> = config::read_config(None, &prompt_content)?;
    let _markers = args.get_markers()?;
    let wf = if let Some(ref file) = &args.workflow_file {
        let workflow_content = fs::read_to_string(file)?;
        config::read_config(None, &workflow_content)?
    } else {
        Workflow::default()
    };
    let _given_key =
        parse_defined_key(&prompt_hash, &args.prompt_key).ok_or(AssistantError::AppAccessError)?;

    if let Some((prompts, workflow)) = parse_scenario(prompt_hash, wf) {
        let client: Option<CClient> = config.create_client();
        let settings_default = Settings {
            flags: (args.clone(), config, prompts, workflow, args.auto, client),
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

    NextWorkflow {
        auto: bool,
    },

    ActionPerformed(AreaIndex, text_editor::Action),

    QueryAi {
        name: String,
        tag: String,
        auto: bool,
    },
    Answered {
        answer: Result<(String, String), (String, OpenAIApiError)>,
        auto: bool,
    },
    Compiled(Result<Output, AssistantError>),

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

impl Model {
    fn set_proposal(&mut self, content: Content) {
        self.proposal = match content {
            Content::Json(prop) => {
                let prop = serde_json::from_str::<HashMap<String, Vec<String>>>(&prop)
                    .map(|hmap| {
                        hmap.iter()
                            .map(|(k, v)| {
                                let mut vec = Vec::new();
                                for i in v {
                                    vec.push((i.clone(), false));
                                }
                                (k.clone(), vec)
                            })
                            .collect()
                    })
                    .ok();
                prop
            }
            _ => None,
        };
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum Talk {
    InputShown {
        name: AssistantName,
        message: Content,
    },
    ToAi {
        name: AssistantName,
        message: Content,
    },
    FromAi {
        name: AssistantName,
        message: Content,
    },
    ResponseShown {
        name: AssistantName,
        message: Content,
    },
}

impl Talk {
    fn get_message<'a>(&self) -> Content {
        let n = match self {
            Talk::InputShown { message, .. } => message,
            Talk::ToAi { message, .. } => message,
            Talk::FromAi { message, .. } => message,
            Talk::ResponseShown { message, .. } => message,
        };
        n.clone()
    }
    fn get_name(&self) -> AssistantName {
        let n = match self {
            Talk::InputShown { name, .. } => name,
            Talk::ToAi { name, .. } => name,
            Talk::FromAi { name, .. } => name,
            Talk::ResponseShown { name, .. } => name,
        };
        n.clone()
    }
}

#[derive(Debug)]
struct Model {
    env: Cli,
    prompts: HashMap<String, openai_api::scenario::Prompt>,
    context: Option<Arc<Mutex<Context>>>,
    conversations: Vec<Talk>,
    // edit_area contaiins current view of conversation,
    // Prompt is set up on Thread creation time and it is not changed.
    // Input edit_area will be used for querying to AI.
    // Result edit_area will be used for displaying the result of AI.
    edit_areas: Vec<EditArea>,
    current: (String, String),
    workflow: Workflow,
    auto: Option<usize>,
    proposal: Option<HashMap<String, Vec<(String, bool)>>>,
}

impl Model {
    fn dec_auto(&mut self) {
        if let Some(auto) = self.auto {
            if auto > 0 {
                info!("dec_auto(): {:?}", &self.auto);
                self.auto = Some(auto - 1);
            }
        }
    }
    fn auto_enabled(&self) -> bool {
        if let Some(auto) = self.auto {
            if auto > 0 {
                return true;
            }
        }
        false
    }
    fn put_talk(&mut self, talk: Talk) {
        self.conversations.push(talk);
    }
    fn get_talk(&self, cnstr: impl Fn(AssistantName, Content) -> Talk) -> Option<Talk> {
        for talk in self.conversations.iter().rev() {
            let talk_ = match talk {
                Talk::InputShown { name, message } => {
                    cnstr(name.to_string().clone(), message.clone())
                }
                Talk::ToAi { name, message } => cnstr(name.to_string().clone(), message.clone()),
                Talk::ResponseShown { name, message } => {
                    cnstr(name.to_string().clone(), message.clone())
                }
                Talk::FromAi { name, message } => cnstr(name.to_string().clone(), message.clone()),
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

#[derive(Debug, Clone)]
struct LoadedData {
    prompt: String,
    prefix: Option<String>,
    input: String,
}
fn load_data_from_prompt(prompt: Prompt, tag: &str) -> Option<LoadedData> {
    debug!("load_data_from_prompt(): prompt:{:?} tag:{}", &prompt, &tag);

    prompt
        .inputs
        .iter()
        .find(|i| i.tag == tag)
        .map(|i| LoadedData {
            prompt: prompt.instruction.clone(),
            prefix: i.prefix.clone(),
            input: i.text.clone(),
        })
}

fn load_content(model: &Model, tag: &str) -> Option<LoadedData> {
    let prompt = model.prompts.get(&model.current.0).unwrap().clone();
    let mut text = model
        .get_talk(|name, message| Talk::InputShown { name, message })
        .map(|t| t.get_message().get_text())
        .unwrap_or("".to_string());
    if let Some(selected) = model.proposal.clone().map(|p| pick_selected(&p)) {
        info!("load_content(): selected:{:?}", &selected);
        text = selected.join("\n");
    }

    debug!("load_content(): prompt:{:?} tag:{}", &prompt, &tag);

    prompt
        .inputs
        .iter()
        .find(|i| i.tag == tag)
        .map(|i| LoadedData {
            prompt: prompt.instruction.clone(),
            prefix: i.prefix.clone(),
            input: text.to_string(),
        })
}

fn get_content(contents: Vec<Mark>) -> Option<Mark> {
    let mut res = None;
    for c in contents {
        if let Mark::Content { .. } = c {
            res = Some(c);
            break;
        }
    }
    res
}

fn set_editor_contents(area: &mut Vec<EditArea>, idx: AreaIndex, text: &str) {
    let default = EditArea::default();
    area[idx as usize] = EditArea {
        content: text_editor::Content::with_text(text),
        ..default
    };
}

impl Application for Model {
    type Message = Message;
    type Theme = Theme;
    type Executor = iced::executor::Default;
    type Flags = (
        Cli,
        OpenAi,
        HashMap<String, openai_api::scenario::Prompt>,
        Workflow,
        Option<usize>,
        Option<CClient>,
    );

    fn new(flags: <Model as iced::Application>::Flags) -> (Model, Command<Message>) {
        let name = flags.0.prompt_key.clone();
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
        let first_prompt = flags.2.get(&name).unwrap().clone();
        let assistant_names = flags.2.keys().cloned().collect::<Vec<_>>();
        let tag = first_prompt.inputs.first().unwrap().tag.clone();
        let loaded = load_data_from_prompt(first_prompt, &tag);
        // Initialize EditArea with loaded input.
        let mut prefixed_text = String::from("");
        if let Some(i) = loaded {
            prefixed_text = i.prefix.unwrap_or_default() + "\n" + &i.input;
        }
        let commands: Vec<Command<Message>> = vec![
            Command::perform(
                load_initial_input(name.clone(), tag.clone()),
                |(name, tag)| Message::LoadInput { name, tag },
            ),
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
                workflow: flags.3,
                auto: flags.0.auto,
                conversations: vec![],
                proposal: None,
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

            Message::NextWorkflow { auto } => {
                let current = self.current.clone();
                info!("NextWorkflow:current {:?}", current.clone());
                let wf = &self.workflow;

                let mut do_next = false;

                match wf.get_directive(&current.0, &current.1) {
                    Directive::KeepAsIs => (),
                    Directive::Stop => (),

                    Directive::JumpTo {
                        name: next_name,
                        tag: next_tag,
                    } => {
                        info!("JumpTo: name:{}, tag:{}", next_name, next_tag);
                        let loaded = load_data_from_prompt(
                            self.prompts.get(&next_name).unwrap().clone(),
                            &next_tag,
                        );
                        if let Some(_) = loaded {
                            do_next = true;
                            next_current = Some((next_name.clone(), next_tag.clone()));
                        }
                    }
                    Directive::PassResultTo {
                        name: next_name,
                        tag: next_tag,
                        ..
                    } => {
                        info!("PassResultTo: name:{}, tag:{}", next_name, next_tag);

                        if let Some(loaded) = load_content(&self, &next_tag) {
                            info!("PassResultTo: loaded:{:?}", &loaded);
                            let input = loaded.input.clone();
                            let prefix = loaded.prefix.clone();
                            let prefixed_text = prefix.unwrap_or_default() + "\n" + &input;

                            self.put_talk(Talk::InputShown {
                                name: next_name.clone(),
                                message: Content::Text(prefixed_text.clone()),
                            });
                            next_current = Some((next_name, next_tag));
                            do_next = true;
                        }
                    }
                };
                Command::none()
            }

            Message::LoadInput { name, tag } => {
                // A bit too early, but let's do it for now.
                next_current = Some((name.clone(), tag.clone()));

                let prompt = self.prompts.get(&name).unwrap().clone();
                //Command::perform(load_input(prompt, tag), Message::InputLoaded)
                let result = load_data_from_prompt(prompt, &tag)
                    .map(|i| {
                        let input = i.input.clone();
                        let prefix = i.prefix.clone();
                        let prefixed_text = prefix.unwrap_or_default() + "\n" + &input;
                        self.put_talk(Talk::InputShown {
                            name,
                            message: Content::Text(prefixed_text),
                        });
                    })
                    .ok_or(());
                Command::none()
            }
            Message::ActionPerformed(idx, action) => {
                self.edit_areas[idx as usize].content.perform(action);
                Command::none()
            }
            Message::QueryAi { name, auto, .. } => {
                if let Some(context) = self.context.clone() {
                    let pass_context = context.clone();
                    let pass_name = name.clone();
                    let input = self.edit_areas[AreaIndex::Input as usize].content.text();
                    self.put_talk(Talk::ToAi {
                        name,
                        message: Content::Text(input.clone()),
                    });

                    Command::perform(ask(pass_context, pass_name, input), move |answer| {
                        Message::Answered { answer, auto }
                    })
                } else {
                    Command::none()
                }
            }
            Message::Answered { answer, auto } => {
                self.dec_auto();
                self.proposal = None;

                let command = Command::none();

                match answer {
                    Ok((name, text)) => {
                        next_current = Some((self.current.0.clone(), self.current.1.clone()));
                        self.put_talk(Talk::FromAi {
                            name: name.clone(),
                            message: Content::Text(text.clone()),
                        });

                        let opt_markers = self.env.get_markers();
                        let mut resp_content = Content::Text(text.clone());
                        if let Ok(markers) = opt_markers {
                            let contents = split_code(&text, &markers.clone()).clone();
                            resp_content =
                                extract_content(self, contents).unwrap_or(Content::Text(text));
                        } else {
                            trace!("No: markers");
                            resp_content = Content::Text(text);
                        }
                        self.set_proposal(resp_content.clone());
                        self.put_talk(Talk::ResponseShown {
                            name,
                            message: resp_content,
                        })
                    }
                    _ => error!("FAILED"),
                }

                if auto {
                    self.update(Message::NextWorkflow {
                        auto: self.auto_enabled(),
                    })
                } else {
                    command
                }
            }
            Message::Compiled(msg) => {
                debug!("{:?}", msg);
                Command::none()
            }
            Message::Toggled(message, i, checked) => {
                info!("Toggled: message:{}, checked:{}", message, checked);

                if let Some(vec) = &mut self.proposal {
                    if let Some(v) = vec.get_mut(&message) {
                        v[i].1 = checked;
                    }
                }

                Command::none()
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
            Message::FontLoaded(_) => Command::none(),
            Message::DoNothing => Command::none(),
        };

        if let Some((name, tag)) = next_current {
            info!("Updated to: {} {}", &name, &tag);
            self.current = (name, tag);
            debug!("{:?}", &self.conversations);

            let input = self
                .get_talk(|name, message| Talk::InputShown { name, message })
                .map(|t| t.get_message().get_text())
                .unwrap_or("".to_string());
            info!("input: {:?}", &input);
            set_editor_contents(&mut self.edit_areas, AreaIndex::Input, &input);
            let answer = self
                .get_talk(|name, message| Talk::ResponseShown { name, message })
                .map(|t| t.get_message().get_text())
                .unwrap_or("junk".to_string());
            info!("answer: {:?}", &answer);
            set_editor_contents(&mut self.edit_areas, AreaIndex::Result, &answer);
        }

        command
    }

    fn view(&self) -> Element<Message> {
        let vec = &self.edit_areas;
        debug!("view(): {:?}", vec);

        let response: Element<Message> = self
            .proposal
            .clone()
            .map(|p| to_checkboxes(p).into())
            .unwrap_or(text_editor(&vec.get(AreaIndex::Result as usize).unwrap().content).into());

        column![
            row![
                row(list_inputs(&self.prompts)
                    .into_iter()
                    .map(|(name, tag)| button(&name, &tag)
                        .on_press(Message::LoadInput { name, tag })
                        .into())),
                row![
                    horizontal_space(),
                    button("Next", "").on_press(Message::NextWorkflow {
                        auto: self.auto_enabled()
                    }),
                    button("Ask AI", "").on_press(Message::QueryAi {
                        name: self.current.0.clone(),
                        tag: self.current.1.clone(),
                        auto: false,
                    }),
                    button("auto", "").on_press(Message::QueryAi {
                        name: self.current.0.clone(),
                        tag: self.current.1.clone(),
                        auto: true,
                    }),
                    button("save", "").on_press(Message::SaveConversation {
                        outut_dir: self.env.output_dir.clone(),
                    }),
                ]
                .align_items(Alignment::End)
                .width(iced::Length::Fill),
            ],
            row![
                column![
                    text_editor(&vec.get(AreaIndex::Input as usize).unwrap().content)
                        .on_action(|action| Message::ActionPerformed(AreaIndex::Input, action)),
                ],
                column![response,],
            ],
        ]
        .into()
    }
}

async fn load_initial_input(p0: String, p1: String) -> (String, String) {
    (p0, p1)
}

fn extract_content(model: &mut Model, contents: Vec<Mark>) -> Option<Content> {
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

fn list_inputs(prompts: &HashMap<String, Prompt>) -> Vec<(String, String)> {
    let mut items = Vec::new();
    for k in prompts.keys() {
        for i in prompts.get(k).unwrap().inputs.iter() {
            items.push((k.clone(), i.tag.clone()));
        }
    }
    items
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

#[derive(Clone, Debug, PartialEq)]
enum Mark {
    Marker { text: String, lang: Option<String> },
    Content { text: String, lang: Option<String> },
}

fn split_code(source: &str, markers: &Vec<regex::Regex>) -> Vec<Mark> {
    let mut curr_pos: usize = 0; // index to source
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
                result.push(Mark::Content {
                    text: String::from(&source[curr_pos..(curr_pos + pos)]),
                    lang: lang.clone(),
                });
                curr_pos += pos;
            }
            lang = matched.get(1).map(|m| {
                String::from(
                    &source[curr_pos + m.range().start - pos..(curr_pos + m.range().end - pos)],
                )
            });
            let r = all_matched.range();
            let len = r.end - r.start;

            result.push(Mark::Marker {
                text: String::from(&source[curr_pos..curr_pos + len]),
                lang: lang.clone(),
            });
            curr_pos += len;
        } else {
            // not marker found. This might be a error.
        }
    }
    if curr_pos < max {
        result.push(Mark::Content {
            text: String::from(&source[curr_pos..max]),
            lang: lang.clone(),
        });
    }

    result
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::config::read_config;
    #[test]
    fn test_split_mark_only() {
        let input = r#"```start
```"#
            .to_string();
        let markers = vec!["```(start)".to_string(), "```".to_string()];
        let mut cli = Cli::default();
        cli = Cli {
            command: Commands::AskAi {
                markers: Some(markers),
            },
            ..cli
        };
        let rex_markers = cli.get_markers();

        let res = split_code(&input, &rex_markers.unwrap());
        assert_eq!(res.len(), 3);
        assert_eq!(
            res.first(),
            Some(&Mark::Marker {
                text: "```start".to_string(),
                lang: Some("start".to_string())
            })
        );
        assert_eq!(
            res.get(1),
            Some(&Mark::Content {
                text: "\n".to_string(),
                lang: Some("start".to_string())
            })
        );
        assert_eq!(
            res.get(2),
            Some(&Mark::Marker {
                text: "```".to_string(),
                lang: None
            })
        );
    }
    #[test]
    fn test_split_mark_backquotes() {
        let input = r#"```start
asdf
```"#
            .to_string();
        let markers = vec!["```start```".to_string()];
        let mut cli = Cli::default();
        cli = Cli {
            command: Commands::AskAi {
                markers: Some(markers),
            },
            ..cli
        };
        let rex_markers = cli.get_markers();
        let res = split_code(&input, &rex_markers.unwrap());
        assert_eq!(res.len(), 1);
        assert_eq!(
            res.first(),
            Some(&Mark::Content {
                text: "```start\nasdf\n```".to_string(),
                lang: None
            })
        );
    }

    #[test]
    fn test_split_mark_and_content() {
        let input = r#"asdf
```start
hjklm
```
xyzw
"#
        .to_string();
        let markers = vec!["```(start)".to_string(), "```".to_string()];
        let mut cli = Cli::default();
        cli = Cli {
            command: Commands::AskAi {
                markers: Some(markers),
            },
            ..cli
        };
        let rex_markers = cli.get_markers();

        let res = split_code(&input, &rex_markers.unwrap());
        assert_eq!(res.len(), 5);
        assert_eq!(
            res.first(),
            Some(&Mark::Content {
                text: "asdf\n".to_string(),
                lang: None
            })
        );
        assert_eq!(
            res.get(1),
            Some(&Mark::Marker {
                text: "```start".to_string(),
                lang: Some("start".to_string())
            })
        );
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
        "#
        .to_string();
        let prompt: Prompt = read_config(None, &prompt_content).unwrap();
        assert_eq!(prompt.instruction, "asdf\nasdf\n".to_string());
    }

    #[test]
    fn test_convert_workflow() {
        let workflow_content = r#"
        !Workflow
        workflow:
          name1:
            tag1:
              !KeepAsIs
            tag2:
              !JumpTo
              name: name1
              tag: tag1
          name2:
             tag3:
               !PassResultTo
               name: name2
               tag: tag2
          name3:
             tag3:
               !PassResultTo
               name: name2
               tag: tag2
               concatp: true
          name4:
             tag2:
               !PassResultTo
               name: name2
               tag: tag2
               concatp: false
        "#
        .to_string();
        let workflow: Result<Workflow, _> = read_config(None, &workflow_content);
        //let workflow: Result<Workflow, _> = serde_yaml::from_str(&workflow_content);
        let workflow = workflow.unwrap();
        assert_eq!(
            workflow.get_directive("name1", "tag1"),
            Directive::KeepAsIs {}
        );
        assert_eq!(
            workflow.get_directive("name1", "tag2"),
            Directive::JumpTo {
                name: "name1".to_string(),
                tag: "tag1".to_string()
            }
        );
        assert_eq!(
            workflow.get_directive("name2", "tag3"),
            Directive::PassResultTo {
                name: "name2".to_string(),
                tag: "tag2".to_string(),
                concatp: None
            }
        );
        assert_eq!(
            workflow.get_directive("name3", "tag3"),
            Directive::PassResultTo {
                name: "name2".to_string(),
                tag: "tag2".to_string(),
                concatp: Some(true)
            }
        );
        assert_eq!(
            workflow.get_directive("name4", "tag2"),
            Directive::PassResultTo {
                name: "name2".to_string(),
                tag: "tag2".to_string(),
                concatp: Some(false)
            }
        );
    }
}
