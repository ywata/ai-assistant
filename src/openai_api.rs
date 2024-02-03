use std::future::Future;
use async_openai::{
    types::{CreateMessageRequestArgs, CreateRunRequestArgs, CreateThreadRequestArgs,
            RunStatus, MessageContent, CreateAssistantRequestArgs,
            AssistantObject, ThreadObject, },
    config::OpenAIConfig,
    error::OpenAIError,
    Client,
};

use serde::{Serialize, Deserialize};
use crate::OpenAIApiError::OpenAIAccessError;


#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum OpenAi {
    Token { token: String },
}
#[derive(Clone, Debug)]
pub enum Conversation {
    ToAi {message: String},
    FromAi {message: String},
}

impl Default for OpenAi {
    fn default() -> Self {
        OpenAi::Token {token: "".to_string()}
    }
}

#[derive(Clone, Debug)]
pub struct Context {
     client: Client<OpenAIConfig>,
     thread:ThreadObject,
     assistant: AssistantObject,
     conversation: Vec<(Conversation)>,
}

impl Context {
    pub fn new(client: Client<OpenAIConfig>, thread:ThreadObject, assistant: AssistantObject) -> Self {
        Context{client, thread, assistant, conversation:Vec::new()}
    }
    pub fn client(self) -> Client<OpenAIConfig> {
        self.client
    }
    pub fn thread(self) -> ThreadObject {
        self.thread
    }
    pub fn assistant(self) -> AssistantObject {
        self.assistant
    }
    pub fn conversation(self) -> Vec<(Conversation)> {
        self.conversation
    }

    pub fn add_conversation(&mut self, conversation: Conversation) {
        self.conversation.push(conversation);
    }
}


fn create_opeai_client(config: OpenAi) -> Client<OpenAIConfig> {
    match config {
        OpenAi::Token { token } => {
            let token = token.as_str();
            let oai_config: OpenAIConfig = OpenAIConfig::default()
                .with_api_key(token);

            //create a client
            
            Client::with_config(oai_config)
        }
    }
}

pub async fn connect(config: OpenAi, name: String, prompt: String )
                 -> Result<Context, OpenAIApiError> {
    let client = create_opeai_client(config);
    let (thread, assistant) = setup_assistant(name, &client, prompt).await?;
    let context: Context = Context{client, thread, assistant, conversation:Vec::new()};
    Ok(context)

}



async fn setup_assistant(name: String, client: &Client<OpenAIConfig>, prompt: String)
    -> Result<(ThreadObject, AssistantObject), OpenAIApiError> {
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

pub trait Saver {
    fn save(&self, out_dir:&String, text:String) -> impl Future<Output = Result<(), OpenAIApiError>> + Send;
}



pub async fn main_action<S>(client:&Client<OpenAIConfig>,
                            thread:&ThreadObject, assistant:&AssistantObject,
                            input:&String,
                            out_dir: &String,
                            saver : S)
                            -> Result<(), OpenAIApiError>
    where
        S: Saver
{
    let query = [("limit", "1")]; //limit the list responses to 1 message

    let assistant_id = &assistant.id;

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
                    .data.first().unwrap()
                    .id.clone();
                //get the message from the response
                let message = client
                    .threads()
                    .messages(&thread.id)
                    .retrieve(&message_id)
                    .await?;
                //get the content from the message
                let content = message
                    .content.first().unwrap();

                //get the text from the content
                let text = match content {
                    MessageContent::Text(text) => text.text.value.clone(),
                    MessageContent::ImageFile(_) => panic!("imaged are not supported in the terminal"),
                };
                //print the text
                println!("--- Response: {}", &text);

                saver.save(out_dir, text).await?;


            }
            RunStatus::Failed => {
                awaiting_response = false;
                println!("--- Run Failed: {:#?}", run);
            }
            otherwise => report_status(otherwise),
        }
    }


    //once we have broken from the main loop we can delete the assistant and thread
//    client.assistants().delete(assistant_id).await?;
//    client.threads().delete(&thread.id).await?;


    Ok(())
}

pub async fn ask(context: Context, input: String) -> Result<String, OpenAIApiError>
{
    let query = [("limit", "1")]; //limit the list responses to 1 message

    println!("--- User: {}", &input);
    let assistant_id = &context.assistant.id;
    let client = &context.client;
    let thread_id = &context.thread.id;

    //create a message for the thread
    let message = CreateMessageRequestArgs::default()
        .role("user")
        .content(input.clone())
        .build()?;

    //attach message to the thread
    let _message_obj = client
        .threads()
        .messages(&thread_id)
        .create(message)
        .await?;

    //create a run for the thread
    let run_request = CreateRunRequestArgs::default()
        .assistant_id(assistant_id)
        .build()?;
    let run = client
        .threads()
        .runs(&thread_id)
        .create(run_request)
        .await?;

    //wait for the run to complete
    let mut awaiting_response = true;
    while awaiting_response {
        //retrieve the run
        let run = client
            .threads()
            .runs(&thread_id)
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
                    .messages(&thread_id)
                    .list(&query)
                    .await?;
                //get the message id from the response
                let message_id = response
                    .data.first().unwrap()
                    .id.clone();
                //get the message from the response
                let message = client
                    .threads()
                    .messages(&thread_id)
                    .retrieve(&message_id)
                    .await?;
                //get the content from the message
                let content = message
                    .content.first().unwrap();

                //get the text from the content
                let text = match content {
                    MessageContent::Text(text) => text.text.value.clone(),
                    MessageContent::ImageFile(_) => panic!("imaged are not supported in the terminal"),
                };
                //print the text
                println!("--- Response: {}", &text);
                return Ok(text.clone());


            }
            RunStatus::Failed => {
                awaiting_response = false;
                println!("--- Run Failed: {:#?}", run);
            }
            otherwise => report_status(otherwise),
        }
    }


    Ok(String::from("???"))
}


pub fn report_status(status: RunStatus) {
    match status {
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
        },
        _ => panic!("should not reach here"),
    }

}

#[derive(Debug, Clone)]
pub enum OpenAIApiError {
    OpenAIAccessError

}

impl From<OpenAIError> for OpenAIApiError {
    fn from(error: OpenAIError) -> OpenAIApiError {
        dbg!(error);
        OpenAIAccessError
    }
}

impl From<std::io::Error> for OpenAIApiError {
    fn from(error: std::io::Error) -> OpenAIApiError {
        dbg!(error);
        OpenAIApiError::OpenAIAccessError
    }
}

