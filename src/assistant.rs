use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::{fs, io};

use async_openai::config::{AzureConfig, Config, OpenAIConfig};
use async_openai::Client;
use regex::Regex;
use std::process::Output;

use iced::widget::{
    self, checkbox, column, horizontal_space, row, text_editor, Button, Column, Text,
};
use iced::{Alignment, Application, Command, Element, Settings, Theme};

use thiserror::Error;

use clap::{Parser, Subcommand};
use log::{debug, error, info, trace};
use serde::Deserialize;
use tokio::sync::Mutex;

use openai_api::{connect, AiServiceApi, Context, Conversation, OpenAIApiError, OpenAi};

use crate::compile::compile;
use openai_api::scenario::{parse_cli_settings, parse_scenario, Directive, Prompt, Workflow};

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
    prompt_keys: Vec<String>,
    #[arg(long)]
    workflow_file: Option<String>,
    #[arg(long)]
    output_dir: String,
    #[arg(long)]
    tag: String,
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
            prompt_keys: Vec::default(),
            workflow_file: None,
            output_dir: "output".to_string(),
            tag: "default".to_string(),
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
    let _given_keys = parse_cli_settings(&prompt_hash, &args.prompt_keys, &args.tag)
        .ok_or(AssistantError::AppAccessError)?;

    if let Some((prompts, workflow)) = parse_scenario(prompt_hash, wf) {
        #[cfg(not(feature = "azure_ai"))]
        let client: Option<Client<OpenAIConfig>> = config.create_client();
        #[cfg(feature = "azure_ai")]
        let client: Option<Client<AzureConfig>> = config.create_client();

        let settings_default = Settings {
            flags: (args.clone(), config, prompts, workflow, args.auto, client),
            ..Default::default()
        };
        //let updated_settings = Settings {
        //    flags: (args.clone(), config, prompts, workflow, args.auto, client),
        //    ..settings
        //};

        Ok(Model::run(settings_default)?)
    } else {
        Err(AssistantError::AppAccessError)
    }
}

#[derive(Clone, Debug)]
enum Message<C: Config> {
    Connected(Result<Context<C>, OpenAIApiError>),
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
    Toggled(bool),
    DoNothing,
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

#[derive(Debug, Copy, Clone)]
enum AreaIndex {
    Prompt = 0,
    Input = 1,
    Result = 2,
}

#[derive(Debug)]
struct Model<C: Config> {
    env: Cli,
    prompts: HashMap<String, openai_api::scenario::Prompt>,
    context: Option<Arc<Mutex<Context<C>>>>,
    // edit_area contaiins current view of conversation,
    // Prompt is set up on Thread creation time and it is not changed.
    // Input edit_area will be used for querying to AI.
    // Result edit_area will be used for displaying the result of AI.
    edit_areas: Vec<EditArea>,
    current: (String, String),
    workflow: Workflow,
    auto: Option<usize>,
}

impl<C: Config> Model<C> {
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
        return false;
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
fn load_data(prompt: Prompt, tag: &str) -> Option<LoadedData> {
    debug!("load_data(): prompt:{:?} tag:{}", &prompt, &tag);

    prompt.inputs.iter().find(|i| i.tag == tag).map(|i| {
        LoadedData {
            prompt: prompt.instruction.clone(),
            prefix: i.prefix.clone(),
            input: i.text.clone(),
        }
    })
}

fn load_content(prompt: Prompt, tag: &str, edit_area: &EditArea) -> Option<LoadedData> {
    debug!("load_content(): prompt:{:?} tag:{}", &prompt, &tag);
    prompt.inputs.iter().find(|i| i.tag == tag).map(|i| {
        LoadedData {
            prompt: prompt.instruction.clone(),
            prefix: i.prefix.clone(),
            input: edit_area.content.text().clone(),
        }
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
        content: text_editor::Content::with_text(&text),
        ..default
    };
}

impl<C: Config + std::fmt::Debug + Send + Sync + 'static> Application for Model<C>
where
    OpenAi: AiServiceApi<C>,
{
    type Message = Message<C>;
    type Theme = Theme;
    type Executor = iced::executor::Default;
    type Flags = (
        Cli,
        OpenAi,
        HashMap<String, openai_api::scenario::Prompt>,
        Workflow,
        Option<usize>,
        Option<async_openai::Client<C>>,
    );

    fn new(flags: <Model<C> as iced::Application>::Flags) -> (Model<C>, Command<Message<C>>) {
        let name = flags.0.prompt_keys.first().unwrap();
        let tag = flags.0.tag.clone();
        let mut prompt = EditArea::default();
        let mut input = EditArea::default();
        let result = EditArea::default();

        // As OpenAICOnfig and AzureConfig cannot co-exists in a generic way,
        // we need to use conditional compilation.
        // It might be good to place this in main() but it introduces a lot of
        // conditional compilation.
        #[cfg(not(azure_ai))]
        //let client = create_opeai_client(&flags.1);
        let client = (&flags.1).create_client();
        #[cfg(azure_ai)]
        let client = create_opeai_client(&flags.1);
        let loaded = load_data(flags.2.get(name).unwrap().clone(), &tag);
        // Initialize EditArea with loaded input.
        if let Some(i) = loaded {
            let prefixed_text = i.prefix.unwrap_or_default() + "\n" + &i.input;
            input.content = text_editor::Content::with_text(&prefixed_text);
            prompt.content = text_editor::Content::with_text(&i.prompt);
        }
        let commands: Vec<Command<Message<C>>> = vec![Command::perform(
            connect(
                flags.1.clone(),
                client.unwrap(),
                flags.0.prompt_keys.clone(),
                flags.2.clone(),
            ),
            Message::Connected,
        )];

        (
            Model {
                env: flags.0.clone(),
                prompts: flags.2.clone(),
                context: None,
                edit_areas: vec![prompt, input, result],
                current: (name.clone(), tag.clone()),
                workflow: flags.3,
                auto: flags.0.auto,
            },
            Command::<Message<C>>::batch(commands),
        )
    }

    fn title(&self) -> String {
        "title".to_string()
    }

    fn update(&mut self, message: Message<C>) -> Command<Message<C>> {
        debug!("{:?}", message);
        match message {
            Message::Connected(Ok(ctx)) => {
                info!("Connected");
                self.context = Some(Arc::new(Mutex::new(ctx)));
                Command::none()
            }
            Message::Connected(Err(_)) => Command::none(),

            Message::NextWorkflow { auto } => {
                let wf = &self.workflow;
                let name = self.current.0.clone();
                let tag = self.current.1.clone();
                let mut do_next = false;
                info!("current: name:{}, tag:{}", &name, &tag);
                let _ = match wf.get_directive(&name, &tag) {
                    Directive::KeepAsIs => (),
                    Directive::Stop => (),

                    Directive::JumpTo { name, tag } => {
                        info!("JumpTo: name:{}, tag:{}", name, tag);

                        let loaded = load_data(self.prompts.get(&name).unwrap().clone(), &tag);
                        if let Some(i) = loaded {
                            self.current = (name.clone(), tag.clone());
                            let prefixed_text = i.prefix.unwrap_or_default() + "\n" + &i.input;
                            set_editor_contents(
                                &mut self.edit_areas,
                                AreaIndex::Input,
                                &prefixed_text,
                            );
                            set_editor_contents(&mut self.edit_areas, AreaIndex::Prompt, &i.prompt);
                            do_next = true;
                        }
                    }
                    Directive::PassResultTo { name, tag } => {
                        info!("PassResultTo: name:{}, tag:{}", name, tag);
                        self.current = (name.clone(), tag.clone());
                        let loaded = load_content(
                            self.prompts.get(&name).unwrap().clone(),
                            &tag,
                            &self.edit_areas[AreaIndex::Result as usize],
                        );
                        if let Some(i) = loaded {
                            let prefixed_text = i.prefix.unwrap_or_default() + "\n" + &i.input;
                            self.current = (name.clone(), tag.clone());
                            set_editor_contents(
                                &mut self.edit_areas,
                                AreaIndex::Input,
                                &prefixed_text,
                            );
                            set_editor_contents(&mut self.edit_areas, AreaIndex::Prompt, &i.prompt);
                            do_next = true;
                        }
                    }
                };
                if auto && do_next {
                    let pass_context = self.context.clone().unwrap();
                    let pass_name = name.clone();
                    Command::perform(
                        openai_api::ask(
                            pass_context,
                            pass_name,
                            self.edit_areas[AreaIndex::Input as usize].content.text(),
                        ),
                        move |answer| Message::Answered {
                            answer,
                            auto: auto.clone(),
                        },
                    )
                } else {
                    Command::none()
                }
            }

            Message::LoadInput { name, tag } => {
                // A bit too early, but let's do it for now.
                self.current = (name.clone(), tag.clone());
                let prompt = self.prompts.get(&name).unwrap().clone();
                //Command::perform(load_input(prompt, tag), Message::InputLoaded)
                let result = load_data(prompt, &tag)
                    .map(|i| {
                        let input = i.input.clone();
                        let prefix = i.prefix.clone();
                        let prefixed_text = prefix.unwrap_or_default() + "\n" + &input;
                        set_editor_contents(&mut self.edit_areas, AreaIndex::Input, &prefixed_text);

                        let prompt = i.prompt.clone();
                        set_editor_contents(&mut self.edit_areas, AreaIndex::Prompt, &prompt);
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
                    let _handle = tokio::spawn(async move {
                        let mut ctx = context.lock().await;
                        ctx.add_conversation(&name, Conversation::ToAi { message: input })
                    });

                    Command::perform(
                        openai_api::ask(
                            pass_context,
                            pass_name,
                            self.edit_areas[AreaIndex::Input as usize].content.text(),
                        ),
                        move |answer| Message::Answered {
                            answer,
                            auto: auto.clone(),
                        },
                    )
                } else {
                    Command::none()
                }
            }
            Message::Answered { answer, auto } => {
                info!("{:?}", answer);
                let _ = self.dec_auto();

                let mut command = Command::none();

                match answer {
                    Ok((name, text)) => {
                        let opt_markers = self.env.get_markers();
                        let mut content = text_editor::Content::with_text("");
                        let context = self.context.clone().unwrap();
                        let cloned_text = text.clone();
                        let _handle = tokio::spawn(async move {
                            let mut ctx = context.lock().await;
                            ctx.add_conversation(
                                &name,
                                Conversation::FromAi {
                                    message: cloned_text,
                                },
                            )
                        });

                        if let Ok(markers) = opt_markers {
                            let contents = split_code(&text, &markers.clone()).clone();
                            let json = String::from("json");
                            let fsharp = String::from("fsharp");
                            //let text = String::from("text");

                            if let Some(Mark::Content {
                                text,
                                lang: Some(matcher),
                            }) = get_content(contents)
                            {
                                if matcher == json {
                                    let response = serde_json::from_str::<Response>(&text);
                                    if let Ok(_resp) = response {
                                        content = text_editor::Content::with_text(&text);
                                    }
                                } else if matcher == fsharp {
                                    content = text_editor::Content::with_text(&text);
                                    let mut path = PathBuf::from(&self.env.output_dir);
                                    path.push("sample.fs");
                                    command = Command::perform(
                                        save_and_compile(path, text),
                                        Message::Compiled,
                                    );
                                } else {
                                    //
                                }
                            } else {
                                trace!("No splitted contents: {}", &text);
                                content = text_editor::Content::with_text(&text);
                            }
                        } else {
                            trace!("No: markers");
                            content = text_editor::Content::with_text(&text);
                        }

                        let default = EditArea::default();
                        self.edit_areas[AreaIndex::Result as usize] =
                            EditArea { content, ..default };
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
            Message::Toggled(_) => {
                Command::none()
            }
            Message::SaveConversation { outut_dir } => {
                let context = self.context.clone().unwrap();
                let _handle = tokio::spawn(async move {
                    let ctx = context.lock().await;
                    ctx.save_conversation(&outut_dir)
                });
                Command::none()
            }
            Message::DoNothing => Command::none(),
        }
    }

    fn view(&self) -> Element<Message<C>> {
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
                    button(&"Next", &"").on_press(Message::NextWorkflow {
                        auto: self.auto_enabled()
                    }),
                    button(&"Ask AI", &"").on_press(Message::QueryAi {
                        name: self.current.0.clone(),
                        tag: self.current.1.clone(),
                        auto: false,
                    }),
                    button(&"auto", &"").on_press(Message::QueryAi {
                        name: self.current.0.clone(),
                        tag: self.current.1.clone(),
                        auto: true,
                    }),
                    button(&"save", &"").on_press(Message::SaveConversation {
                        outut_dir: self.env.output_dir.clone(),
                    }),
                ]
                .align_items(Alignment::End)
                .width(iced::Length::Fill),
            ],
            row![
                column![
                    /*                    text_editor(&vec.get(AreaIndex::Prompt as usize).unwrap().content)
                                       .on_action(|action|Message::ActionPerformed(AreaIndex::Prompt, action)),
                    */
                    text_editor(&vec.get(AreaIndex::Input as usize).unwrap().content)
                        .on_action(|action| Message::ActionPerformed(AreaIndex::Input, action)),
                ],
                column![
                    text_editor(&vec.get(AreaIndex::Result as usize).unwrap().content) //.on_action(|action|Message::ActionPerformed(AreaIndex::Result, action)),
                ],
            ],
        ]
        .into()
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


#[derive(Clone, Debug, Deserialize)]
struct Response {
    missing: Vec<String>,
    possible: Vec<String>,
}

impl From<reqwest::Error> for AssistantError {
    fn from(error: reqwest::Error) -> AssistantError {
        dbg!(error);

        AssistantError::APIError
    }
}




fn button<'a, C: Config>(text: &str, tag: &str) -> widget::Button<'a, Message<C>> {
    let title = text.to_string() + ":" + tag;
    Button::new(Text::new(title))
}

fn to_checkboxes<'a, C: Config + 'a>(resp: HashMap<String, Vec<String>>) -> iced::widget::Column<'a, Message<C>> {
    let mut col = Column::new();
    for (k, v) in resp {
        for i in v {
            let cb = checkbox("default", false)
                .on_toggle(Message::Toggled);
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
                tag: "tag2".to_string()
            }
        );
    }
}
