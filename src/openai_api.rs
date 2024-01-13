use std::error::Error;
use async_openai::{
    types::{CreateMessageRequestArgs, CreateRunRequestArgs, CreateThreadRequestArgs,
            RunStatus, MessageContent, CreateAssistantRequestArgs,
            AssistantObject, ThreadObject,},
    config::OpenAIConfig,
    error::OpenAIError,
    Client,
};

use thiserror::Error;
use serde::{Serialize, Deserialize};


#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum OpenAi{
    Token{token: String},
}


pub fn create_opeai_client(config:OpenAi) -> Client<OpenAIConfig>{
    match config {
        OpenAi::Token{token} => {
            let token = token.as_str();
            let oai_config: OpenAIConfig = OpenAIConfig::default()
                .with_api_key(token);

            //create a client
            let client = Client::with_config(oai_config);
            return client;
        }
    }
}


pub async fn setup_assistant(name:&String, client: &Client<OpenAIConfig>, prompt:&String) -> Result<(ThreadObject, AssistantObject),Box<dyn Error>> {
    //create a thread for the conversation
    let thread_request = CreateThreadRequestArgs::default().build()?;
    let thread = client.threads().create(thread_request.clone()).await?;

    let assistant_name = name;
    let instructions = prompt;

    //create the assistant
    let assistant_request = CreateAssistantRequestArgs::default()
        .name(assistant_name)
        .instructions(instructions)
        .model("gpt-3.5-turbo-1106")
        .build()?;
    let assistant = client.assistants().create(assistant_request).await?;
    //get the id of the assistant

    Ok((thread, assistant))
}
