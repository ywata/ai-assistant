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


fn save_output(dir:&String, file:&String, text:&String, markers:&Option<Vec<String>>) -> io::Result<()> {
    if markers.is_none() {
        save_file(dir, file, text)?;
    } else {
        let contents = split_code(text, markers.as_ref().unwrap());

        let mut mark_found = false;
        for c in contents {
            match c {
                Mark::Marker{text: _} => mark_found = true,
                Mark::Content{text} => {
                    save_file(&dir, "output.fs", &text.to_string())?;
                    break;
                }
            }
        }
    }
    Ok(())
}


fn save_file(dir: &str, file: &str, content: &String) -> io::Result<()> {
    let path = path::Path::new(dir);
    let path_buf = path.join(file);

    let result = fs::write(path_buf, content);

    result
}


fn save_input(dir:&String, file:&String, inputs:&Vec<(&str, &String)>) -> io::Result<()> {
    let mut combined_input = String::new();
    for (tag, content) in inputs {
        combined_input.push_str(tag);
        combined_input.push_str(content);
    }
    save_file(dir, file, &combined_input)
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
    //main_action(config, &instructions, &client, &thread, &assistant, None);


    loop{
        //create a message for the thread
        let message = CreateMessageRequestArgs::default()
            .role("user")
            .content(input.clone())
            .build()?;

        //attach message to the thread
        let _message_obj = client
            .threads()
            .messages(&thread.id)
            .create(message)
            .await?;

        //create a run for the thread
        let run_request = CreateRunRequestArgs::default()
            .assistant_id(assistant_id)
            .build()?;
        let run = client
            .threads()
            .runs(&thread.id)
            .create(run_request)
            .await?;

        //wait for the run to complete
        let mut awaiting_response = true;
        while awaiting_response {
            //retrieve the run
            let run = client
                .threads()
                .runs(&thread.id)
                .retrieve(&run.id)
                .await?;
            //check the status of the run
            match run.status {
                RunStatus::Completed => {
                    awaiting_response = false;
                    // once the run is completed we
                    // get the response from the run
                    // which will be the first message
                    // in the thread

                    //retrieve the response from the run
                    let response = client
                        .threads()
                        .messages(&thread.id)
                        .list(&query)
                        .await?;
                    //get the message id from the response
                    let message_id = response
                        .data.get(0).unwrap()
                        .id.clone();
                    //get the message from the response
                    let message = client
                        .threads()
                        .messages(&thread.id)
                        .retrieve(&message_id)
                        .await?;
                    //get the content from the message
                    let content = message
                        .content.get(0).unwrap();

                    //get the text from the content
                    let text = match content {
                        MessageContent::Text(text) => text.text.value.clone(),
                        MessageContent::ImageFile(_) => panic!("imaged are not supported in the terminal"),
                    };
                    //print the text
                    println!("--- Response: {}", &text);
                    println!("{:?}", &output_directory);

                    if let Commands::AskAi{markers, ..} = &args.command {
                        save_output(&output_directory, &"output.fs".to_string(), &text, markers)?;
                    } else if let Commands::RunFs{markers, ..} = &args.command {
                        save_output(&output_directory, &"output.fs".to_string(), &text, markers)?;
                    } else {
                        save_file(&output_directory, "output.fs", &text)?;
                    }

                    let input_pair = vec![("### prompt\n", &instructions), ("### input\n", &input)];
                    save_input(&output_directory, &"input.txt".to_string(), &input_pair)?;

                },
                RunStatus::Failed => {
                    awaiting_response = false;
                    println!("--- Run Failed: {:#?}", run);
                }
                otherwise => report_status(otherwise),
            }
            //wait for 1 second before checking the status again
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        break;
    }

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