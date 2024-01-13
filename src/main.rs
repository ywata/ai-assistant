use std::error::Error;
use std::{fs, io, path};
use std::io::ErrorKind;
use std::path::{PathBuf};

use thiserror::Error;
use chrono;

pub mod config;

use clap::{Parser, Subcommand};
use serde::{Serialize, Deserialize};
use openai_api::{
    create_opeai_client,
    main_action,
    report_status,
    setup_assistant,
    OpenAi};

use tokio::fs::write;

use async_openai::{
    types::{CreateMessageRequestArgs, CreateRunRequestArgs, CreateThreadRequestArgs,
            RunStatus, MessageContent, CreateAssistantRequestArgs,
            AssistantObject, ThreadObject,},
    config::OpenAIConfig,
    error::OpenAIError,
    Client,
};

#[derive(Parser, Debug)]
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

#[derive(Debug, Subcommand)]
enum Commands {
    AskAi {
        #[clap(
        required = true,
        //arg_enum,
        )]
        input: String,
        #[clap(
        required = true,
        //arg_enum,
        )]
        prompt: String,
        #[clap(
        required = true,
        //arg_enum,
        )]
        output_dir: String,
        #[arg(long)]
        markers: Option<Vec<String>>
    },
    RunFs {
        #[clap(
        required = true,
        //arg_enum,
        )]
        input: String,
        #[clap(
        required = true,
        //arg_enum,
        )]
        prompt: String,
        #[clap(
        required = true,
        //arg_enum,
        )]
        output_dir: String,
        #[arg(long)]
        markers: Option<Vec<String>>
    },
}



#[derive(Error, Debug)]
pub enum AppError {
    #[error("file already exists for the directory")]
    FileExists(),
}



fn prepare_directory(dir: &str) -> io::Result<()>{
    let path = path::Path::new(dir);
    if path.exists() {
        if path.is_dir() {
            return Ok(());
        } else {
            return Err(io::Error::new(ErrorKind::Other, "file already exists"));
        }
    }

    let result = fs::create_dir(path);
    match result {
        Ok(_) => Ok(()),
        Err(err) => {
            if err.kind() != ErrorKind::AlreadyExists {
                return Err(err);
            } else {
                return Ok(());
            }
        },

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
            Commands::RunFs{input, ..} => {
                fs::read_to_string(input)
            },
        }
    }
    fn get_prompt(&self) -> io::Result<String> {
        match self {
            Commands::AskAi{prompt,..} => {
                fs::read_to_string(prompt)
            },
            Commands::RunFs{prompt, ..} => {
                fs::read_to_string(prompt)
            },
        }
    }
    fn get_output_dir(&self, dir:Option<&str>) -> Option<String>{

        let p = match self {
            Commands::AskAi{output_dir, ..} => {
                output_dir.clone()
            },
            Commands::RunFs{output_dir, ..} => {
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
            curr_pos = curr_pos + marker.len();
        } else {
            // not marker found. This might be a error.
        }

    }
    if curr_pos < max {
        result.push(Mark::Content{text: &source[curr_pos..max]});
    }
    result
}


async fn save_output(dir:&String, file:&String, text:&String, markers:&Option<Vec<String>>) -> io::Result<()> {
    if markers.is_none() {
        save_file(dir, file, text).await?
    } else {
        let contents = split_code(text, markers.as_ref().unwrap());

        let mut mark_found = false;
        for c in contents {
            match c {
                Mark::Marker{text: _} => mark_found = true,
                Mark::Content{text} => {
                    save_file(&dir, "output.fs", &text.to_string()).await?;
                    break;
                }
            }
        }
    }
    Ok(())
}


async fn save_file(dir: &str, file: &str, content: &String) -> io::Result<()> {
    let path = path::Path::new(dir);
    let path_buf = path.join(file);

    let result = tokio::fs::write(path_buf, content).await;

    result
}


async fn save_input(dir:&String, file:&String, inputs:&Vec<(&str, &String)>) -> io::Result<()> {
    println!("save_input()");
    let mut combined_input = String::new();
    for (tag, content) in inputs {
        combined_input.push_str(tag);
        combined_input.push_str(content);
    }
    save_file(dir, file, &combined_input).await?;

    Ok(())
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Cli::parse();

    let input = args.command.get_input()?;
    let instructions = args.command.get_prompt()?;

    let now = chrono::Utc::now();
    let dir_name = now.format("%Y%m%d-%H%M%S").to_string();
    let output_directory = args.command.get_output_dir(Some(&dir_name)).ok_or(io::Error::new(ErrorKind::Other, "invalid file anme"))?;

    prepare_directory(&output_directory)?;

    let config_content = fs::read_to_string(&args.yaml)?;
    let config: OpenAi = config::read_config(&args.key, &config_content)?;
    let client = create_opeai_client(config);
    let (thread, assistant) = setup_assistant(&args.name, &client, &instructions).await?;
    let assistant_id = &assistant.id;
    let query = [("limit", "1")]; //limit the list responses to 1 message
    //async fn save(dir: &String, args:&Cli, instructions:&String, input:&String, text:&String) -> Result<(), Box<dyn Error>> {
    async fn save(text:String, out_dir: String, instructions:String, input:String) -> Result<(), Box<dyn Error>> {
        println!("###### {:?}", &out_dir);
        let inputs = vec![("### prompt", &instructions), ("### input", &input)];
        save_input(&out_dir, &"input.txt".to_string(), &inputs).await?;
/*
        if let Commands::AskAi{markers, ..} = &args.command {
            save_output(&output_directory, &"output.fs".to_string(), &text, markers).await?;
        } else if let Commands::RunFs{markers, ..} = &args.command {
            save_output(&output_directory, &"output.fs".to_string(), &text, markers).await?;
        } else {
            save_file(&output_directory, "output.fs", &text).await?;
        }
        */

        Ok(())
    }

    main_action(&client, &thread, &assistant, &input,
                |text| save(text, output_directory.clone(), instructions.clone(), input.clone())).await?;
    //once we have broken from the main loop we can delete the assistant and thread
    client.assistants().delete(assistant_id).await?;
    client.threads().delete(&thread.id).await?;


    Ok(())
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