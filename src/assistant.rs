use std::{fs, io};
use std::collections::HashMap;
use std::path::{PathBuf};
use std::sync::{Arc};

use std::process::Output;
use regex::{Regex};

use iced::widget::{self, Button, Text, column, horizontal_space, row, text_editor};
use iced::{
    Alignment, Application, Command, Element, Settings, Theme,
};

use thiserror::Error;

use clap::{Parser, Subcommand};
use serde::Deserialize;
use tokio::sync::Mutex;

use openai_api::{connect, Context, Conversation, OpenAi, OpenAIApiError};

use crate::compile::compile;
use openai_api::scenario::{Prompt, Workflow, Directive};

//use thiserror::Error;
pub mod config;
mod compile;

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
            _ => Ok(vec![Regex::new("").unwrap()]),
        };
        res
    }
}

impl Default for Cli {
    fn default() -> Self {
        Cli {config_file:"service.yaml".to_string(),
            config_key:"openai".to_string(),
            prompt_file: "prompt.txt".to_string(),
            prompt_keys: Vec::default(),
            workflow_file: None,
            output_dir: "output".to_string(),
            tag: "default".to_string(),
            command: Commands::default(),
        }
    }
}



pub fn main() -> Result<(), AssistantError> {
    let args = Cli::parse();
    println!("{:?}", args);
    let config_content = fs::read_to_string(&args.config_file)?;
    let config: OpenAi = config::read_config(Some(&args.config_key), &config_content)?;
    let prompt_content = fs::read_to_string(&args.prompt_file)?;
    let prompts = config::read_config(None, &prompt_content)?;
    let _markers = args.get_markers()?;
    let workflow = if let Some(ref file) = args.workflow_file {
        let workflow_content = fs::read_to_string(&file)?;
        config::read_config(None, &workflow_content)?
    } else {
        Workflow::default()
    };

    let settings = Settings::default();
    let updated_settings = Settings {
        flags: (args, config, prompts, workflow),
        ..settings
    };

    Ok(Model::run(updated_settings)?)

}





#[derive(Clone, Debug)]
enum Message {
    Connected(Result<Context, OpenAIApiError>),
    LoadInput {name:String, tag: String},
    PassResult{name:String, tag: String},
    InputLoaded(Option<LoadedInput>),
    OpenFile(AreaIndex),
    FileOpened(Result<(AreaIndex, (PathBuf, Arc<String>)), (AreaIndex, Error)>),
    ActionPerformed(AreaIndex, text_editor::Action),
    QueryAi {name: String, tag: String},
    Answered(Result<(String, String), (String, OpenAIApiError)>),
    Compiled(Result<Output, AssistantError>),
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
enum AreaIndex{
    Prompt = 0,
    Input = 1,
    Result = 2,
}

#[derive(Debug)]
struct Model {
    env: Cli,
    prompts: HashMap<String, openai_api::scenario::Prompt>,
    context: Option<Arc<Mutex<Context>>>,
    edit_areas: Vec<EditArea>,
    current: (String, String),
    workflow: Workflow,
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

#[derive(Debug, Clone)]
struct LoadedInput {
    prompt: String,
    prefix: Option<String>,
    input: String,
}
async fn load_input(prompt: Prompt, tag: String) -> Option<LoadedInput> {
    println!("tag:{:?}", &tag);

    prompt.inputs.iter().find(|i| i.tag == tag)
        .map(|i| (LoadedInput{prompt:prompt.instruction.clone(),
            prefix: i.prefix.clone(),
            input: i.text.clone()}))
}

fn get_content(contents: Vec<Mark>) -> Option<Mark> {
    let mut res = None;
    for c in contents {
        if let Mark::Content {..} = c {
            res = Some(c);
            break;
        }
    }
    res
}

impl Application for Model {
    type Message = Message;
    type Theme = Theme;
    type Executor = iced::executor::Default;
    type Flags = (Cli, OpenAi, HashMap<String, openai_api::scenario::Prompt>, Workflow);

    fn  new(flags: <Model as iced::Application>::Flags) -> (Model, Command<Message>) {
        let name = flags.0.prompt_keys.first().unwrap();
        let tag = flags.0.tag.clone();
        let prompt = EditArea::default();
        let input = EditArea::default();
        let result = EditArea::default();
        let commands = vec![
            Command::perform(connect(flags.1.clone(), flags.0.prompt_keys.clone(), flags.2.clone()),
                Message::Connected),
            Command::perform(load_input(flags.2.get(name).unwrap().clone(), tag.clone()),
                Message::InputLoaded)];

        (Model {
            env: flags.0.clone(),
            prompts: flags.2.clone(),
            context: None,
            edit_areas: vec![prompt, input, result],
            current:(name.clone(), tag.clone()),
            workflow: flags.3,
        },
         Command::batch(commands))
    }

    fn title(&self) -> String {
        "title".to_string()
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Connected(Ok(ctx)) => {
                println!("Connected");
                self.context = Some(Arc::new(Mutex::new(ctx)));
                Command::none()
            },
            Message::OpenFile(_idx) => {
                Command::none()
            },
            Message::Connected(Err(_)) => {
                Command::none()
            },
            Message::LoadInput{name, tag} => {
                // A bit too early, but let's do it for now.
                self.current = (name.clone(), tag.clone());
                let prompt = self.prompts.get(&name).unwrap().clone();
                Command::perform(load_input(prompt, tag), Message::InputLoaded)
            },
            Message::PassResult{name, tag} => {
                Command::perform(load_input(self.prompts.get(&name).unwrap().clone(), tag), Message::InputLoaded)
            },
            Message::InputLoaded(Some(LoadedInput{prompt, prefix, input:text})) => {
                let default = EditArea::default();
                let prefixed_text = prefix.unwrap_or_default() + "\n" + &text;
                self.edit_areas[AreaIndex::Input as usize] = EditArea{
                    content: text_editor::Content::with_text(&prefixed_text),
                    ..default
                };
                let default = EditArea::default();
                self.edit_areas[AreaIndex::Prompt as usize] = EditArea{
                    content: text_editor::Content::with_text(&prompt),
                    ..default
                };
                Command::none()
            },
            | Message::InputLoaded(None) => {
                println!("InputLoade:None");
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
            Message::QueryAi {name,..} => {
                if let Some(context) = self.context.clone() {
                    let pass_context = context.clone();
                    let pass_name = name.clone();
                    let input = self.edit_areas[AreaIndex::Input as usize].content.text();
                    let _handle = tokio::spawn(async move {
                        let mut ctx = context.lock().await;
                        ctx.add_conversation(name.clone(), Conversation::ToAi { message: input })
                    });

                    Command::perform(openai_api::ask(pass_context, pass_name,
                                                     self.edit_areas[AreaIndex::Input as usize].content.text()),
                                     Message::Answered)

                } else {
                    Command::none()
                }
            },
            Message::Answered(res) => {
                println!("{:?}", res);
                let mut command = Command::none();
                match res {
                    Ok((name, text)) =>{
                        println!("Ok");
                        let opt_markers = self.env.get_markers();
                        let mut content = text_editor::Content::with_text("");
                        let context = self.context.clone().unwrap();
                        let cloned_text = text.clone();
                        let _handle = tokio::spawn(async move {
                            let mut ctx = context.lock().await;
                            ctx.add_conversation(name, Conversation::FromAi { message: cloned_text })
                        });

                        if let Ok(markers) = opt_markers {
                            println!("Ok:markers:{:?}", markers);
                            let contents = split_code(&text, &markers.clone()).clone();
                            let json = String::from("json");
                            let fsharp = String::from("fsharp");
                            //let text = String::from("text");

                            if let Some(Mark::Content{text, lang: Some(matcher)}) = get_content(contents) {
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
                            } else {
                                println!("No: contents");
                                content = text_editor::Content::with_text(&text);
                            }
                        } else {
                            println!("No: markers");
                            content = text_editor::Content::with_text(&text);
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
            Message::DoNothing => {
                Command::none()
            },
        }
    }

    fn view(&self) -> Element<Message> {
        let  vec = &self.edit_areas;
        column![
            row![
                row(
                    list_inputs(&self.prompts).into_iter()
                    .map(|(name, tag)|
                        button(name.clone(), tag.clone())
                        .on_press(Message::LoadInput{name, tag})
                        .into())),
                row![
                    horizontal_space(iced::Length::Fill),
                    button("Next".to_string(), "".to_string())
                      .on_press(load_message(&self.workflow, &self.current.0, &self.current.1)),
                    button("Ask AI".to_string(), "".to_string())
                        .on_press(Message::QueryAi{name: self.current.0.clone(), tag: self.current.1.clone()})
                    ].align_items(Alignment::End)
                .width(iced::Length::Fill),

                ],
            row![
                column![
/*                    text_editor(&vec.get(AreaIndex::Prompt as usize).unwrap().content)
                    .on_action(|action|Message::ActionPerformed(AreaIndex::Prompt, action)),
 */
                    text_editor(&vec.get(AreaIndex::Input as usize).unwrap().content)
                    .on_action(|action|Message::ActionPerformed(AreaIndex::Input, action)),
                ],
                column![
                    text_editor(&vec.get(AreaIndex::Result as usize).unwrap().content)
                    //.on_action(|action|Message::ActionPerformed(AreaIndex::Result, action)),
                ],
            ],
        ].into()
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

async fn load_file<T: Copy>(idx: T, path: PathBuf) -> Result<(T, (PathBuf, Arc<String>)), (T, Error)> {
    let contents = tokio::fs::read_to_string(&path)
        .await
        .map(Arc::new)
        .map_err(|error| (idx, Error::IoError(error.kind())))?;

    Ok((idx, (path.clone(), contents)))
}

fn load_message(wf: &Workflow, name: &str, tag: &str) -> Message {
    let directive = wf.get_directive(name, tag);
    match directive {
        Directive::KeepAsIs => Message::DoNothing,
        Directive::JumpTo { name, tag } =>
            Message::LoadInput { name: name.to_string(), tag: tag.to_string() },
        Directive::PassResultTo { name, tag } =>
            Message::PassResult { name: name.to_string(), tag: tag.to_string() },
    }
}

#[derive(Clone, Debug, Deserialize)]
struct Response {
    missing: Vec<String>,
    possible: Vec<String>,
}



#[derive(Debug, Clone)]
enum Error {
    APIError,
    IoError(io::ErrorKind),
}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Error {
        dbg!(error);

        Error::APIError
    }
}

fn button<'a>(text: String, tag: String) -> widget::Button<'a, Message> {
    let title = text.clone() + ":" + &tag;
    Button::new(Text::new(title))
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
        "#.to_string();
        let workflow: Result<Workflow, _> = read_config(None, &workflow_content);
        //let workflow: Result<Workflow, _> = serde_yaml::from_str(&workflow_content);
        let workflow = workflow.unwrap();
        assert_eq!(workflow.get_directive("name1", "tag1"), Directive::KeepAsIs{});
        assert_eq!(workflow.get_directive("name1", "tag2"), Directive::JumpTo{name: "name1".to_string(), tag: "tag1".to_string()});
        assert_eq!(workflow.get_directive("name2", "tag3"), Directive::PassResultTo{name: "name2".to_string(), tag: "tag2".to_string()});
    }
}