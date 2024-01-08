use std::error::Error;
use std::{fs, io, path};
use std::io::ErrorKind;
use std::path::{PathBuf};


use chrono;

pub mod config;

use clap::{Parser, Subcommand};
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
    },
}


use thiserror::Error;
#[derive(Error, Debug)]
pub enum AppError {
    #[error("file already exists for the directory")]
    FileExists(),
}

use async_openai::{
    types::{CreateMessageRequestArgs, CreateRunRequestArgs, CreateThreadRequestArgs,
            RunStatus, MessageContent, CreateAssistantRequestArgs},
    config::OpenAIConfig,
    Client,
};


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


fn save_file(dir: &str, file: &str, content: &String) -> io::Result<()> {
    let path = path::Path::new(dir);
    let path_buf = path.join(file);

    let result = fs::write(path_buf, content);

    result
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
const TEMPLATE_YAML: &str = r#"
token: Mandatory
"#;


#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Cli::parse();
    let input = args.command.get_input()?;
    let prompt = args.command.get_prompt()?;

    let now = chrono::Utc::now();
    let dir_name = &now.format("%Y%m%d-%H%M%S").to_string();
    let output_directory = args.command.get_output_dir(Some(dir_name)).ok_or(io::Error::new(ErrorKind::Other, "invalid file anme"))?;

    prepare_directory(&output_directory)?;

    let config_content = fs::read_to_string(&args.yaml)?;

    let config = config::read_config(&args.key, &TEMPLATE_YAML.to_string(), &config_content)?;
    let token = config["token"].as_str().unwrap();
    let oai_config: OpenAIConfig = OpenAIConfig::default()
        .with_api_key(token);

    // Original code is from example/assistants/src/main.rs of async-openai
    let query = [("limit", "1")]; //limit the list responses to 1 message

    //create a client
    let client = Client::with_config(oai_config);

    //create a thread for the conversation
    let thread_request = CreateThreadRequestArgs::default().build()?;
    let thread = client.threads().create(thread_request.clone()).await?;

    let assistant_name = args.name;

    let instructions = prompt;

    //create the assistant
    let assistant_request = CreateAssistantRequestArgs::default()
        .name(&assistant_name)
        .instructions(&instructions)
        .model("gpt-3.5-turbo-1106")
        .build()?;
    let assistant = client.assistants().create(assistant_request).await?;
    //get the id of the assistant
    let assistant_id = &assistant.id;


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

                    if let Commands::AskAi{..} = &args.command {
                        save_file(&output_directory, "output.fs", &text)?;
                    } else if let Commands::RunFs{..} = &args.command {
                        save_file(&output_directory, "output.fs", &text)?;
                    } else {
                        save_file(&output_directory, "output.fs", &text)?;
                    }


                    let mut combined_input = String::new();
                    combined_input.push_str("### prompt\n");
                    combined_input.push_str(&instructions);
                    combined_input.push_str("### input\n");
                    combined_input.push_str(&input);

                    save_file(&output_directory, "input.txt", &combined_input)?;

                }
                RunStatus::Failed => {
                    awaiting_response = false;
                    println!("--- Run Failed: {:#?}", run);
                }
                RunStatus::Queued => {
                    println!("--- Run Queued");
                },
                RunStatus::Cancelling => {
                    println!("--- Run Cancelling");
                },
                RunStatus::Cancelled => {
                    println!("--- Run Cancelled");
                },
                RunStatus::Expired => {
                    println!("--- Run Expired");
                },
                RunStatus::RequiresAction => {

                    println!("--- Run Requires Action");
                },
                RunStatus::InProgress => {
                    println!("--- Waiting for response...");
                }
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
